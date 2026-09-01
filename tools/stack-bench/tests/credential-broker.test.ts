import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { request as httpRequest, createServer } from 'node:http';
import type { IncomingHttpHeaders, OutgoingHttpHeaders, Server } from 'node:http';
import { connect } from 'node:net';
import type { Socket } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import { gzipSync } from 'node:zlib';

import { createCredentialBroker } from '../container/credential-broker.js';
import { readCredentialBrokerLedger, reconcileCredentialBrokerReceipt }
  from '../container/credential-broker-accounting.js';
import type { BrokerConfig, BrokerLedger, BrokerMode }
  from '../container/credential-broker-accounting.js';
import { credentialBrokerDiagnostics, startCredentialBroker, stopCredentialBroker }
  from '../container/credential-broker-process.js';
import type { CredentialBrokerHandle } from '../container/credential-broker-process.js';
import type { BrokerStats } from '../container/credential-broker.js';
import { compiledEntrypoint } from '../src/package-root.js';

const PRICING_RATES = {
  input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6, cacheRead: 0.3,
};
const ONE_REQUEST_USAGE = { input_tokens: 100, output_tokens: 100,
  cache_read_input_tokens: 0, cache_creation_input_tokens: 0 };
const DETAILED_USAGE = { input_tokens: 100, output_tokens: 100,
  cache_read_input_tokens: 50, cache_creation_input_tokens: 30,
  cache_creation: { ephemeral_5m_input_tokens: 20, ephemeral_1h_input_tokens: 10 } };
const ZERO_USAGE = { input: 0, output: 0, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 };

interface SendOptions {
  method?: string;
  path?: string;
  headers?: OutgoingHttpHeaders;
  body?: string;
}

interface SendResult {
  status: number | undefined;
  body: string;
}

interface SeenRequest {
  method: string | undefined;
  url: string | undefined;
  headers: IncomingHttpHeaders;
  body: string;
}

interface BrokerTestContext {
  brokerPort: number;
  sessionToken: string;
  credential: string;
  seen: SeenRequest[];
  stats: () => BrokerStats;
}

interface BrokerReady {
  pid: number;
  baseUrl: string;
  sessionToken: string;
  root: string;
  ledgerPath: string;
}

type BrokerTestConfig = Partial<Pick<BrokerConfig,
  'ledgerPath' | 'maxBudgetUsd' | 'pricingRates' | 'maxOutputTokens'>> & {
    upstreamBody?: string | Buffer;
    upstreamHeaders?: OutgoingHttpHeaders;
  };

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function brokerReady(value: unknown): BrokerReady {
  if (!record(value) || typeof value.pid !== 'number' || typeof value.baseUrl !== 'string'
    || typeof value.sessionToken !== 'string' || typeof value.root !== 'string'
    || typeof value.ledgerPath !== 'string') {
    throw new Error('credential broker subprocess returned invalid readiness data');
  }
  return { pid: value.pid, baseUrl: value.baseUrl, sessionToken: value.sessionToken,
    root: value.root, ledgerPath: value.ledgerPath };
}

const listen = (server: Server): Promise<number> => new Promise((resolveListen, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', () => {
    const address = server.address();
    assert.ok(address && typeof address !== 'string');
    resolveListen(address.port);
  });
});

const close = (server: Server): Promise<void> => new Promise((resolveClose, reject) => {
  server.close(error => error ? reject(error) : resolveClose());
});

async function waitFor(predicate: () => boolean, timeoutMs = 2_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error('timed out waiting for broker state');
    await new Promise<void>(resolveWait => setTimeout(resolveWait, 10));
  }
}

function send(port: number, { method = 'POST', path = '/v1/messages', headers = {}, body = '{}' }:
  SendOptions = {}): Promise<SendResult> {
  return new Promise((resolveSend, reject) => {
    const request = httpRequest({ hostname: '127.0.0.1', port, method, path, headers }, response => {
      const chunks: Buffer[] = [];
      response.on('data', (chunk: Buffer) => chunks.push(chunk));
      response.on('end', () => resolveSend({ status: response.statusCode,
        body: Buffer.concat(chunks).toString('utf8') }));
    });
    request.once('error', reject);
    request.end(body);
  });
}

