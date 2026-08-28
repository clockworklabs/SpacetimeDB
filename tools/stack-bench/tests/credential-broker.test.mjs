import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { request as httpRequest, createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { gzipSync } from 'node:zlib';

import { createCredentialBroker, readCredentialBrokerLedger,
  reconcileCredentialBrokerReceipt, startCredentialBroker,
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

async function withBroker(mode, body, config = {}) {
  const { upstreamBody = '{"ok":true}', upstreamHeaders = {}, ...brokerConfig } = config;
  const seen = [];
  const upstreamServer = createServer((request, response) => {
    const chunks = [];
    request.on('data', chunk => chunks.push(chunk));
    request.on('end', () => {
      seen.push({ method: request.method, url: request.url, headers: request.headers,
        body: Buffer.concat(chunks).toString('utf8') });
      response.writeHead(200, { 'content-type': 'application/json', ...upstreamHeaders });
      response.end(upstreamBody);
    });
  });
  const upstreamPort = await listen(upstreamServer);
  const sessionToken = 'session-token-value-1234567890';
  const credential = 'provider-secret-value-1234567890';
  const { server, stats } = createCredentialBroker({ mode, credential, sessionToken,
    model: 'test-model', maxOutputTokens: 4096, ...brokerConfig }, {
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
      headers: { authorization: `Bearer ${sessionToken}` }, body: '' })).status, 404);
    assert.equal((await send(brokerPort, { path: '/v1/complete',
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
        body: '{"model":"test-model","max_tokens":1024}',
      });
      assert.deepEqual(result, { status: 200, body: '{"ok":true}' });
      assert.equal(seen.length, 1);
      assert.equal(seen[0].url, '/v1/messages?beta=true');
      assert.equal(seen[0].body, '{"model":"test-model","max_tokens":1024}');
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
    credential: 'provider-secret-value-1234567890', sessionToken,
    model: 'test-model', maxOutputTokens: 4096 }, {
    requestUpstream: httpRequest,
    upstream: { protocol: 'http:', hostname: '127.0.0.1', port: upstreamPort },
    maxRequests: 2,
    maxRequestBytes: 64,
  });
  const brokerPort = await listen(server);
  const headers = { authorization: `Bearer ${sessionToken}` };
  try {
    assert.equal((await send(brokerPort, { headers, body: 'x'.repeat(65) })).status, 413);
    assert.equal((await send(brokerPort, { headers,
      body: '{"model":"test-model","max_tokens":1}' })).status, 200);
    assert.equal((await send(brokerPort, { headers,
      body: '{"model":"test-model","max_tokens":1}' })).status, 429);
    assert.deepEqual(seen, ['/v1/messages']);
  } finally {
    await close(server);
    await close(upstreamServer);
  }
});

test('credential broker enforces model, output token, and session cost limits', async () => {
  const rates = { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6, cacheRead: 0.3 };
  await withBroker('api-key', async ({ brokerPort, sessionToken, seen, stats }) => {
    const headers = { authorization: `Bearer ${sessionToken}`,
      'content-type': 'application/json' };
    assert.equal((await send(brokerPort, { headers,
      body: '{"model":"other-model","max_tokens":1}' })).status, 400);
    assert.equal((await send(brokerPort, { headers,
      body: '{"model":"test-model","max_tokens":4097}' })).status, 400);
    assert.equal((await send(brokerPort, { headers,
      body: '{"model":"test-model","max_tokens":1000}' })).status, 402);
    assert.equal(seen.length, 0);
    assert.deepEqual(stats(), { acceptedRequests: 3, billableRequests: 0,
      completedBillableRequests: 0, spentUsd: 0, reservedUsd: 0 });
  }, { maxBudgetUsd: 0.01, pricingRates: rates });
});

test('credential broker charges reported usage and reserves enough for the next request', async () => {
  const rates = { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6, cacheRead: 0.3 };
  const upstreamBody = JSON.stringify({ usage: { input_tokens: 100, output_tokens: 100,
    cache_read_input_tokens: 0, cache_creation_input_tokens: 0 } });
  await withBroker('api-key', async ({ brokerPort, sessionToken, seen, stats }) => {
    const request = { headers: { authorization: `Bearer ${sessionToken}`,
      'content-type': 'application/json' },
    body: '{"model":"test-model","max_tokens":1000}' };
    assert.equal((await send(brokerPort, request)).status, 200);
    assert.equal((await send(brokerPort, request)).status, 200);
    assert.equal((await send(brokerPort, request)).status, 200);
    assert.equal((await send(brokerPort, request)).status, 402);
    assert.equal(seen.length, 3);
    assert.deepEqual(stats(), { acceptedRequests: 4, billableRequests: 3,
      completedBillableRequests: 3, spentUsd: 0.0054, reservedUsd: 0 });
  }, { maxBudgetUsd: 0.02, pricingRates: rates, upstreamBody });
});

test('credential broker prices repeated compressed streams instead of exhausting the request cap', async () => {
  const rates = { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6, cacheRead: 0.3 };
  const stream = [
    'data: {"type":"message_start","message":{"usage":{"input_tokens":100,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}',
    'data: {"type":"message_delta","usage":{"output_tokens":100}}',
    '',
  ].join('\n');
  await withBroker('api-key', async ({ brokerPort, sessionToken, seen, stats }) => {
    const request = { headers: { authorization: `Bearer ${sessionToken}`,
      'content-type': 'application/json', 'accept-encoding': 'gzip' },
    body: '{"model":"test-model","max_tokens":128000}' };
    for (let turn = 0; turn < 60; turn += 1) {
      assert.equal((await send(brokerPort, request)).status, 200);
    }
    assert.equal(seen[0].headers['accept-encoding'], undefined);
    assert.deepEqual(stats(), { acceptedRequests: 60, billableRequests: 60,
      completedBillableRequests: 60, spentUsd: 0.108, reservedUsd: 0 });
  }, { maxBudgetUsd: 100, maxOutputTokens: 128_000, pricingRates: rates,
    upstreamHeaders: { 'content-type': 'text/event-stream', 'content-encoding': 'gzip' },
    upstreamBody: gzipSync(stream) });
});

