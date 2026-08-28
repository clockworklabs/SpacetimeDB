import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { aggregateCodingSessionResults, codingSessionInterruption,
  parseCodingSessionResult, providerSessionFailure,
  runCodingSessionWithRecovery } from '../src/agents/coding-session-recovery.mjs';
import { agentSessionFailure } from '../src/agents/agent-adapter-contract.mjs';
import { createBackendLease, readBackendLease, writeBackendLease } from '../src/runtime/backend-lease.mjs';
import { credentialBrokerDiagnostics, reconcileCredentialBrokerReceipt,
  startCredentialBroker, stopCredentialBroker } from '../container/credential-broker.mjs';
import { recoverStoppedBuildContainer } from '../container/recover-build-container.mjs';

function brokerReceipt(costUsd, maxBudgetUsd = 10) {
  return { schemaVersion: 2, source: 'credential-broker', model: 'claude-sonnet-5',
    maxBudgetUsd, costUsd, cliCostUsd: costUsd, calculatedCostUsd: costUsd,
    usage: { input: 1, output: 1, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0 },
    pricingRates: { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6,
      cacheRead: 0.3 },
    complete: true, reconciled: true, error: null };
}

test('provider mid-response errors resume the exact paid session', () => {
  const sessionId = '950df556-38bb-429c-aee9-1af4a00a6c7a';
  const result = parseCodingSessionResult(`${JSON.stringify({ type: 'result', is_error: true,
    terminal_reason: 'api_error', api_error_status: 503,
    session_id: sessionId, total_cost_usd: 3.25 })}\n`);
  assert.deepEqual(codingSessionInterruption(Object.assign(new Error('exit'), { status: 1 }), result), {
    kind: 'provider-api-error', resumeSession: sessionId, recoverStoppedContainer: false,
    terminalReason: 'api_error', providerStatus: 503,
  });
  assert.equal(codingSessionInterruption(null, { terminal_reason: 'api_error',
    api_error_status: 401, session_id: sessionId }), null);
});

test('provider connection failures resume the exact paid session', () => {
  const sessionId = '950df556-38bb-429c-aee9-1af4a00a6c7a';
  const result = { type: 'result', is_error: true, terminal_reason: 'cost_receipt_error',
    session_id: sessionId,
    result: 'API Error: Unable to connect to API (ConnectionRefused)',
    total_cost_usd: 1.25, num_turns: 2, usage: {},
    stack_bench_cost_receipt: brokerReceipt(1.25) };
  assert.deepEqual(codingSessionInterruption(null, result), {
    kind: 'provider-connection-error', resumeSession: sessionId,
    recoverStoppedContainer: false, terminalReason: 'cost_receipt_error', providerStatus: null,
  });
  const calls = [];
  const coding = runCodingSessionWithRecovery({ prompt: 'build', retryLimit: 1,
    maxBudgetUsd: 10,
    invoke(request) {
      calls.push(request);
      return JSON.stringify(calls.length === 1 ? result : {
        is_error: false, session_id: sessionId, total_cost_usd: 0.5,
        num_turns: 1, usage: {}, stack_bench_cost_receipt: brokerReceipt(0.5, 8.75),
      });
    } });
  assert.equal(coding.spawnError, null);
  assert.equal(calls.length, 2);
  assert.equal(calls[1].resumeSession, sessionId);
});

