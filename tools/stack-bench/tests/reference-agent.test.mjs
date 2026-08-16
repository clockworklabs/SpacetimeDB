import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { parseReferenceAgentArgs, prepareReferenceSource,
  referenceDevCommand } from '../reference-agent.mjs';

function argv({ mode = 'build', level = '2', runIndex = '0' } = {}) {
  return ['node', 'reference-agent.mjs', '--mode', mode, '--backend', 'mongodb',
    '--app', '/work/reference', '--track', 'ecommerce', '--level', level,
    '--run-index', runIndex];
}

test('the model-free reference builder accepts any explicit positive level', () => {
  assert.equal(parseReferenceAgentArgs(argv({ level: '1' })).level, 1);
  assert.equal(parseReferenceAgentArgs(argv({ level: '2' })).level, 2);
  assert.equal(parseReferenceAgentArgs(argv({ level: '3' })).level, 3);
});

test('the model-free reference builder rejects unsupported modes and malformed scope', () => {
  assert.throws(() => parseReferenceAgentArgs(argv({ mode: 'upgrade' })), /only build mode/);
  assert.throws(() => parseReferenceAgentArgs(argv({ level: '0' })), /positive integer level/);
  assert.throws(() => parseReferenceAgentArgs(argv({ level: '1.5' })), /positive integer level/);
  assert.throws(() => parseReferenceAgentArgs(argv({ runIndex: '-1' })), /non-negative integer run-index/);
});

test('reference clients are explicitly reachable outside their build container', () => {
  assert.equal(referenceDevCommand('reference-server'),
    'exec npm run dev > /tmp/reference-server.log 2>&1');
  assert.equal(referenceDevCommand('reference-client', { networkVisible: true }),
    'exec npm run dev -- --host 0.0.0.0 > /tmp/reference-client.log 2>&1');
  assert.throws(() => referenceDevCommand('../unsafe'), /unsafe reference log name/);
});

test('reference adapter seeds an empty campaign app from the exact registered fixture', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-'));
  try {
    const args = { backend: 'mongodb', track: 'ecommerce', level: 1,
      app: join(root, 'app') };
    const seeded = prepareReferenceSource(args);
    assert.equal(seeded.fixture.id, 'ecommerce-l1-mongodb');
    assert.equal(seeded.seeded, true);
    assert.equal(prepareReferenceSource(args).seeded, false);
    writeFileSync(join(args.app, 'unexpected.txt'), 'different source');
    assert.throws(() => prepareReferenceSource(args), /contains source other than/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