async function withBroker(mode: BrokerMode, body: (context: BrokerTestContext) => Promise<void>,
  config: BrokerTestConfig = {}): Promise<void> {
  const { upstreamBody = '{"ok":true}', upstreamHeaders = {}, ...brokerConfig } = config;
  const seen: SeenRequest[] = [];
  const upstreamServer = createServer((request, response) => {
    const chunks: Buffer[] = [];
    request.on('data', (chunk: Buffer) => chunks.push(chunk));
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

test('credential broker rejects an unauthorized request before its body completes', async () => {
  await withBroker('api-key', async ({ brokerPort }) => {
    const result = await new Promise<SendResult>((resolveResult, reject) => {
      const request = httpRequest({ hostname: '127.0.0.1', port: brokerPort,
        method: 'POST', path: '/v1/messages', headers: { 'content-length': '1000000' } }, response => {
        const chunks: Buffer[] = [];
        response.on('data', (chunk: Buffer) => chunks.push(chunk));
        response.on('end', () => resolveResult({ status: response.statusCode,
          body: Buffer.concat(chunks).toString('utf8') }));
      });
      request.once('error', reject);
      request.write('{');
    });
    assert.deepEqual(result, { status: 401, body: 'unauthorized' });
  });
});

test('credential broker survives a downstream disconnect and settles exact upstream usage', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-broker-disconnect-'));
  const ledgerPath = join(root, 'ledger.json');
  const upstreamServer = createServer((request, response) => {
    request.resume();
    request.on('end', () => {
      response.writeHead(200, { 'content-type': 'application/json' });
      response.write('{"usage":');
      setTimeout(() => response.end(`${JSON.stringify(ONE_REQUEST_USAGE)}}`), 25);
    });
  });
  const upstreamPort = await listen(upstreamServer);
  const sessionToken = 'session-token-value-1234567890';
  const { server, stats } = createCredentialBroker({ mode: 'api-key',
    credential: 'provider-secret-value-1234567890', sessionToken,
    ledgerPath, model: 'test-model', maxOutputTokens: 4096,
    maxBudgetUsd: 1, pricingRates: PRICING_RATES }, {
    requestUpstream: httpRequest,
    upstream: { protocol: 'http:', hostname: '127.0.0.1', port: upstreamPort },
  });
  const brokerPort = await listen(server);
  const requestOptions = { hostname: '127.0.0.1', port: brokerPort, method: 'POST',
    path: '/v1/messages', headers: { authorization: `Bearer ${sessionToken}`,
      'content-type': 'application/json' } };
  try {
    await new Promise<void>((resolveRequest, reject) => {
      const request = httpRequest(requestOptions, response => {
        response.once('data', () => { response.destroy(); resolveRequest(); });
      });
      request.once('error', reject);
      request.end('{"model":"test-model","max_tokens":1000}');
    });
    await waitFor(() => stats().completedBillableRequests === 1);
    const second = await send(brokerPort, {
      headers: requestOptions.headers,
      body: '{"model":"test-model","max_tokens":1000}',
    });
    assert.equal(second.status, 200);
    assert.deepEqual(stats(), { acceptedRequests: 2, billableRequests: 2,
      completedBillableRequests: 2, estimatedBillableRequests: 0,
      spentUsd: 0.0036, reservedUsd: 0 });
    assert.equal(readCredentialBrokerLedger(ledgerPath,
      { model: 'test-model', maxBudgetUsd: 1 }).complete, true);
  } finally {
    await close(server);
    await close(upstreamServer);
    rmSync(root, { recursive: true, force: true });
  }
});

test('credential broker survives malformed client socket input', async () => {
  await withBroker('api-key', async ({ brokerPort, sessionToken }) => {
    await new Promise<void>((resolveSocket, reject) => {
      const socket = connect({ host: '127.0.0.1', port: brokerPort });
      socket.once('connect', () => socket.write('not-http\r\n\r\n'));
      socket.once('data', () => socket.destroy());
      socket.once('close', () => resolveSocket());
      socket.once('error', reject);
    });
    const result = await send(brokerPort, {
      headers: { authorization: `Bearer ${sessionToken}` },
      body: '{"model":"test-model","max_tokens":1}',
    });
    assert.equal(result.status, 200);
  });
});

for (const mode of ['api-key', 'subscription-token'] satisfies BrokerMode[]) {
  test(`credential broker replaces the session credential for ${mode}`, async () => {
    await withBroker(mode, async ({ brokerPort, sessionToken, credential, seen, stats }) => {
      const result = await send(brokerPort, {
        path: '/v1/messages?beta=true',
        headers: { authorization: `Bearer ${sessionToken}`, 'content-type': 'application/json' },
        body: '{"model":"test-model","max_tokens":1024}',
      });
      assert.deepEqual(result, { status: 200, body: '{"ok":true}' });
      assert.equal(seen.length, 1);
      const forwarded = seen[0];
      assert.ok(forwarded);
      assert.equal(forwarded.url, '/v1/messages?beta=true');
      assert.equal(forwarded.body, '{"model":"test-model","max_tokens":1024}');
      if (mode === 'api-key') {
        assert.equal(forwarded.headers['x-api-key'], credential);
        assert.equal(forwarded.headers.authorization, undefined);
      } else {
        assert.equal(forwarded.headers.authorization, `Bearer ${credential}`);
        assert.equal(forwarded.headers['x-api-key'], undefined);
      }
      assert.doesNotMatch(JSON.stringify(forwarded.headers), new RegExp(sessionToken));
      assert.equal(stats().acceptedRequests, 1);
    });
  });
}

test('credential broker enforces request size and count limits', async () => {
  const seen: string[] = [];
  const upstreamServer = createServer((request, response) => {
    request.resume();
    request.on('end', () => {
      assert.ok(request.url);
      seen.push(request.url);
      response.end('ok');
    });
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
    assert.deepEqual(await send(brokerPort, { headers,
      body: '{"model":"other-model","max_tokens":1}' }),
    { status: 400, body: 'invalid provider request' });
    assert.equal((await send(brokerPort, { headers,
      body: '{"model":"test-model","max_tokens":4097}' })).status, 400);
    assert.equal((await send(brokerPort, { headers,
      body: '{"model":"test-model","max_tokens":1000}' })).status, 402);
    assert.equal(seen.length, 0);
    assert.deepEqual(stats(), { acceptedRequests: 3, billableRequests: 0,
      completedBillableRequests: 0, estimatedBillableRequests: 0,
      spentUsd: 0, reservedUsd: 0 });
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
      completedBillableRequests: 3, estimatedBillableRequests: 0,
      spentUsd: 0.0054, reservedUsd: 0 });
  }, { maxBudgetUsd: 0.02, pricingRates: rates, upstreamBody });
});

