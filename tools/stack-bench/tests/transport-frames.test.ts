import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import test from 'node:test';
import { brotliCompressSync, gzipSync } from 'node:zlib';
import type { Page } from 'playwright';

import { captureResponses, ReceivedTransport, transportFrameText } from '../grader/transport-frames.js';
import { ActionInconclusive } from '../src/actions/action-contract.js';
import { findingSchema } from '../src/actions/action-findings.js';

const message = 'subscription update with support-secret-619 inline';

function captureFailure(received: ReceivedTransport) {
  try { received.contains('absent'); }
  catch (error) {
    assert(error instanceof ActionInconclusive);
    const finding = findingSchema.parse(error.details.finding);
    assert.equal(finding.kind, 'transport-incomplete');
    if (finding.kind === 'transport-incomplete') return finding.fields.capture;
  }
  assert.fail('Incomplete capture must remain inconclusive');
}

test('compressed SpacetimeDB frames decode to their message text and other frames keep their bytes', () => {
  assert.equal(transportFrameText(Buffer.concat([Buffer.from([2]), gzipSync(message)])), message);
  assert.equal(transportFrameText(Buffer.concat([Buffer.from([1]), brotliCompressSync(message)])),
    message);
  assert.equal(transportFrameText(Buffer.concat([Buffer.from([0]), Buffer.from(message)])),
    `\u0000${message}`);
  // Other frames keep their bytes.
  assert.equal(transportFrameText(message), message);
  assert.equal(transportFrameText(Buffer.from(message)), message);
  const socketIo = Buffer.from('2["chat",{"text":"hi"}]');
  assert.equal(transportFrameText(socketIo), socketIo.toString('utf8'));
  const notGzip = Buffer.from([2, 0x41, 0x42]);
  assert.equal(transportFrameText(notGzip), notGzip.toString('utf8'));
});

test('transport absence cannot pass after truncation, eviction, or an unreadable body', () => {
  const received = new ReceivedTransport();
  received.record('x'.repeat(200_001) + 'private-tail');
  assert.equal(received.contains('private-tail'), true);
  assert.equal(received.contains('absent'), false);
  received.pending = 1;
  assert.throws(() => received.contains('absent'), /incomplete/);
  assert.equal(captureFailure(received)?.pendingBodies, 1);
  assert.equal(received.contains('absent', false), false);
  received.pending = 0;
  const bounded = new ReceivedTransport(20);
  bounded.record('first-secret');
  bounded.record('first-secret');
  assert.equal(bounded.contains('absent'), false, 'Identical received text adds no evidence bytes');
  assert.deepEqual(bounded.chunks, ['first-secret']);
  bounded.record('second-secret');
  assert.equal(bounded.contains('second-secret'), true);
  assert.throws(() => bounded.contains('first-secret'), /incomplete/);
  assert.deepEqual(captureFailure(bounded), {
    byteLimit: 1, bodyReadFailures: 0, unsupportedStreams: 0, pendingBodies: 0, retainedBytes: 13,
  });
  const oversized = new ReceivedTransport(5);
  oversized.record('too-large');
  assert.throws(() => oversized.contains('large'), /incomplete/);
  assert.equal(captureFailure(oversized)?.byteLimit, 1);
  received.incomplete = true;
  assert.throws(() => received.contains('absent'), /incomplete/);
});

