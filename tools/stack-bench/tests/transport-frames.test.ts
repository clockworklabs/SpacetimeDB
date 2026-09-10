import assert from 'node:assert/strict';
import test from 'node:test';
import { brotliCompressSync, gzipSync } from 'node:zlib';

import { ReceivedTransport, transportFrameText } from '../grader/transport-frames.js';

const message = 'subscription update with support-secret-619 inline';

test('compressed SpacetimeDB frames decode to their message text', () => {
  assert.equal(transportFrameText(Buffer.concat([Buffer.from([2]), gzipSync(message)])), message);
  assert.equal(transportFrameText(Buffer.concat([Buffer.from([1]), brotliCompressSync(message)])),
    message);
  assert.equal(transportFrameText(Buffer.concat([Buffer.from([0]), Buffer.from(message)])),
    `\u0000${message}`);
});

test('other frames keep their bytes', () => {
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
  assert.equal(received.contains('absent', false), false);
  received.pending = 0;
  const bounded = new ReceivedTransport(20);
  bounded.record('first-secret');
  bounded.record('second-secret');
  assert.equal(bounded.contains('second-secret'), true);
  assert.throws(() => bounded.contains('first-secret'), /incomplete/);
  const oversized = new ReceivedTransport(5);
  oversized.record('too-large');
  assert.throws(() => oversized.contains('large'), /incomplete/);
  received.incomplete = true;
  assert.throws(() => received.contains('absent'), /incomplete/);
});