test('credential broker clears different concurrent reservations before the next request', async () => {
  const rates = { input: 2, output: 10, cacheWrite5m: 2.5, cacheWrite1h: 4, cacheRead: 0.2 };
  const upstreamBody = JSON.stringify({ usage: ONE_REQUEST_USAGE });
  const bodyWithBytes = (bytes: number): string => {
    const prefix = '{"model":"test-model","max_tokens":65536,"padding":"';
    const suffix = '"}';
    return `${prefix}${'x'.repeat(bytes - prefix.length - suffix.length)}${suffix}`;
  };
  await withBroker('api-key', async ({ brokerPort, sessionToken, stats }) => {
    const headers = { authorization: `Bearer ${sessionToken}`,
      'content-type': 'application/json' };
    const first = send(brokerPort, { headers, body: bodyWithBytes(4_049) });
    const second = send(brokerPort, { headers, body: bodyWithBytes(6_840) });
    assert.deepEqual((await Promise.all([first, second])).map(result => result.status), [200, 200]);
    assert.equal(stats().reservedUsd, 0);
    assert.equal((await send(brokerPort, { headers, body: bodyWithBytes(4_049) })).status, 200);
    assert.deepEqual(stats(), { acceptedRequests: 3, billableRequests: 3,
      completedBillableRequests: 3, estimatedBillableRequests: 0,
      spentUsd: 0.0036, reservedUsd: 0 });
  }, { maxBudgetUsd: 100, maxOutputTokens: 65_536, pricingRates: rates, upstreamBody });
});