test('capture failures identify body reads, declared body caps, and unsupported streams without payloads', async () => {
  const commands: unknown[] = [];
  const session = Object.assign(new EventEmitter(), {
    send: async (method: string, params?: unknown) => { commands.push({ method, params }); },
  });
  const page = Object.assign(new EventEmitter(), {
    context: () => ({ newCDPSession: async () => session }),
  });
  const received = new ReceivedTransport();
  await captureResponses(page as unknown as Page, received);
  assert.deepEqual(commands, [
    { method: 'Network.enable', params: undefined },
    { method: 'Network.configureDurableMessages', params: {
      maxTotalBufferSize: 8 * 1024 * 1024, maxResourceBufferSize: 8 * 1024 * 1024,
    } },
  ]);
  page.emit('response', {
    headers: () => ({ 'content-type': 'application/json' }),
    text: async () => { throw new Error('private-token-and-body'); },
  });
  assert.equal(captureFailure(received)?.pendingBodies, 1);
  await new Promise<void>(resolve => setImmediate(resolve));
  assert.deepEqual(captureFailure(received), {
    byteLimit: 0, bodyReadFailures: 1, unsupportedStreams: 0, pendingBodies: 0, retainedBytes: 0,
  });
  page.emit('response', {
    headers: () => ({ 'content-type': 'text/html', 'content-length': String(8 * 1024 * 1024 + 1) }),
    text: () => { assert.fail('Oversized body must not be read'); },
  });
  session.emit('Network.responseReceived', { response: { mimeType: 'text/event-stream' }, type: 'Fetch' });
  assert.deepEqual(captureFailure(received), {
    byteLimit: 1, bodyReadFailures: 1, unsupportedStreams: 1, pendingBodies: 0, retainedBytes: 0,
  });
  received.record('observed-secret');
  assert.equal(received.contains('observed-secret'), true, 'A captured leak remains measurable');
  assert.equal(received.contains('absent', false), false, 'Positive receipt probes can still wait');
});

test('privacy capture includes public script, style and error bodies', async () => {
  const session = Object.assign(new EventEmitter(), { send: async () => {} });
  const page = Object.assign(new EventEmitter(), { context: () => ({ newCDPSession: async () => session }) });
  const received = new ReceivedTransport();
  await captureResponses(page as unknown as Page, received);
  for (const type of ['application/javascript', 'text/javascript; charset=utf-8', 'application/ecmascript',
    'text/css', 'application/json', 'text/html', 'text/plain']) {
    page.emit('response', { headers: () => ({ 'content-type': type }), status: () => 500,
      text: async () => `private-sentinel-${type}` });
    await new Promise<void>(resolve => setImmediate(resolve));
    assert.equal(received.contains(`private-sentinel-${type}`), true, type);
  }
  assert.equal(received.contains('absent'), false);
});

test('body failure diagnostics use bounded categories without error text or URL secrets', async t => {
  const messages: string[] = [];
  t.mock.method(process.stderr, 'write', (chunk: string) => { messages.push(String(chunk)); return true; });
  const session = Object.assign(new EventEmitter(), { send: async () => {} });
  const page = Object.assign(new EventEmitter(), {
    context: () => ({ newCDPSession: async () => session }), isClosed: () => false,
  });
  const received = new ReceivedTransport();
  await captureResponses(page as unknown as Page, received);
  const cases: [unknown, string][] = [
    [new Error('Protocol error (Network.getResponseBody): Request content was evicted from inspector cache'), 'body-evicted'],
    [new Error('Protocol error (Network.getResponseBody): No data found for resource with given identifier'), 'resource-unavailable'],
    [new Error('Target page, context or browser has been closed'), 'target-closed'],
    [new Error('response.body: net::ERR_BLOCKED_BY_ORB'), 'blocked-by-orb'],
    [new Error('response.body: net::ERR_FAILED'), 'request-failed'],
    [new Error('Protocol error (Network.getResponseBody): private-password'), 'protocol-error'],
    [Object.assign(new Error('private-password https://private.test/secret'), { name: 'private-error-name' }), 'unknown'],
    ['private-password', 'unknown'],
    [new Error('another private-password'), 'unknown'],
  ];
  for (const [error] of cases) {
    page.emit('response', {
      headers: () => ({ 'content-type': 'text/html' }), status: () => 200,
      url: () => 'https://assets.test/private-password?token=private-password',
      request: () => ({ resourceType: () => 'font', failure: () => null }),
      text: async () => { throw error; },
    });
    await new Promise<void>(resolve => setImmediate(resolve));
  }
  assert.equal(messages.length, 8, 'Diagnostics must remain bounded');
  const records = messages.map(line => JSON.parse(line.slice(line.indexOf('{'))));
  assert.deepEqual(records.map(record => record.error), cases.slice(0, 8).map(([, category]) => category));
  assert(records.every(record => record.origin === 'https://assets.test' && /^[a-f0-9]{64}$/.test(record.pathSha256)));
  assert.doesNotMatch(messages.join(''), /private-password|private-error-name|private\.test/);
  assert.equal(captureFailure(received)?.bodyReadFailures, 9);
  received.record('observed-secret');
  assert.equal(received.contains('observed-secret'), true);
});