test('a refused local credential broker is a harness failure and cannot auto-retry unknown cost', () => {
  const sessionId = '950df556-38bb-429c-aee9-1af4a00a6c7a';
  const result = { type: 'result', is_error: true, terminal_reason: 'cost_receipt_error',
    session_id: sessionId,
    result: 'API Error: Unable to connect to API (ConnectionRefused)',
    total_cost_usd: 1.25, num_turns: 2, usage: {},
    stack_bench_credential_broker: { endpointKind: 'local-credential-broker',
      child: { exitCode: 1, signal: null, stderrTail: 'broker failed' },
      ledger: { complete: false, reservedUsd: 1.2 } },
    stack_bench_cost_receipt: { ...brokerReceipt(1.25), complete: false,
      reconciled: false, error: 'broker ledger is incomplete' } };
  assert.deepEqual(providerSessionFailure(result), {
    code: 'credential-broker-unavailable', status: null,
  });
  assert.deepEqual(codingSessionInterruption(null, result), {
    kind: 'credential-broker-unavailable', resumeSession: sessionId,
    recoverStoppedContainer: false, terminalReason: 'cost_receipt_error', providerStatus: null,
  });
  let calls = 0;
  const coding = runCodingSessionWithRecovery({ prompt: 'build', retryLimit: 3,
    maxBudgetUsd: 10, invoke() { calls += 1; return JSON.stringify(result); } });
  assert.equal(calls, 1);
  assert.match(coding.spawnError, /broker failed without a complete reconciled cost receipt/);
  assert.equal(coding.interruptions[0].kind, 'credential-broker-unavailable');
  assert.equal(agentSessionFailure({ ok: false,
    providerMetadata: { failureCode: 'credential-broker-unavailable' } }).kind,
  'harness_failure');
});

test('a real broker child exit reaches the run result and stops recovery as a harness failure', async () => {
  const model = 'test-model';
  const maxBudgetUsd = 10;
  const pricingRates = { input: 3, output: 15, cacheWrite5m: 3.75,
    cacheWrite1h: 6, cacheRead: 0.3 };
  const broker = startCredentialBroker({ mode: 'api-key',
    environment: { name: 'ANTHROPIC_API_KEY', value: 'provider-secret-value-1234567890' },
    mount: null }, { networkMode: 'host', deadlineMs: 10_000, model,
    maxBudgetUsd, pricingRates });
  broker.child.kill('SIGTERM');
  await once(broker.child, 'exit');
  const ledger = await stopCredentialBroker(broker);
  const diagnostics = credentialBrokerDiagnostics(broker);
  const reconciled = reconcileCredentialBrokerReceipt({ ledger,
    cliResult: { type: 'result', is_error: true, terminal_reason: 'api_error',
      session_id: '950df556-38bb-429c-aee9-1af4a00a6c7a', total_cost_usd: 0,
      usage: { input_tokens: 0, output_tokens: 0, cache_read_input_tokens: 0,
        cache_creation_input_tokens: 0 },
      result: 'API Error: Unable to connect to API (ConnectionRefused)' },
    model, maxBudgetUsd, pricingRates, brokerDiagnostics: diagnostics });
  assert.equal(reconciled.ok, true);
  assert.equal(reconciled.result.stack_bench_cost_receipt.reconciled, true);
  assert.ok(reconciled.result.stack_bench_credential_broker.child.exitCode !== null
    || reconciled.result.stack_bench_credential_broker.child.signal !== null);
  let calls = 0;
  const coding = runCodingSessionWithRecovery({ prompt: 'build', retryLimit: 3,
    maxBudgetUsd, invoke() { calls += 1; return JSON.stringify(reconciled.result); } });
  assert.equal(calls, 1);
  assert.match(coding.spawnError, /local credential broker failed/);
  assert.equal(coding.interruptions[0].kind, 'credential-broker-unavailable');
  assert.equal(coding.interruptions[0].costUsd, 0);
  const failureCode = providerSessionFailure(coding.result).code;
  const agentResult = { ok: false, providerMetadata: { failureCode,
    credentialBroker: coding.result.stack_bench_credential_broker } };
  assert.equal(agentSessionFailure(agentResult).kind, 'harness_failure');
  assert.equal(agentResult.providerMetadata.credentialBroker.termination.exited, true);
});

