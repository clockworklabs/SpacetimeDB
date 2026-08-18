import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { aggregateCodingSessionResults, codingSessionInterruption,
  parseCodingSessionResult, runCodingSessionWithRecovery } from '../agent.mjs';
import { createBackendLease, readBackendLease, writeBackendLease } from '../backend-lease.mjs';
import { recoverStoppedBuildContainer } from '../container/recover-build-container.mjs';

test('provider mid-response errors resume the exact paid session', () => {
  const sessionId = '950df556-38bb-429c-aee9-1af4a00a6c7a';
  const result = parseCodingSessionResult(`${JSON.stringify({ type: 'result', is_error: true,
    terminal_reason: 'api_error', session_id: sessionId, total_cost_usd: 3.25 })}\n`);
  assert.deepEqual(codingSessionInterruption(Object.assign(new Error('exit'), { status: 1 }), result), {
    kind: 'provider-api-error', resumeSession: sessionId, recoverStoppedContainer: false,
    terminalReason: 'api_error', providerStatus: null,
  });
});

test('only a non-OOM forced exit is eligible for container recovery', () => {
  const failure = diagnostic => Object.assign(new Error('killed'), { status: 137,
    stderr: `STACK_BENCH_CODING_PROCESS_DIAGNOSTIC ${JSON.stringify(diagnostic)}\n` });
  const killed = codingSessionInterruption(failure({ status: 137,
    container: { ExitCode: 143, OOMKilled: false }, cgroupMemory: 'oom 0\noom_kill 0\n' }), null);
  assert.equal(killed.kind, 'coding-process-killed');
  assert.equal(killed.recoverStoppedContainer, true);
  assert.equal(codingSessionInterruption(failure({ status: 137,
    container: { ExitCode: 137, OOMKilled: true }, cgroupMemory: 'oom 1\noom_kill 1\n' }), null), null);
});

test('retry accounting includes every paid invocation', () => {
  const combined = aggregateCodingSessionResults([
    { is_error: true, session_id: 'first', total_cost_usd: 3.8464, num_turns: 94,
      usage: { input_tokens: 1, output_tokens: 2, cache_creation_input_tokens: 3,
        cache_read_input_tokens: 4 } },
    { is_error: false, session_id: 'first', total_cost_usd: 1.25, num_turns: 5,
      usage: { input_tokens: 10, output_tokens: 20, cache_creation_input_tokens: 30,
        cache_read_input_tokens: 40 } },
  ]);
  assert.equal(combined.total_cost_usd, 5.0964);
  assert.equal(combined.num_turns, 99);
  assert.deepEqual(combined.usage, { input_tokens: 11, output_tokens: 22,
    cache_creation_input_tokens: 33, cache_read_input_tokens: 44 });
  assert.equal(combined.is_error, false);
});

test('the retry loop resumes in place and deducts the failed call from the cost cap', () => {
  const sessionId = '950df556-38bb-429c-aee9-1af4a00a6c7a';
  const calls = [];
  const coding = runCodingSessionWithRecovery({ prompt: 'full original prompt', retryLimit: 2,
    maxBudgetUsd: 10,
    invoke(request) {
      calls.push(request);
      if (calls.length === 1) {
        const output = JSON.stringify({ is_error: true, terminal_reason: 'api_error',
          session_id: sessionId, total_cost_usd: 3.5, num_turns: 4,
          usage: { input_tokens: 1, output_tokens: 2, cache_creation_input_tokens: 3,
            cache_read_input_tokens: 4 } });
        throw Object.assign(new Error('provider interrupted'), { status: 1, stdout: output });
      }
      return JSON.stringify({ is_error: false, session_id: sessionId, total_cost_usd: 1.25,
        num_turns: 2, usage: { input_tokens: 5, output_tokens: 6,
          cache_creation_input_tokens: 7, cache_read_input_tokens: 8 } });
    } });
  assert.equal(coding.spawnError, null);
  assert.equal(coding.result.total_cost_usd, 4.75);
  assert.equal(calls.length, 2);
  assert.equal(calls[0].input, 'full original prompt');
  assert.equal(calls[0].maxBudgetUsd, 10);
  assert.equal(calls[1].resumeSession, sessionId);
  assert.equal(calls[1].maxBudgetUsd, 6.5);
  assert.match(calls[1].input, /Continue the same task/);
  assert.equal(coding.interruptions[0].kind, 'provider-api-error');
});

test('a non-OOM kill recovers the container without pretending to resume a missing session', () => {
  const calls = [];
  const coding = runCodingSessionWithRecovery({ prompt: 'full prompt', retryLimit: 1,
    invoke(request) {
      calls.push(request);
      if (calls.length === 1) {
        const diagnostic = { status: 137, container: { ExitCode: 143, OOMKilled: false },
          cgroupMemory: 'oom 0\noom_kill 0\n' };
        throw Object.assign(new Error('killed'), { status: 137,
          stderr: `STACK_BENCH_CODING_PROCESS_DIAGNOSTIC ${JSON.stringify(diagnostic)}\n` });
      }
      return JSON.stringify({ is_error: false, session_id: 'replacement', total_cost_usd: 1,
        num_turns: 1, usage: {} });
    } });
  assert.equal(coding.spawnError, null);
  assert.equal(calls[1].recoverStoppedContainer, true);
  assert.equal(calls[1].resumeSession, null);
  assert.match(calls[1].input, /Continue this task from the existing files/);
});

