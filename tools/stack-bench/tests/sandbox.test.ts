import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { ALLOW, DENY, sandboxProbeMode, writeSandbox } from '../src/runtime/sandbox.js';

test('appliance isolation replaces the single-host model CLI sandbox probe', () => {
  assert.equal(sandboxProbeMode({ appliance: true, stackRequired: true }), 'container-isolation');
  assert.equal(sandboxProbeMode({ appliance: false, stackRequired: true }), 'direct-cli');
  assert.equal(sandboxProbeMode({ appliance: true, stackRequired: false }), 'not-required');
  assert.equal(sandboxProbeMode({ appliance: true, stackRequired: true, explicitlySkipped: true }),
    'explicitly-skipped');
});

test('the sandbox denies migrated TypeScript harness source', () => {
  assert.ok(DENY.includes('Read(**/stack-bench/*.ts)'));
});

test('the sandbox denies the staged tree that runs the benchmark', () => {
  assert.ok(DENY.includes('Read(**/stack-bench/dist/**)'));
});

test('sandbox settings contain the shared allow and deny policy', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-sandbox-'));
  try {
    const path = writeSandbox(root);
    assert.deepEqual(JSON.parse(readFileSync(path, 'utf8')), {
      permissions: { allow: ALLOW, deny: DENY },
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
