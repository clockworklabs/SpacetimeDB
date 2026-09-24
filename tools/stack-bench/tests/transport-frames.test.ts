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