test('a provider throttle is classified as waitable, with or without a session', () => {
  const throttled = parseCodingSessionResult(JSON.stringify({ type: 'result', is_error: true,
    terminal_reason: 'api_error', api_error_status: 429, session_id: 'paid-session' }));
  assert.deepEqual(codingSessionInterruption(Object.assign(new Error('exit'), { status: 1 }), throttled), {
    kind: 'provider-throttled', resumeSession: 'paid-session', recoverStoppedContainer: false,
    terminalReason: 'api_error', providerStatus: 429,
  });
  const beforeSession = parseCodingSessionResult(JSON.stringify({ type: 'result', is_error: true,
    terminal_reason: 'api_error', api_error_status: 529 }));
  assert.deepEqual(codingSessionInterruption(null, beforeSession), {
    kind: 'provider-throttled', resumeSession: null, recoverStoppedContainer: false,
    terminalReason: 'api_error', providerStatus: 529,
  });
});

test('throttle waits back off, resume the paid session, and do not spend interruption retries', () => {
  const calls = [];
  const waits = [];
  const logs = [];
  const coding = runCodingSessionWithRecovery({ prompt: 'build the app', retryLimit: 0,
    sleep: ms => waits.push(ms), log: message => logs.push(message),
    invoke(request) {
      calls.push(request);
      if (calls.length <= 2) {
        return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
          api_error_status: 429, session_id: 'paid-session', total_cost_usd: 0.001 });
      }
      return JSON.stringify({ is_error: false, session_id: 'paid-session',
        total_cost_usd: 2, num_turns: 3, usage: {} });
    } });
  assert.equal(coding.spawnError, null);
  assert.equal(calls.length, 3);
  assert.deepEqual(waits, [60_000, 120_000]);
  assert.equal(calls[1].resumeSession, 'paid-session');
  assert.equal(calls[2].resumeSession, 'paid-session');
  assert.deepEqual(coding.throttle,
    { waits: 2, waitedMs: 180_000, maxWaitMs: 300 * 60_000, jitterMs: 0 });
  assert.equal(coding.interruptions.length, 2);
  assert.ok(coding.interruptions.every(item => item.kind === 'provider-throttled'));
  assert.match(logs[0], /provider throttled \(status 429\)/);
});

test('a throttle before any session restarts from the existing files after waiting', () => {
  const calls = [];
  const waits = [];
  const coding = runCodingSessionWithRecovery({ prompt: 'full original prompt', retryLimit: 0,
    sleep: ms => waits.push(ms), log: () => {},
    invoke(request) {
      calls.push(request);
      if (calls.length === 1) {
        return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
          api_error_status: 429 });
      }
      return JSON.stringify({ is_error: false, session_id: 'fresh', total_cost_usd: 1,
        num_turns: 1, usage: {} });
    } });
  assert.equal(coding.spawnError, null);
  assert.deepEqual(waits, [60_000]);
  assert.equal(calls[1].resumeSession, null);
  assert.match(calls[1].input, /Continue this task from the existing files/);
});

test('an unbroken throttle stops at the wait budget with a throttle-specific failure', () => {
  const waits = [];
  let calls = 0;
  const coding = runCodingSessionWithRecovery({ prompt: 'build the app', retryLimit: 3,
    throttleMaxWaitMs: 5 * 60_000, sleep: ms => waits.push(ms), log: () => {},
    invoke() {
      calls += 1;
      return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
        api_error_status: 429, session_id: 'paid-session',
        result: 'API Error: Request rejected (429)' });
    } });
  // 60s + 120s fit inside five minutes; the 300s third wait would exceed it.
  assert.deepEqual(waits, [60_000, 120_000]);
  assert.equal(calls, 3);
  assert.match(coding.spawnError, /provider stayed throttled \(status 429\) after 3 minutes/);
  assert.deepEqual(coding.throttle,
    { waits: 2, waitedMs: 180_000, maxWaitMs: 5 * 60_000, jitterMs: 0 });
});

