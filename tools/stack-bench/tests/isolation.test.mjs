import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const AGENT = join(ROOT, 'agent.mjs');

const args = app => [AGENT, '--mode', 'build', '--backend', 'spacetime', '--track', 'loop',
  '--level', '1', '--app', app, '--print-prompt'];

test('an unavailable isolation image refuses instead of falling back to the host', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-isolation-'));
  try {
    assert.throws(() => execFileSync(process.execPath, args(app), {
      env: { ...process.env, STACK_BENCH_IMAGE: 'stack-bench-image-that-does-not-exist' },
      stdio: 'pipe',
    }), /Command failed/);
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('host execution flags are rejected rather than opening a second runtime path', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-isolation-'));
  try {
    assert.throws(() => execFileSync(process.execPath, [...args(app), '--diagnostic-host'], {
      stdio: 'pipe',
    }), error => error.status === 2 && /Unknown argument: --diagnostic-host/.test(String(error.stderr)));
  } finally { rmSync(app, { recursive: true, force: true }); }
});