test('credential broker prices repeated compressed streams instead of exhausting the request cap', async () => {
  const rates = { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6, cacheRead: 0.3 };
  const stream = [
    'data: {"type":"message_start","message":{"usage":{"input_tokens":100,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}',
    'data: {"type":"message_delta","usage":{"output_tokens":100}}',
    'data: {"type":"message_stop"}',
    '',
  ].join('\n');
  await withBroker('api-key', async ({ brokerPort, sessionToken, seen, stats }) => {
    const request = { headers: { authorization: `Bearer ${sessionToken}`,
      'content-type': 'application/json', 'accept-encoding': 'gzip' },
    body: '{"model":"test-model","max_tokens":128000}' };
    for (let turn = 0; turn < 60; turn += 1) {
      assert.equal((await send(brokerPort, request)).status, 200);
    }
    const forwarded = seen[0];
    assert.ok(forwarded);
    assert.equal(forwarded.headers['accept-encoding'], undefined);
    assert.deepEqual(stats(), { acceptedRequests: 60, billableRequests: 60,
      completedBillableRequests: 60, estimatedBillableRequests: 0,
      spentUsd: 0.108, reservedUsd: 0 });
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
      assert.equal(ledger.estimatedBillableRequests, 0);
      assert.equal(ledger.spentUsd, 0.0036);
      assert.deepEqual(ledger.usage,
        { input: 200, output: 200, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 });
      const text = readFileSync(ledgerPath, 'utf8');
      assert.doesNotMatch(text, new RegExp(sessionToken));
      assert.doesNotMatch(text, new RegExp(credential));
      assert.deepEqual(readdirSync(root), ['ledger.json']);
    }, { ledgerPath, maxBudgetUsd: 0.02, pricingRates: rates, upstreamBody });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('credential broker rejects a settled request without exact provider usage', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-broker-estimated-ledger-test-'));
  const ledgerPath = join(root, 'ledger.json');
  try {
    await withBroker('api-key', async ({ brokerPort, sessionToken }) => {
      const response = await send(brokerPort, {
        headers: { authorization: `Bearer ${sessionToken}`, 'content-type': 'application/json' },
        body: '{"model":"test-model","max_tokens":1000}',
      });
      assert.equal(response.status, 200);
      const ledger = readCredentialBrokerLedger(ledgerPath,
        { model: 'test-model', maxBudgetUsd: 1 });
      assert.equal(ledger.complete, true);
      assert.equal(ledger.estimatedBillableRequests, 1);
      const reconciled = reconcileCredentialBrokerReceipt({ ledger,
        cliResult: { type: 'result', is_error: false, total_cost_usd: 0, usage: ZERO_USAGE },
        model: 'test-model', maxBudgetUsd: 1, pricingRates: PRICING_RATES });
      assert.equal(reconciled.ok, false);
      assert.equal(reconciled.receipt.reconciled, false);
      assert.match(reconciled.receipt.error ?? '', /without exact provider usage/);
    }, { ledgerPath, maxBudgetUsd: 1, pricingRates: PRICING_RATES,
      upstreamBody: '{"ok":true}' });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('credential broker rejects a cleanly ended provider error stream', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-broker-error-stream-test-'));
  const ledgerPath = join(root, 'ledger.json');
  const stream = [
    'data: {"type":"message_start","message":{"usage":{"input_tokens":100,"output_tokens":0}}}',
    'event: error',
    'data: {"type":"error","error":{"type":"api_error","message":"connection lost"}}',
    '',
  ].join('\n');
  try {
    await withBroker('api-key', async ({ brokerPort, sessionToken }) => {
      assert.equal((await send(brokerPort, {
        headers: { authorization: `Bearer ${sessionToken}`, 'content-type': 'application/json' },
        body: '{"model":"test-model","max_tokens":1000}',
      })).status, 200);
      const ledger = readCredentialBrokerLedger(ledgerPath,
        { model: 'test-model', maxBudgetUsd: 1 });
      assert.equal(ledger.complete, true);
      assert.equal(ledger.estimatedBillableRequests, 1);
      const reconciled = reconcileCredentialBrokerReceipt({ ledger,
        cliResult: { type: 'result', is_error: true, total_cost_usd: 0, usage: ZERO_USAGE },
        model: 'test-model', maxBudgetUsd: 1, pricingRates: PRICING_RATES });
      assert.equal(reconciled.ok, false);
      assert.match(reconciled.receipt.error ?? '', /without exact provider usage/);
    }, { ledgerPath, maxBudgetUsd: 1, pricingRates: PRICING_RATES,
      upstreamHeaders: { 'content-type': 'text/event-stream' }, upstreamBody: stream });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('streamed provider usage produces a reconciled campaign-priced receipt', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-broker-cache-ledger-test-'));
  const ledgerPath = join(root, 'ledger.json');
  const stream = [
    'data: {"type":"message_start","message":{"usage":{"input_tokens":100,"output_tokens":0,"cache_read_input_tokens":50,"cache_creation_input_tokens":30,"cache_creation":{"ephemeral_5m_input_tokens":20,"ephemeral_1h_input_tokens":10}}}}',
    'data: {"type":"message_delta","usage":{"output_tokens":100}}',
    'data: {"type":"message_stop"}',
    '',
  ].join('\n');
  try {
    await withBroker('api-key', async ({ brokerPort, sessionToken }) => {
      const response = await send(brokerPort, {
        headers: { authorization: `Bearer ${sessionToken}`, 'content-type': 'application/json' },
        body: '{"model":"test-model","max_tokens":1000}',
      });
      assert.equal(response.status, 200);
      const ledger = readCredentialBrokerLedger(ledgerPath,
        { model: 'test-model', maxBudgetUsd: 1 });
      assert.deepEqual(ledger.usage,
        { input: 100, output: 100, cacheRead: 50, cacheWrite5m: 20, cacheWrite1h: 10 });
      assert.equal(ledger.spentUsd, 0.00195);
      const reconciled = reconcileCredentialBrokerReceipt({ ledger,
        cliResult: { type: 'result', is_error: false, total_cost_usd: 0.002925,
          usage: { input_tokens: 100, output_tokens: 100, cache_read_input_tokens: 50,
            cache_creation_input_tokens: 30 } },
        model: 'test-model', maxBudgetUsd: 1, pricingRates: PRICING_RATES });
      assert.equal(reconciled.ok, true);
      assert.equal(reconciled.receipt.costUsd, 0.00195);
      assert.equal(reconciled.receipt.calculatedCostUsd, 0.00195);
      assert.equal(reconciled.receipt.cliCostUsd, 0.002925);
    }, { ledgerPath, maxBudgetUsd: 1, pricingRates: PRICING_RATES,
      upstreamHeaders: { 'content-type': 'text/event-stream' }, upstreamBody: stream });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('campaign pricing is authoritative when CLI pricing differs', () => {
  const ledger = { schemaVersion: 3, model: 'test-model', maxBudgetUsd: 1,
    acceptedRequests: 2, billableRequests: 2, completedBillableRequests: 2,
    estimatedBillableRequests: 0,
    spentUsd: 0.0036, reservedUsd: 0,
    usage: { input: 200, output: 200, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 },
    complete: true,
    updatedAt: new Date().toISOString() };
  const reconciled = reconcileCredentialBrokerReceipt({ ledger,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 0.0054,
      usage: { ...ONE_REQUEST_USAGE, input_tokens: 200, output_tokens: 200 } },
    model: 'test-model', maxBudgetUsd: 1, pricingRates: PRICING_RATES });
  assert.equal(reconciled.ok, true);
  assert.equal(reconciled.result.is_error, false);
  assert.equal(reconciled.result.total_cost_usd, 0.0036);
  assert.equal(reconciled.receipt.source, 'credential-broker');
  assert.equal(reconciled.receipt.cliCostUsd, 0.0054);
  assert.equal(reconciled.receipt.error, null);
});

test('broker retains cache write classes that the CLI summary combines', () => {
  const ledger = { schemaVersion: 3, model: 'claude-sonnet-5', maxBudgetUsd: 50,
    acceptedRequests: 66, billableRequests: 66, completedBillableRequests: 66,
    estimatedBillableRequests: 0,
    spentUsd: 1.686071, reservedUsd: 0,
    usage: { input: 124, output: 48559, cacheRead: 5100226,
      cacheWrite5m: 66627, cacheWrite1h: 3405 },
    complete: true, updatedAt: new Date().toISOString() };
  const reconciled = reconcileCredentialBrokerReceipt({ ledger,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 2.529107,
      usage: { input_tokens: 124, output_tokens: 48559,
        cache_read_input_tokens: 5100226, cache_creation_input_tokens: 70032 } },
    model: 'claude-sonnet-5', maxBudgetUsd: 50,
    pricingRates: { input: 2, output: 10, cacheWrite5m: 2.5,
      cacheWrite1h: 4, cacheRead: 0.2 } });
  assert.equal(reconciled.ok, true);
  assert.equal(reconciled.receipt.costUsd, 1.686071);
  assert.equal(reconciled.receipt.calculatedCostUsd, 1.686071);
  assert.equal(reconciled.receipt.cliCostUsd, 2.529107);
  assert.ok(reconciled.receipt.usage);
  assert.equal(reconciled.receipt.usage.cacheWrite1h, 3405);
});

test('missing or incomplete broker ledgers fail closed with a conservative receipt', () => {
  const missing = reconcileCredentialBrokerReceipt({ ledger: null,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 0.25 },
    model: 'test-model', maxBudgetUsd: 1, pricingRates: PRICING_RATES });
  assert.equal(missing.ok, false);
  assert.equal(missing.result.total_cost_usd, 1);
  assert.equal(missing.result.stack_bench_cost_receipt.complete, false);

  const incomplete = reconcileCredentialBrokerReceipt({ ledger: {
    schemaVersion: 3, model: 'test-model', maxBudgetUsd: 1,
    acceptedRequests: 1, billableRequests: 1, completedBillableRequests: 0,
    estimatedBillableRequests: 0,
    spentUsd: 0.1, reservedUsd: 0.4, usage: ZERO_USAGE, complete: false,
    updatedAt: new Date().toISOString(),
  }, cliResult: { type: 'result', is_error: false, total_cost_usd: 0.1 },
  model: 'test-model', maxBudgetUsd: 1, pricingRates: PRICING_RATES });
  assert.equal(incomplete.ok, false);
  assert.equal(incomplete.result.total_cost_usd, 0.5);
  assert.match(incomplete.receipt.error ?? '', /incomplete/);
});

test('matching broker and CLI receipts preserve a successful result', () => {
  const ledger = { schemaVersion: 3, model: 'test-model', maxBudgetUsd: 1,
    acceptedRequests: 1, billableRequests: 1, completedBillableRequests: 1,
    estimatedBillableRequests: 0,
    spentUsd: 0.00195, reservedUsd: 0,
    usage: { input: 100, output: 100, cacheRead: 50, cacheWrite5m: 20, cacheWrite1h: 10 },
    complete: true,
    updatedAt: new Date().toISOString() };
  const reconciled = reconcileCredentialBrokerReceipt({ ledger,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 0.00195,
      usage: DETAILED_USAGE },
    model: 'test-model', maxBudgetUsd: 1, pricingRates: PRICING_RATES });
  assert.equal(reconciled.ok, true);
  assert.equal(reconciled.result.is_error, false);
  assert.equal(reconciled.result.total_cost_usd, 0.00195);
  assert.equal(reconciled.result.stack_bench_cost_receipt.reconciled, true);
  assert.deepEqual(reconciled.receipt.usage, {
    input: 100, output: 100, cacheRead: 50, cacheWrite5m: 20, cacheWrite1h: 10,
  });
  assert.deepEqual(reconciled.receipt.pricingRates, PRICING_RATES);
  assert.equal(reconciled.receipt.calculatedCostUsd, 0.00195);
});

test('receipt reconciliation accepts provider usage that includes CLI-omitted calls', () => {
  const ledger = { schemaVersion: 3, model: 'test-model', maxBudgetUsd: 1,
    acceptedRequests: 2, billableRequests: 2, completedBillableRequests: 2,
    estimatedBillableRequests: 0,
    spentUsd: 0.00201, reservedUsd: 0,
    usage: { input: 120, output: 110, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 },
    complete: true,
    updatedAt: new Date().toISOString() };
  const reconciled = reconcileCredentialBrokerReceipt({ ledger,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 0.00351,
      usage: ONE_REQUEST_USAGE },
  model: 'test-model', maxBudgetUsd: 1, pricingRates: PRICING_RATES });
  assert.equal(reconciled.ok, true);
  assert.equal(reconciled.receipt.costUsd, 0.00201);
  assert.deepEqual(reconciled.result.usage, {
    input_tokens: 120, output_tokens: 110, cache_read_input_tokens: 0,
    cache_creation_input_tokens: 0,
    cache_creation: { ephemeral_5m_input_tokens: 0, ephemeral_1h_input_tokens: 0 },
  });
});

test('receipt reconciliation rejects provider usage below the CLI lower bound', () => {
  const ledger = { schemaVersion: 3, model: 'test-model', maxBudgetUsd: 1,
    acceptedRequests: 1, billableRequests: 1, completedBillableRequests: 1,
    estimatedBillableRequests: 0,
    spentUsd: 0.0018, reservedUsd: 0,
    usage: { input: 100, output: 100, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 },
    complete: true, updatedAt: new Date().toISOString() };
  const reconciled = reconcileCredentialBrokerReceipt({ ledger,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 0.0018,
      usage: { ...ONE_REQUEST_USAGE, output_tokens: 101 } },
  model: 'test-model', maxBudgetUsd: 1, pricingRates: PRICING_RATES });
  assert.equal(reconciled.ok, false);
  assert.match(reconciled.receipt.error ?? '', /usage is lower than CLI usage/);
});

test('real paid-session usage reconciles to broker totals', () => {
  const ledger = { schemaVersion: 3, model: 'claude-sonnet-5', maxBudgetUsd: 50,
    acceptedRequests: 97, billableRequests: 97, completedBillableRequests: 97,
    estimatedBillableRequests: 0,
    spentUsd: 2.665346, reservedUsd: 0,
    usage: { input: 2647, output: 77041, cacheRead: 8026721,
      cacheWrite5m: 113719, cacheWrite1h: 0 },
    complete: true, updatedAt: new Date().toISOString() };
  const reconciled = reconcileCredentialBrokerReceipt({ ledger,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 3.998019,
      usage: { input_tokens: 156, output_tokens: 77016,
        cache_read_input_tokens: 8026721, cache_creation_input_tokens: 113719 } },
    model: 'claude-sonnet-5', maxBudgetUsd: 50,
    pricingRates: { input: 2, output: 10, cacheWrite5m: 2.5,
      cacheWrite1h: 4, cacheRead: 0.2 } });
  assert.equal(reconciled.ok, true);
  assert.equal(reconciled.receipt.costUsd, 2.665346);
  assert.ok(reconciled.result.usage);
  assert.equal(reconciled.result.usage.input_tokens, 2647);
  assert.equal(reconciled.result.usage.output_tokens, 77041);
});

test('receipt reconciliation rejects spend that cannot be reproduced from broker usage', () => {
  const ledger = { schemaVersion: 3, model: 'test-model', maxBudgetUsd: 1,
    acceptedRequests: 1, billableRequests: 1, completedBillableRequests: 1,
    estimatedBillableRequests: 0,
    spentUsd: 0.002, reservedUsd: 0,
    usage: { input: 100, output: 100, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 },
    complete: true, updatedAt: new Date().toISOString() };
  const reconciled = reconcileCredentialBrokerReceipt({ ledger,
    cliResult: { type: 'result', is_error: false, total_cost_usd: 0.002,
      usage: ONE_REQUEST_USAGE },
    model: 'test-model', maxBudgetUsd: 1, pricingRates: PRICING_RATES });
  assert.equal(reconciled.ok, false);
  assert.match(reconciled.receipt.error ?? '', /usage-priced spend/);
});

test('credential broker lifecycle creates only an attempt-scoped session credential', async () => {
  const providerCredential = 'provider-secret-value-1234567890';
  const rates = { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6, cacheRead: 0.3 };
  const broker = await startCredentialBroker({ mode: 'api-key', credential: providerCredential },
  { networkMode: 'host', deadlineMs: 10_000, model: 'test-model',
    maxBudgetUsd: 1, pricingRates: rates });
  let ledger: BrokerLedger | null = null;
  try {
    assert.equal(existsSync(broker.root), true);
    assert.equal(broker.listenHost, '127.0.0.1');
    assert.doesNotMatch(broker.baseUrl, new RegExp(providerCredential));
    assert.match(broker.sessionToken, /^[a-f0-9]{64}$/);
    assert.equal((await send(Number(new URL(broker.baseUrl).port))).status, 401);
  } finally {
    ledger = await stopCredentialBroker(broker);
  }
  assert.equal(existsSync(broker.root), false);
  assert.ok(ledger);
  assert.equal(ledger.complete, true);
  assert.equal(ledger.spentUsd, 0);
  const diagnostics = credentialBrokerDiagnostics(broker);
  assert.ok(diagnostics?.drain && diagnostics.ledger);
  assert.equal(diagnostics.endpointKind, 'local-credential-broker');
  assert.equal(diagnostics.drain.timedOut, false);
  assert.equal(diagnostics.ledger.complete, true);
  assert.equal(diagnostics.ledger.spentUsd, 0);
});

test('credential broker diagnostics retain child exit, stderr, and the final ledger', async () => {
  const providerCredential = 'provider-secret-value-1234567890';
  const broker = await startCredentialBroker({ mode: 'api-key', credential: providerCredential },
    { networkMode: 'host', deadlineMs: 10_000, model: 'test-model' });
  const providerCut = 11;
  const sessionCut = 19;
  const stderr = broker.child.stderr;
  assert.ok(stderr);
  stderr.emit('data', Buffer.from(
    `${'x'.repeat(20_000)} broker failure ${providerCredential.slice(0, providerCut)}`));
  stderr.emit('data', Buffer.from(
    `${providerCredential.slice(providerCut)} ${broker.sessionToken.slice(0, sessionCut)}`));
  stderr.emit('data', Buffer.from(
    `${broker.sessionToken.slice(sessionCut)}\n`));
  broker.child.kill();
  await once(broker.child, 'exit');
  const ledger = await stopCredentialBroker(broker, { drainTimeoutMs: 0 });
  const diagnostics = credentialBrokerDiagnostics(broker);
  assert.ok(ledger);
  assert.ok(diagnostics?.drain && diagnostics.termination && diagnostics.ledger);
  assert.equal(ledger.complete, true);
  assert.ok(diagnostics.child.exitCode !== null || diagnostics.child.signal !== null);
  assert.match(diagnostics.child.stderrTail ?? '', /broker failure \[REDACTED\] \[REDACTED\]/);
  assert.equal(diagnostics.child.stderrTruncated, true);
  assert.doesNotMatch(JSON.stringify(diagnostics), new RegExp(providerCredential));
  assert.doesNotMatch(JSON.stringify(diagnostics), new RegExp(broker.sessionToken));
  assert.doesNotMatch(diagnostics.child.stderrTail ?? '',
    new RegExp(providerCredential.slice(0, providerCut)));
  assert.doesNotMatch(diagnostics.child.stderrTail ?? '',
    new RegExp(broker.sessionToken.slice(0, sessionCut)));
  assert.equal(diagnostics.drain.terminationRequested, false);
  assert.equal(diagnostics.termination.exited, true);
  assert.equal(diagnostics.ledger.complete, true);
});

test('credential broker closes an active request after its parent exits',
  { timeout: 10_000 }, async () => {
    const moduleUrl = pathToFileURL(compiledEntrypoint('container', 'credential-broker-process.js')).href;
    const script = `
      import { startCredentialBroker } from ${JSON.stringify(moduleUrl)};
      const broker = await startCredentialBroker({ mode: 'api-key',
        credential: 'provider-secret-value-1234567890' },
        { networkMode: 'host', deadlineMs: 10000, model: 'test-model' });
      process.stdout.write(JSON.stringify({ pid: broker.child.pid, baseUrl: broker.baseUrl,
        sessionToken: broker.sessionToken, root: broker.root, ledgerPath: broker.ledgerPath }) + '\\n');
      process.stdin.once('data', () => process.exit(0));
      process.stdin.resume();
    `;
    const parent = spawn(process.execPath, ['--input-type=module', '--eval', script], {
      stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true,
    });
    let active: Socket | null = null;
    let ready: BrokerReady | null = null;
    const alive = (pid: number): boolean => {
      try { process.kill(pid, 0); return true; }
      catch (error) { return !record(error) || error.code !== 'ESRCH'; }
    };
    try {
      ready = await new Promise<BrokerReady>((resolveReady, reject) => {
        let output = '';
        parent.stdout.on('data', (chunk: Buffer) => {
          output += chunk.toString('utf8');
          const newline = output.indexOf('\n');
          if (newline === -1) return;
          try { resolveReady(brokerReady(JSON.parse(output.slice(0, newline)))); }
          catch (error) { reject(error); }
        });
        parent.once('error', reject);
        parent.once('exit', code => reject(new Error(`broker parent exited before ready (${code})`)));
      });
      const brokerInfo = ready;
      const port = Number(new URL(brokerInfo.baseUrl).port);
      active = connect({ host: '127.0.0.1', port });
      active.on('error', () => {});
      await once(active, 'connect');
      const activeClosed = new Promise<void>(resolveClosed =>
        active?.once('close', () => resolveClosed()));
      active.write('POST /v1/messages HTTP/1.1\r\n'
        + `Host: 127.0.0.1:${port}\r\n`
        + `Authorization: Bearer ${brokerInfo.sessionToken}\r\n`
        + 'Content-Type: application/json\r\nContent-Length: 1000\r\n\r\n'
        + '{"model":"test-model"');
      await waitFor(() => readCredentialBrokerLedger(brokerInfo.ledgerPath,
        { model: 'test-model', maxBudgetUsd: null }).acceptedRequests === 1);
      parent.stdin.end('exit');
      await once(parent, 'exit');
      const started = Date.now();
      await waitFor(() => !alive(brokerInfo.pid), 4_000);
      assert.ok(Date.now() - started < 4_000);
      await Promise.race([activeClosed, new Promise((_, reject) => setTimeout(
        () => reject(new Error('active broker socket stayed open')), 1_000))]);
      await new Promise<void>((resolveProbe, reject) => {
        const probe = connect({ host: '127.0.0.1', port });
        const timeout = setTimeout(() => {
          probe.destroy();
          reject(new Error('broker port did not close'));
        }, 1_000);
        probe.once('error', () => { clearTimeout(timeout); resolveProbe(); });
        probe.once('connect', () => {
          clearTimeout(timeout);
          probe.destroy();
          reject(new Error('broker remained reachable after parent exit'));
        });
      });
    } finally {
      active?.destroy();
      if (parent.exitCode === null) parent.kill();
      if (ready && alive(ready.pid)) {
        try { process.kill(ready.pid, 'SIGKILL'); } catch { /* already stopped */ }
      }
      if (ready?.root) rmSync(ready.root, { recursive: true, force: true });
    }
  });

test('credential broker shutdown waits for an in-flight request to settle', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-broker-drain-'));
  const incomplete = { schemaVersion: 3, model: 'test-model', maxBudgetUsd: 1,
    acceptedRequests: 1, billableRequests: 1, completedBillableRequests: 0,
    estimatedBillableRequests: 0,
    spentUsd: 0, reservedUsd: 0.4, usage: ZERO_USAGE, complete: false,
    updatedAt: new Date().toISOString() };
  const complete = { ...incomplete, completedBillableRequests: 1,
    spentUsd: 0.0018, reservedUsd: 0, complete: true };
  let reads = 0;
  let slept = 0;
  let graceful = null;
  let alive = true;
  let clock = 0;
  const broker: CredentialBrokerHandle = { root, ledgerPath: join(root, 'ledger.json'),
    model: 'test-model', maxBudgetUsd: 1, child: { pid: 42 } };
  const ledger = await stopCredentialBroker(broker, {
    drainTimeoutMs: 1_000,
    pollMs: 100,
    readLedger: () => ++reads < 3 ? incomplete : complete,
    requestStop: child => { graceful = child.pid; alive = false; },
    alive: () => alive,
    sleep: async ms => { slept += ms; clock += ms; },
    now: () => clock,
  });
  assert.ok(ledger);
  assert.equal(ledger.complete, true);
  assert.equal(reads, 4, 'the final read must happen after termination');
  assert.equal(slept, 200);
  assert.equal(graceful, 42);
  assert.equal(broker.finalDiagnostics?.termination?.forceRequested, false);
  assert.equal(existsSync(root), false);
});