test('a stable throttle offset spreads concurrent retries without changing retry accounting', () => {
  const waits = [];
  let calls = 0;
  const coding = runCodingSessionWithRecovery({ prompt: 'build', retryLimit: 0,
    throttleJitterMs: 17_000, sleep: ms => waits.push(ms), log: () => {},
    invoke() {
      calls += 1;
      return calls === 1
        ? JSON.stringify({ is_error: true, terminal_reason: 'api_error', api_error_status: 429 })
        : JSON.stringify({ is_error: false, session_id: 'done', usage: {} });
    } });
  assert.deepEqual(waits, [77_000]);
  assert.deepEqual(coding.throttle,
    { waits: 1, waitedMs: 77_000, maxWaitMs: 300 * 60_000, jitterMs: 17_000 });
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

test('a bounded coding-session timeout continues from the existing app', () => {
  const timeout = Object.assign(new Error('spawnSync docker ETIMEDOUT'), { code: 'ETIMEDOUT' });
  assert.deepEqual(codingSessionInterruption(timeout, null), {
    kind: 'coding-session-timeout', resumeSession: null, recoverStoppedContainer: false,
    terminalReason: null, providerStatus: null,
  });
  const calls = [];
  const coding = runCodingSessionWithRecovery({ prompt: 'build the app', retryLimit: 1,
    invoke(request) {
      calls.push(request);
      if (calls.length === 1) throw timeout;
      return JSON.stringify({ is_error: false, session_id: 'replacement', total_cost_usd: 1,
        num_turns: 1, usage: {} });
    } });
  assert.equal(coding.spawnError, null);
  assert.equal(calls.length, 2);
  assert.match(calls[1].input, /Continue this task from the existing files/);
});

test('retry accounting includes every paid invocation', () => {
  const combined = aggregateCodingSessionResults([
    { is_error: true, session_id: 'first', total_cost_usd: 3.8464, num_turns: 94,
      stack_bench_cost_receipt: { id: 'first-receipt' },
      usage: { input_tokens: 1, output_tokens: 2, cache_creation_input_tokens: 3,
        cache_read_input_tokens: 4 } },
    { is_error: false, session_id: 'first', total_cost_usd: 1.25, num_turns: 5,
      stack_bench_cost_receipt: { id: 'second-receipt' },
      usage: { input_tokens: 10, output_tokens: 20, cache_creation_input_tokens: 30,
        cache_read_input_tokens: 40 } },
  ]);
  assert.equal(combined.total_cost_usd, 5.0964);
  assert.equal(combined.num_turns, 99);
  assert.deepEqual(combined.usage, { input_tokens: 11, output_tokens: 22,
    cache_creation_input_tokens: 33, cache_read_input_tokens: 44 });
  assert.deepEqual(combined.stack_bench_cost_receipts, [
    { invocation: 1, receipt: { id: 'first-receipt' } },
    { invocation: 2, receipt: { id: 'second-receipt' } },
  ]);
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
          api_error_status: 503, session_id: sessionId, total_cost_usd: 3.5, num_turns: 4,
          stack_bench_cost_receipt: brokerReceipt(3.5),
          usage: { input_tokens: 1, output_tokens: 2, cache_creation_input_tokens: 3,
            cache_read_input_tokens: 4 } });
        throw Object.assign(new Error('provider interrupted'), { status: 1, stdout: output });
      }
      return JSON.stringify({ is_error: false, session_id: sessionId, total_cost_usd: 1.25,
        stack_bench_cost_receipt: brokerReceipt(1.25, 6.5),
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

test('a capped session does not treat a numeric cost as a broker receipt', () => {
  let calls = 0;
  const coding = runCodingSessionWithRecovery({ prompt: 'build', retryLimit: 2,
    maxBudgetUsd: 10,
    invoke() {
      calls += 1;
      return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
        api_error_status: 503, session_id: 'paid-session', total_cost_usd: 2 });
    } });
  assert.equal(calls, 1);
  assert.match(coding.spawnError, /without a complete reconciled broker cost receipt/);
  assert.equal(coding.interruptions[0].costUsd, null);
});

test('a capped session does not retry with an unreconciled broker receipt', () => {
  let calls = 0;
  const receipt = { ...brokerReceipt(2), reconciled: false, error: 'cost mismatch' };
  const coding = runCodingSessionWithRecovery({ prompt: 'build', retryLimit: 2,
    maxBudgetUsd: 10,
    invoke() {
      calls += 1;
      return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
        api_error_status: 503, session_id: 'paid-session', total_cost_usd: 2,
        stack_bench_cost_receipt: receipt });
    } });
  assert.equal(calls, 1);
  assert.match(coding.spawnError, /without a complete reconciled broker cost receipt/);
  assert.equal(coding.interruptions[0].costUsd, null);
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
      track: 'ecommerce', runIndex: 0, database: 'app_ecom_run0',
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
      track: 'ecommerce', runIndex: 0, database: 'app_ecom_run0',
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
      track: 'ecommerce', runIndex: 91, database: 'app_ecom_run91',
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
    const hostConfigResult = docker(['inspect', '--format', '{{json .HostConfig}}',
      firstResult.containerName]);
    assert.equal(hostConfigResult.status, 0, hostConfigResult.stderr);
    const hostConfig = JSON.parse(hostConfigResult.stdout);
    assert.equal(hostConfig.ReadonlyRootfs, true);
    assert.deepEqual([...hostConfig.CapAdd].sort(),
      ['CAP_DAC_OVERRIDE', 'CAP_FOWNER', 'CAP_KILL', 'CAP_SETGID', 'CAP_SETUID']);
    assert.equal(hostConfig.CapDrop.map(value => value.replace(/^CAP_/, '')).includes('ALL'), true);
    assert.equal(hostConfig.SecurityOpt
      .map(value => value.replace(/:true$/, '')).includes('no-new-privileges'), true);
    assert.equal(hostConfig.PidsLimit, 512);
    assert.equal(hostConfig.Tmpfs['/tmp'], 'rw,nosuid,nodev,mode=1777');
    assert.equal(hostConfig.Tmpfs['/home/developer'],
      'rw,nosuid,nodev,uid=10001,gid=10001,mode=0700');
    assert.equal(hostConfig.Tmpfs['/home/developer/.claude'],
      'rw,nosuid,nodev,uid=10001,gid=10001,mode=0700');
    assert.equal(hostConfig.Tmpfs['/deps'], 'rw,nosuid,nodev,mode=0755');
    assert.equal(hostConfig.Tmpfs['/run/application'], 'rw,nosuid,nodev,mode=0700');
    assert.equal(hostConfig.Tmpfs['/root'], undefined);
    const agentCaps = docker(['exec', '--user', '10001:10001', firstResult.containerName,
      'sh', '-c', 'grep "^CapEff:" /proc/self/status']);
    assert.equal(agentCaps.status, 0, agentCaps.stderr);
    assert.match(agentCaps.stdout, /CapEff:\s+0+\s*$/);
    const sessionState = docker(['exec', '--user', '10001:10001', firstResult.containerName,
      'sh', '-c', 'test -w /home/developer/.claude/session-env']);
    assert.equal(sessionState.status, 0, sessionState.stderr);
    const dropped = docker(['exec', firstResult.containerName, '/usr/bin/setpriv',
      '--reuid=10001', '--regid=10001', '--init-groups', 'id', '-u']);
    assert.equal(dropped.status, 0, dropped.stderr);
    assert.equal(dropped.stdout.trim(), '10001');
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
    const reused = spawnSync(process.execPath, baseArgs,
      { encoding: 'utf8', env, timeout: 180_000 });
    assert.equal(reused.status, 0, reused.stderr);
    assert.equal(JSON.parse(reused.stdout).identity, secondResult.identity);
  } finally {
    const exact = docker(['inspect', '--format', '{{.Id}}', containerName]);
    if (exact.status === 0 && exact.stdout.trim()) docker(['rm', '-f', exact.stdout.trim()]);
    rmSync(root, { recursive: true, force: true });
  }
});