test('stopped-container recovery removes only the exact authenticated lease target', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-build-recovery-'));
  try {
    const path = join(root, 'lease.json');
    const lease = createBackendLease({ runId: 'recover-build-test', backend: 'mongodb',
      track: 'ecommerce', runIndex: 0, database: 'stackbench_ecom_run0',
      container: { name: 'stack-bench-mongodb', id: 'database-id' } });
    lease.state = 'active';
    lease.resources.buildContainer = { name: 'leased-build', id: 'a'.repeat(64),
      image: 'image-id', owned: true, running: false, networkMode: 'bridge' };
    writeBackendLease(path, lease);
    const calls = [];
    const recovered = recoverStoppedBuildContainer({
      existing: { id: 'a'.repeat(64), running: false }, containerName: 'leased-build',
      leaseContext: { path, lease }, backend: 'mongodb',
      execute(command, args) { calls.push([command, args]); return { status: 0, stdout: '' }; },
    });
    assert.deepEqual(calls, [['docker', ['rm', 'a'.repeat(64)]]]);
    assert.equal(recovered.lease.resources.buildContainer, null);
    assert.equal(readBackendLease(path).resources.buildContainer, null);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('stopped-container recovery fails closed on a lease mismatch', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-build-recovery-'));
  try {
    const path = join(root, 'lease.json');
    const lease = createBackendLease({ runId: 'recover-build-test', backend: 'mongodb',
      track: 'ecommerce', runIndex: 0, database: 'stackbench_ecom_run0',
      container: { name: 'stack-bench-mongodb', id: 'database-id' } });
    lease.state = 'active';
    lease.resources.buildContainer = { name: 'leased-build', id: 'a'.repeat(64),
      image: 'image-id', owned: true, running: false, networkMode: 'bridge' };
    writeBackendLease(path, lease);
    let executed = false;
    assert.throws(() => recoverStoppedBuildContainer({
      existing: { id: 'b'.repeat(64), running: false }, containerName: 'leased-build',
      leaseContext: { path, lease }, backend: 'mongodb',
      execute() { executed = true; return { status: 0 }; },
    }), /does not match/);
    assert.equal(executed, false);
    assert.equal(readBackendLease(path).resources.buildContainer.id, 'a'.repeat(64));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('Docker replaces a stopped leased build container and preserves its app mount', {
  skip: process.env.STACK_BENCH_DOCKER_RECOVERY_SMOKE !== '1',
}, () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-recovery-docker-'));
  const app = join(root, 'app');
  const leasePath = join(root, 'lease.json');
  const containerName = `stack-bench-${root.split(/[\\/]/).at(-1)}`;
  const docker = (args, options = {}) => spawnSync('docker', args,
    { encoding: 'utf8', timeout: 120_000, ...options });
  try {
    mkdirSync(app);
    const databaseContainer = docker(['inspect', '--format', '{{.Id}}', 'stack-bench-mongodb']);
    assert.equal(databaseContainer.status, 0, databaseContainer.stderr);
    const lease = createBackendLease({ runId: 'recover-build-docker-smoke', backend: 'mongodb',
      track: 'ecommerce', runIndex: 91, database: 'stackbench_ecom_run91',
      container: { name: 'stack-bench-mongodb', id: databaseContainer.stdout.trim() } });
    lease.state = 'active';
    writeBackendLease(leasePath, lease);
    const env = { ...process.env, STACK_BENCH_LEASE: leasePath,
      STACK_BENCH_LEASE_TOKEN: lease.ownershipToken };
    const baseArgs = ['tools/stack-bench/container/run-build.mjs', '--prepare-only',
      '--app', app, '--backend', 'mongodb', '--image',
      process.env.STACK_BENCH_BUILD_IMAGE
        ?? 'stack-bench-build@sha256:b404d26138ea16be07a672389981e7e8b89d7740570f9baa3ddd4d0e96336c10'];
    const first = spawnSync(process.execPath, baseArgs,
      { encoding: 'utf8', env, timeout: 180_000 });
    assert.equal(first.status, 0, first.stderr);
    const firstResult = JSON.parse(first.stdout);
    writeFileSync(join(app, 'preserved.txt'), 'preserved\n');
    const stopped = docker(['stop', firstResult.containerName]);
    assert.equal(stopped.status, 0, stopped.stderr);
    const second = spawnSync(process.execPath,
      [baseArgs[0], '--recover-stopped-container', ...baseArgs.slice(1)],
      { encoding: 'utf8', env, timeout: 180_000 });
    assert.equal(second.status, 0, second.stderr);
    const secondResult = JSON.parse(second.stdout);
    assert.notEqual(firstResult.identity, secondResult.identity);
    const mounted = docker(['exec', secondResult.containerName, 'cat', '/app/preserved.txt']);
    assert.equal(mounted.status, 0, mounted.stderr);
    assert.equal(mounted.stdout.trim(), 'preserved');
  } finally {
    const exact = docker(['inspect', '--format', '{{.Id}}', containerName]);
    if (exact.status === 0 && exact.stdout.trim()) docker(['rm', '-f', exact.stdout.trim()]);
    rmSync(root, { recursive: true, force: true });
  }
});