test('credential broker forces termination and records typed shutdown errors', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-broker-force-'));
  let alive = true;
  let forced = null;
  let clock = 0;
  const broker: CredentialBrokerHandle = { root, ledgerPath: join(root, 'missing-ledger.json'),
    model: 'test-model', maxBudgetUsd: 1, child: { pid: 43 }, processState: {} };
  const ledger = await stopCredentialBroker(broker, {
    drainTimeoutMs: 0, gracefulTimeoutMs: 100, forceTimeoutMs: 100, pollMs: 25,
    readLedger: () => { throw new Error('ledger unavailable'); },
    requestStop: () => {},
    terminate: pid => { forced = pid; alive = false; },
    alive: () => alive,
    sleep: async ms => { clock += ms; },
    now: () => clock,
  });
  assert.equal(ledger, null);
  assert.equal(forced, 43);
  assert.deepEqual(broker.finalDiagnostics?.termination, {
    gracefulRequested: true, forceRequested: true, exited: true,
    gracefulTimeoutMs: 100, forceTimeoutMs: 100,
  });
  assert.deepEqual(broker.finalDiagnostics?.errors, [
    { type: 'ledger-read-error', phase: 'drain', message: 'ledger unavailable' },
    { type: 'ledger-read-error', phase: 'final', message: 'ledger unavailable' },
  ]);
  assert.equal(existsSync(root), false);
});
