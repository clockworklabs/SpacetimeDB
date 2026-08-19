import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { request as httpRequest, createServer } from 'node:http';
import test from 'node:test';

import { createCredentialBroker, startCredentialBroker,
  stopCredentialBroker } from '../container/credential-broker.mjs';

const listen = server => new Promise((resolve, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', () => resolve(server.address().port));
});

const close = server => new Promise((resolve, reject) => {
  server.close(error => error ? reject(error) : resolve());
});

function send(port, { method = 'POST', path = '/v1/messages', headers = {}, body = '{}' } = {}) {
  return new Promise((resolve, reject) => {
    const request = httpRequest({ hostname: '127.0.0.1', port, method, path, headers }, response => {
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => resolve({ status: response.statusCode,
        body: Buffer.concat(chunks).toString('utf8') }));
    });
    request.once('error', reject);
    request.end(body);
  });
}

async function withBroker(mode, body) {
  const seen = [];
  const upstreamServer = createServer((request, response) => {
    const chunks = [];
    request.on('data', chunk => chunks.push(chunk));
    request.on('end', () => {
      seen.push({ method: request.method, url: request.url, headers: request.headers,
        body: Buffer.concat(chunks).toString('utf8') });
      response.writeHead(200, { 'content-type': 'application/json' });
      response.end('{"ok":true}');
    });
  });
  const upstreamPort = await listen(upstreamServer);
  const sessionToken = 'session-token-value-1234567890';
  const credential = 'provider-secret-value-1234567890';
  const { server, stats } = createCredentialBroker({ mode, credential, sessionToken }, {
    requestUpstream: httpRequest,
    upstream: { protocol: 'http:', hostname: '127.0.0.1', port: upstreamPort },
  });
  const brokerPort = await listen(server);
  try { await body({ brokerPort, sessionToken, credential, seen, stats }); }
  finally { await close(server); await close(upstreamServer); }
}

test('credential broker rejects unauthorized and unsupported requests before upstream', async () => {
  await withBroker('api-key', async ({ brokerPort, sessionToken, seen, stats }) => {
    assert.equal((await send(brokerPort)).status, 401);
    assert.equal((await send(brokerPort, { method: 'GET',
      headers: { authorization: `Bearer ${sessionToken}` } })).status, 404);
    assert.equal(seen.length, 0);
    assert.equal(stats().acceptedRequests, 0);
  });
});

for (const mode of ['api-key', 'subscription-token']) {
  test(`credential broker replaces the session credential for ${mode}`, async () => {
    await withBroker(mode, async ({ brokerPort, sessionToken, credential, seen, stats }) => {
      const result = await send(brokerPort, {
        path: '/v1/messages?beta=true',
        headers: { authorization: `Bearer ${sessionToken}`, 'content-type': 'application/json' },
        body: '{"model":"test"}',
      });
      assert.deepEqual(result, { status: 200, body: '{"ok":true}' });
      assert.equal(seen.length, 1);
      assert.equal(seen[0].url, '/v1/messages?beta=true');
      assert.equal(seen[0].body, '{"model":"test"}');
      if (mode === 'api-key') {
        assert.equal(seen[0].headers['x-api-key'], credential);
        assert.equal(seen[0].headers.authorization, undefined);
      } else {
        assert.equal(seen[0].headers.authorization, `Bearer ${credential}`);
        assert.equal(seen[0].headers['x-api-key'], undefined);
      }
      assert.doesNotMatch(JSON.stringify(seen[0].headers), new RegExp(sessionToken));
      assert.equal(stats().acceptedRequests, 1);
    });
  });
}

test('credential broker enforces request size and count limits', async () => {
  const seen = [];
  const upstreamServer = createServer((request, response) => {
    request.resume();
    request.on('end', () => { seen.push(request.url); response.end('ok'); });
  });
  const upstreamPort = await listen(upstreamServer);
  const sessionToken = 'session-token-value-1234567890';
  const { server } = createCredentialBroker({ mode: 'api-key',
    credential: 'provider-secret-value-1234567890', sessionToken }, {
    requestUpstream: httpRequest,
    upstream: { protocol: 'http:', hostname: '127.0.0.1', port: upstreamPort },
    maxRequests: 2,
    maxRequestBytes: 4,
  });
  const brokerPort = await listen(server);
  const headers = { authorization: `Bearer ${sessionToken}` };
  try {
    assert.equal((await send(brokerPort, { headers, body: '12345' })).status, 413);
    assert.equal((await send(brokerPort, { headers, body: '1' })).status, 200);
    assert.equal((await send(brokerPort, { headers, body: '1' })).status, 429);
    assert.deepEqual(seen, ['/v1/messages']);
  } finally {
    await close(server);
    await close(upstreamServer);
  }
});

test('credential broker lifecycle creates only an attempt-scoped session credential', async () => {
  const providerCredential = 'provider-secret-value-1234567890';
  const broker = startCredentialBroker({ mode: 'api-key',
    environment: { name: 'ANTHROPIC_API_KEY', value: providerCredential }, mount: null },
  { networkMode: 'host', deadlineMs: 10_000 });
  try {
    assert.equal(existsSync(broker.root), true);
    assert.doesNotMatch(broker.baseUrl, new RegExp(providerCredential));
    assert.match(broker.sessionToken, /^[a-f0-9]{64}$/);
    assert.equal((await send(new URL(broker.baseUrl).port)).status, 401);
  } finally {
    stopCredentialBroker(broker);
  }
  assert.equal(existsSync(broker.root), false);
});