test('credential broker atomically records all direct requests without credentials', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-broker-ledger-test-'));
  const ledgerPath = join(root, 'ledger.json');
  const rates = { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6, cacheRead: 0.3 };
  const upstreamBody = JSON.stringify({ usage: { input_tokens: 100, output_tokens: 100,
    cache_read_input_tokens: 0, cache_creation_input_tokens: 0 } });
  try {
    await withBroker('api-key', async ({ brokerPort, sessionToken, credential }) => {
      const request = { headers: { authorization: `Bearer ${sessionToken}`,
        'content-type': 'application/json' },
      body: '{"model":"test-model","max_tokens":1000}' };
      assert.equal((await send(brokerPort, request)).status, 200);
      assert.equal((await send(brokerPort, request)).status, 200);
      const ledger = readCredentialBrokerLedger(ledgerPath,
        { model: 'test-model', maxBudgetUsd: 0.02 });
      assert.equal(ledger.complete, true);
      assert.equal(ledger.billableRequests, 2);
      assert.equal(ledger.completedBillableRequests, 2);
      assert.equal(ledger.spentUsd, 0.0036);
      const text = readFileSync(ledgerPath, 'utf8');
      assert.doesNotMatch(text, new RegExp(sessionToken));
      assert.doesNotMatch(text, new RegExp(credential));
      assert.deepEqual(readdirSync(root), ['ledger.json']);
    }, { ledgerPath, maxBudgetUsd: 0.02, pricingRates: rates, upstreamBody });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('broker receipt is authoritative and direct request spend causes fail-closed mismatch', () => {
  const ledger = { schemaVersion: 1, model: 'test-model', maxBudgetUsd: 1,
    acceptedRequests: 2, billableRequests: 2, completedBillableRequests: 2,
    spentUsd: 0.0036, reservedUsd: 0, complete: true,
    updatedAt: new Date().toISOString() };
  const reconciled = reconcileCredentialBrokerReceipt({ ledger,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 0.0018 },
    model: 'test-model', maxBudgetUsd: 1 });
  assert.equal(reconciled.ok, false);
  assert.equal(reconciled.result.is_error, true);
  assert.equal(reconciled.result.total_cost_usd, 0.0036);
  assert.equal(reconciled.receipt.source, 'credential-broker');
  assert.match(reconciled.receipt.error, /does not match CLI receipt/);
});

test('missing or incomplete broker ledgers fail closed with a conservative receipt', () => {
  const missing = reconcileCredentialBrokerReceipt({ ledger: null,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 0.25 },
    model: 'test-model', maxBudgetUsd: 1 });
  assert.equal(missing.ok, false);
  assert.equal(missing.result.total_cost_usd, 1);
  assert.equal(missing.result.stack_bench_cost_receipt.complete, false);

  const incomplete = reconcileCredentialBrokerReceipt({ ledger: {
    schemaVersion: 1, model: 'test-model', maxBudgetUsd: 1,
    acceptedRequests: 1, billableRequests: 1, completedBillableRequests: 0,
    spentUsd: 0.1, reservedUsd: 0.4, complete: false,
    updatedAt: new Date().toISOString(),
  }, cliResult: { type: 'result', is_error: false, total_cost_usd: 0.1 },
  model: 'test-model', maxBudgetUsd: 1 });
  assert.equal(incomplete.ok, false);
  assert.equal(incomplete.result.total_cost_usd, 0.5);
  assert.match(incomplete.receipt.error, /incomplete/);
});

test('matching broker and CLI receipts preserve a successful result', () => {
  const ledger = { schemaVersion: 1, model: 'test-model', maxBudgetUsd: 1,
    acceptedRequests: 1, billableRequests: 1, completedBillableRequests: 1,
    spentUsd: 0.0018, reservedUsd: 0, complete: true,
    updatedAt: new Date().toISOString() };
  const reconciled = reconcileCredentialBrokerReceipt({ ledger,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 0.0018 },
    model: 'test-model', maxBudgetUsd: 1 });
  assert.equal(reconciled.ok, true);
  assert.equal(reconciled.result.is_error, false);
  assert.equal(reconciled.result.total_cost_usd, 0.0018);
  assert.equal(reconciled.result.stack_bench_cost_receipt.reconciled, true);
});

test('credential broker lifecycle creates only an attempt-scoped session credential', async () => {
  const providerCredential = 'provider-secret-value-1234567890';
  const rates = { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6, cacheRead: 0.3 };
  const broker = startCredentialBroker({ mode: 'api-key',
    environment: { name: 'ANTHROPIC_API_KEY', value: providerCredential }, mount: null },
  { networkMode: 'host', deadlineMs: 10_000, model: 'test-model',
    maxBudgetUsd: 1, pricingRates: rates });
  let ledger;
  try {
    assert.equal(existsSync(broker.root), true);
    assert.equal(broker.listenHost, '127.0.0.1');
    assert.doesNotMatch(broker.baseUrl, new RegExp(providerCredential));
    assert.match(broker.sessionToken, /^[a-f0-9]{64}$/);
    assert.equal((await send(new URL(broker.baseUrl).port)).status, 401);
  } finally {
    ledger = stopCredentialBroker(broker);
  }
  assert.equal(existsSync(broker.root), false);
  assert.equal(ledger.complete, true);
  assert.equal(ledger.spentUsd, 0);
});
