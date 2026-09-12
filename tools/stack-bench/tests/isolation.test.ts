import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { parseAgentArgs, refreshCodingInvocationCredentials } from '../commands/agent.js';
import { compiledEntrypoint } from '../src/package-root.js';

const AGENT = compiledEntrypoint('commands', 'agent.js');

const args = (app: string) => [AGENT, '--mode', 'build', '--backend', 'spacetime', '--track', 'loop',
  '--level', '1', '--app', app];

function isCommandFailure(error: unknown): error is Error & { status: number; stderr: unknown } {
  return error instanceof Error
    && typeof Reflect.get(error, 'status') === 'number'
    && Reflect.has(error, 'stderr');
}

test('an unavailable isolation image refuses a coding session instead of falling back to the host', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-isolation-'));
  try {
    assert.throws(() => execFileSync(process.execPath, args(app), {
      env: { ...process.env, STACK_BENCH_IMAGE: 'stack-bench-image-that-does-not-exist' },
      stdio: 'pipe',
    }), error => isCommandFailure(error) && error.status === 2
      && /isolated build unavailable: cannot verify isolation image stack-bench-image-that-does-not-exist/.test(String(error.stderr))
      && /benchmark coding sessions require the isolation container/.test(String(error.stderr)));
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
    }), error => isCommandFailure(error)
      && /Unknown option '--diagnostic-host'/.test(String(error.stderr)));
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('agent arguments reject invalid modes and partial numbers', () => {
  const base = ['node', 'agent', '--backend', 'postgres', '--app', 'app'];
  const codex = [...base, '--mode', 'build', '--provider', 'openai'];
  assert.throws(() => parseAgentArgs(codex), /--model is required/);
  assert.equal(parseAgentArgs([...codex, '--model', 'gpt-5.3-codex']).provider, 'openai');
  assert.throws(() => parseAgentArgs([...base, '--mode', 'typo']), /--mode must be/);
  assert.throws(() => parseAgentArgs([...base, '--mode', 'build', '--level', '2junk']),
    /--level must be a positive integer/);
  assert.throws(() => parseAgentArgs([...base, '--mode', 'build', '--run-index', '1junk']),
    /--run-index must be a non-negative integer/);
});


test('coding invocation reloads the selected key file and refuses a billing-mode switch', () => {
  const directory = mkdtempSync(join(tmpdir(), 'stack-bench-refresh-'));
  const keyFile = join(directory, 'key');
  try {
    writeFileSync(keyFile, 'first-fake-key');
    const first = refreshCodingInvocationCredentials({ provider: 'anthropic', keyFile,
      expectedMode: null, env: {} });
    writeFileSync(keyFile, 'renewed-fake-key');
    const second = refreshCodingInvocationCredentials({ provider: 'anthropic', keyFile,
      expectedMode: first.mode, env: {} });
    assert.equal(second.env.STACK_BENCH_AGENT_API_KEY, 'renewed-fake-key');
    assert.throws(() => refreshCodingInvocationCredentials({ provider: 'anthropic',
      expectedMode: first.mode, env: { CLAUDE_CODE_OAUTH_TOKEN: 'fake-subscription-token' } }), /billing mode changed/);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
