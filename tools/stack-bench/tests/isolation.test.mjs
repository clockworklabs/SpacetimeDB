import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const AGENT = join(ROOT, 'agent.mjs');

const args = app => [AGENT, '--mode', 'build', '--backend', 'spacetime', '--track', 'loop',
  '--level', '1', '--app', app];

test('an unavailable isolation image refuses a coding session instead of falling back to the host', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-isolation-'));
  try {
    assert.throws(() => execFileSync(process.execPath, args(app), {
      env: { ...process.env, STACK_BENCH_IMAGE: 'stack-bench-image-that-does-not-exist' },
      stdio: 'pipe',
    }), /Command failed/);
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('prompt review does not require Docker or mutate the application directory', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-isolation-'));
  try {
    const prompt = execFileSync(process.execPath, [...args(app), '--print-prompt'], {
      env: { ...process.env, STACK_BENCH_IMAGE: 'stack-bench-image-that-does-not-exist' },
      encoding: 'utf8', stdio: 'pipe',
    });
    assert.match(prompt, /Build the application described below/);
    assert.equal(readdirSync(app).length, 0);
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('host execution flags are rejected rather than opening a second runtime path', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-isolation-'));
  try {
    assert.throws(() => execFileSync(process.execPath, [...args(app), '--print-prompt', '--diagnostic-host'], {
      stdio: 'pipe',
    }), error => error.status === 2 && /Unknown argument: --diagnostic-host/.test(String(error.stderr)));
  } finally { rmSync(app, { recursive: true, force: true }); }
});
