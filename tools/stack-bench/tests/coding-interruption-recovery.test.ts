import assert from 'node:assert/strict';
import { once } from 'node:events';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { aggregateCodingSessionResults, codingSessionInterruption,
  parseCodingSessionResult, providerSessionFailure,
  runCodingSessionWithRetries, type CodingSessionInvocation }
  from '../src/agents/coding-session-retry.js';
import { agentSessionFailure } from '../src/agents/agent-result-contract.js';
import { createBackendLease, readBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { reconcileCredentialBrokerReceipt } from '../container/credential-broker-accounting.js';
import { credentialBrokerDiagnostics, startCredentialBroker, stopCredentialBroker }
  from '../container/credential-broker-process.js';
import { clearMissingBuildContainerLease,
  recoverStoppedBuildContainer } from '../container/recover-build-container.js';

function brokerReceipt(costUsd: number, maxBudgetUsd: number = 10) {
  return { schemaVersion: 3, source: 'credential-broker', exact: true, estimatedRequests: 0,
    estimatedByReason: { 'no-usage': 0, 'response-aborted': 0, 'upstream-error': 0 }, model: 'test-model',
    maxBudgetUsd, costUsd, cliCostUsd: costUsd, calculatedCostUsd: costUsd,
    usage: { input: 1, output: 1, cacheWrite5m: 0, cacheWrite1h: 0, cacheRead: 0 },
    pricingRates: { input: 3, output: 15, cacheWrite5m: 3.75, cacheWrite1h: 6,
      cacheRead: 0.3 },
    complete: true, reconciled: true, error: null };
}

function required<T>(value: T | null | undefined, description: string): T {
  if (value === null || value === undefined) throw new Error(`${description} is required`);
  return value;
}

function record(value: unknown, description: string): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new Error(`${description} must be an object`);
  }
  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function brokerDetails(value: unknown, description: string): {
  child: { exitCode: number | null; signal: string | null };
  termination: { exited: boolean };
} {
  const details = record(value, description);
  const child = record(details.child, `${description}.child`);
  const termination = record(details.termination, `${description}.termination`);
  if ((typeof child.exitCode !== 'number' && child.exitCode !== null)
    || (typeof child.signal !== 'string' && child.signal !== null)
    || typeof termination.exited !== 'boolean') {
    throw new Error(`${description} has invalid child termination details`);
  }
  return { child: { exitCode: child.exitCode, signal: child.signal },
    termination: { exited: termination.exited } };
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

test('coding result parsing rejects non-object and malformed output', () => {
  assert.equal(parseCodingSessionResult('true'), null);
  assert.equal(parseCodingSessionResult('{"is_error":"no"}'), null);
  assert.deepEqual(parseCodingSessionResult('noise\n{"is_error":false,"session_id":"done"}'),
    { is_error: false, session_id: 'done' });
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
  const calls: CodingSessionInvocation[] = [];
  const coding = runCodingSessionWithRetries({ prompt: 'build', model: 'test-model', retryLimit: 1,
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
  assert.equal(required(calls[1], 'recovery invocation').resumeSession, sessionId);
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
  const coding = runCodingSessionWithRetries({ prompt: 'build', model: 'test-model', retryLimit: 3,
    maxBudgetUsd: 10, invoke() { calls += 1; return JSON.stringify(result); } });
  assert.equal(calls, 1);
  assert.match(required(coding.spawnError, 'broker failure'), /broker failed without a complete reconciled cost receipt/);
  assert.equal(required(coding.interruptions[0], 'broker interruption').kind, 'credential-broker-unavailable');
  assert.equal(required(agentSessionFailure({ ok: false,
    providerMetadata: { failureCode: 'credential-broker-unavailable' } }),
  'agent session failure').kind, 'harness_failure');
});

test('a real broker child exit reaches the run result and stops recovery as a harness failure', async t => {
  const model = 'test-model';
  const maxBudgetUsd = 10;
  const pricingRates = { input: 3, output: 15, cacheWrite5m: 3.75,
    cacheWrite1h: 6, cacheRead: 0.3 };
  const broker = await startCredentialBroker({ mode: 'api-key',
    credential: 'provider-secret-value-1234567890' }, { networkMode: 'host', deadlineMs: 10_000, model,
    maxBudgetUsd, pricingRates });
  t.after(() => rmSync(broker.root, { recursive: true, force: true }));
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
  assert.equal(record(reconciled.result.stack_bench_cost_receipt,
    'reconciled cost receipt').reconciled, true);
  const reconciledBroker = brokerDetails(reconciled.result.stack_bench_credential_broker,
    'reconciled credential broker');
  assert.ok(reconciledBroker.child.exitCode !== null || reconciledBroker.child.signal !== null);
  let calls = 0;
  const coding = runCodingSessionWithRetries({ prompt: 'build', model: 'test-model', retryLimit: 3,
    maxBudgetUsd, invoke() { calls += 1; return JSON.stringify(reconciled.result); } });
  assert.equal(calls, 1);
  assert.match(required(coding.spawnError, 'broker failure'), /local credential broker failed/);
  assert.equal(required(coding.interruptions[0], 'broker interruption').kind, 'credential-broker-unavailable');
  assert.equal(required(coding.interruptions[0], 'broker interruption').costUsd, 0);
  const failureCode = required(providerSessionFailure(coding.result), 'provider failure').code;
  const codingBroker = brokerDetails(coding.result.stack_bench_credential_broker,
    'recovered credential broker');
  const agentResult = { ok: false, providerMetadata: { failureCode,
    credentialBroker: codingBroker } };
  assert.equal(required(agentSessionFailure(agentResult), 'agent session failure').kind, 'harness_failure');
  assert.equal(codingBroker.termination.exited, true);
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
  const calls: CodingSessionInvocation[] = [];
  const waits: number[] = [];
  const logs: string[] = [];
  const coding = runCodingSessionWithRetries({ prompt: 'build the app', model: 'test-model', retryLimit: 0,
    sleep: ms => waits.push(ms), log: message => logs.push(message),
    invoke(request) {
      calls.push(request);
      if (calls.length <= 2) {
        return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
          api_error_status: 429, session_id: 'paid-session', total_cost_usd: 0.001,
          stack_bench_cost_receipt: brokerReceipt(0.001) });
      }
      return JSON.stringify({ is_error: false, session_id: 'paid-session',
        total_cost_usd: 2, num_turns: 3, usage: {} });
    } });
  assert.equal(coding.spawnError, null);
  assert.equal(calls.length, 3);
  assert.deepEqual(waits, [60_000, 120_000]);
  assert.equal(required(calls[1], 'second invocation').resumeSession, 'paid-session');
  assert.equal(required(calls[2], 'third invocation').resumeSession, 'paid-session');
  assert.deepEqual(coding.throttle,
    { waits: 2, waitedMs: 180_000, maxWaitMs: 15 * 60_000, jitterMs: 0 });
  assert.equal(coding.interruptions.length, 2);
  assert.ok(coding.interruptions.every(item => item.kind === 'provider-throttled'));
  assert.match(required(logs[0], 'throttle log'), /provider throttled \(status 429\)/);
});

test('provider failure without native identity cannot restart the task', () => {
  let calls = 0;
  const coding = runCodingSessionWithRetries({ prompt: 'full original prompt', model: 'test-model', retryLimit: 0,
    sleep: () => { throw new Error('must not wait'); }, log: () => {},
    invoke() { calls++; return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
      api_error_status: 429, total_cost_usd: 0, stack_bench_cost_receipt: brokerReceipt(0) }); } });
  assert.equal(calls, 1);
  assert.match(required(coding.spawnError, 'missing session'), /native session identity/);
});

test('an unbroken throttle stops at the wait budget with a throttle-specific failure', () => {
  const waits: number[] = [];
  let calls = 0;
  const coding = runCodingSessionWithRetries({ prompt: 'build the app', model: 'test-model', retryLimit: 3,
    throttleMaxWaitMs: 5 * 60_000, sleep: ms => waits.push(ms), log: () => {},
    invoke() {
      calls += 1;
      return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
        api_error_status: 429, session_id: 'paid-session', total_cost_usd: 0,
        stack_bench_cost_receipt: brokerReceipt(0),
        result: 'API Error: Request rejected (429)' });
    } });
  // 60s + 120s fit inside five minutes; the 300s third wait would exceed it.
  assert.deepEqual(waits, [60_000, 120_000]);
  assert.equal(calls, 3);
  assert.match(required(coding.spawnError, 'throttle failure'), /provider stayed throttled \(status 429\) after 3 minutes/);
  assert.deepEqual(coding.throttle,
    { waits: 2, waitedMs: 180_000, maxWaitMs: 5 * 60_000, jitterMs: 0 });
});

test('a stable throttle offset spreads concurrent retries without changing retry accounting', () => {
  const waits: number[] = [];
  let calls = 0;
  const coding = runCodingSessionWithRetries({ prompt: 'build', model: 'test-model', retryLimit: 0,
    throttleJitterMs: 17_000, sleep: ms => waits.push(ms), log: () => {},
    invoke() {
      calls += 1;
      return calls === 1
        ? JSON.stringify({ is_error: true, terminal_reason: 'api_error', api_error_status: 429,
          session_id: 'paid-session', total_cost_usd: 0, stack_bench_cost_receipt: brokerReceipt(0) })
        : JSON.stringify({ is_error: false, session_id: 'done', usage: {} });
    } });
  assert.deepEqual(waits, [77_000]);
  assert.deepEqual(coding.throttle,
    { waits: 1, waitedMs: 77_000, maxWaitMs: 15 * 60_000, jitterMs: 17_000 });
});

test('operator continuation keeps the action and candidate, persists each charge once, and resumes natively', () => {
  const saved: number[] = [];
  const calls: CodingSessionInvocation[] = [];
  const candidate = { source: 'unfinished edit', database: 'current row', repairCount: 1 };
  const coding = runCodingSessionWithRetries({ prompt: 'original task', model: 'test-model',
    retryLimit: 0, maxBudgetUsd: 10, throttleMaxWaitMs: 0,
    onInvocation: record => { saved.push(record.invocation); },
    sleep: () => { throw new Error('quota does not use automatic backoff'); },
    waitForProvider: request => {
      assert.deepEqual(saved, Array.from({ length: calls.length }, (_, index) => index + 1));
      assert.equal(request.sessionId, 'native-session');
      assert.deepEqual(candidate, { source: 'unfinished edit', database: 'current row', repairCount: 1 });
      return { requestId: `operator-${request.invocation}`, generation: `wait-${request.invocation}` };
    },
    invoke(request) {
      calls.push(request);
      const rejected = calls.length < 3;
      return JSON.stringify({ is_error: rejected, session_id: 'native-session', total_cost_usd: 0.5,
        stack_bench_cost_receipt: brokerReceipt(0.5, request.maxBudgetUsd!),
        ...(rejected ? { stack_bench_provider_failure: {
          category: calls.length === 1 ? 'quota' : 'rate-limit', status: 429,
          code: calls.length === 1 ? 'insufficient_quota' : 'rate_limit_exceeded' } } : {}),
        usage: { input_tokens: 1, output_tokens: 1 } });
    } });
  assert.equal(coding.spawnError, null);
  assert.deepEqual(saved, [1, 2, 3]);
  assert.deepEqual(calls.map(call => call.maxBudgetUsd), [10, 9.5, 9]);
  assert.deepEqual(calls.map(call => call.input), ['original task', 'Continue.', 'Continue.']);
  assert.ok(calls.slice(1).every(call => call.resumeSession === 'native-session' && !call.recoverStoppedContainer));
  assert.equal(coding.result.total_cost_usd, 1.5);
  assert.equal(coding.result.stack_bench_cost_receipts.length, 3);
  assert.deepEqual(coding.providerWaits?.map(wait => wait.disposition), ['continued', 'continued']);
});

test('persistence failure and rejected native validation retain paid evidence without another invocation', () => {
  for (const persistFails of [true, false]) {
    let calls = 0;
    const coding = runCodingSessionWithRetries({ prompt: 'task', model: 'test-model', retryLimit: 0,
      onInvocation: () => { if (persistFails) throw new Error('receipt disk unavailable'); },
      waitForProvider: () => { throw new Error('native session changed'); },
      invoke: () => { calls++; return JSON.stringify({ is_error: true, session_id: 'native',
        total_cost_usd: 0.5, stack_bench_cost_receipt: brokerReceipt(0.5),
        stack_bench_provider_failure: { category: 'authentication', status: 401, code: 'expired' } }); } });
    assert.equal(calls, 1);
    assert.equal(coding.result.total_cost_usd, 0.5);
    assert.match(coding.spawnError!, persistFails ? /receipt disk unavailable/ : /native session changed/);
  }
});

test('structured transport, permanent request and broker budget failures never enter operator waiting', () => {
  for (const category of ['transport', 'request', 'broker-budget']) {
    let calls = 0;
    const coding = runCodingSessionWithRetries({ prompt: 'task', model: 'test-model', retryLimit: 3,
      waitForProvider: () => { throw new Error('must not wait'); },
      invoke: () => { calls++; return JSON.stringify({ is_error: true, session_id: 'native',
        total_cost_usd: 0.5, stack_bench_cost_receipt: brokerReceipt(0.5),
        stack_bench_provider_failure: { category, status: 429, code: null } }); } });
    assert.equal(calls, 1);
    assert.match(coding.spawnError!, /not eligible for continuation/);
    assert.deepEqual(coding.providerWaits, []);
  }
});

test('a CLI-only 429 never authorizes manual waiting without a broker-confirmed rejection', () => {
  let waits = 0;
  const coding = runCodingSessionWithRetries({ prompt: 'task', model: 'test-model', retryLimit: 0,
    throttleMaxWaitMs: 0, waitForProvider: () => { waits++; return true; },
    invoke: () => JSON.stringify({ is_error: true, session_id: 'native',
      terminal_reason: 'api_error', api_error_status: 429,
      total_cost_usd: 0.5, stack_bench_cost_receipt: brokerReceipt(0.5) }) });
  assert.equal(waits, 0);
  assert.deepEqual(coding.providerWaits, []);
  assert.match(coding.spawnError!, /provider stayed throttled/);
  assert.equal(coding.result.total_cost_usd, 0.5);
});

test('only a non-OOM forced exit is eligible for container recovery', () => {
  const failure = (diagnostic: Record<string, unknown>) => Object.assign(new Error('killed'), { status: 137,
    stderr: `STACK_BENCH_CODING_PROCESS_DIAGNOSTIC ${JSON.stringify(diagnostic)}\n` });
  const killed = codingSessionInterruption(failure({ status: 137,
    container: { ExitCode: 143, OOMKilled: false }, cgroupMemory: 'oom 0\noom_kill 0\n' }), null);
  assert.equal(required(killed, 'killed-process interruption').kind, 'coding-process-killed');
  assert.equal(required(killed, 'killed-process interruption').recoverStoppedContainer, true);
  assert.equal(codingSessionInterruption(failure({ status: 137,
    container: { ExitCode: 137, OOMKilled: true }, cgroupMemory: 'oom 1\noom_kill 1\n' }), null), null);
});

test('a bounded coding-session timeout continues from the existing app', () => {
  const timeout = Object.assign(new Error('spawnSync docker ETIMEDOUT'), { code: 'ETIMEDOUT' });
  assert.deepEqual(codingSessionInterruption(timeout, null), {
    kind: 'coding-session-timeout', resumeSession: null, recoverStoppedContainer: false,
    terminalReason: null, providerStatus: null,
  });
  const calls: CodingSessionInvocation[] = [];
  const coding = runCodingSessionWithRetries({ prompt: 'build the app', model: 'test-model', retryLimit: 1,
    invoke(request) {
      calls.push(request);
      if (calls.length === 1) throw timeout;
      return JSON.stringify({ is_error: false, session_id: 'replacement', total_cost_usd: 1,
        num_turns: 1, usage: {} });
    } });
  assert.equal(coding.spawnError, null);
  assert.equal(calls.length, 2);
  assert.match(required(calls[1], 'replacement invocation').input,
    /Continue this task from the existing files/);
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
  const calls: CodingSessionInvocation[] = [];
  const coding = runCodingSessionWithRetries({ prompt: 'full original prompt', model: 'test-model', retryLimit: 2,
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
  assert.equal(required(calls[0], 'first invocation').input, 'full original prompt');
  assert.equal(required(calls[0], 'first invocation').maxBudgetUsd, 10);
  assert.equal(required(calls[1], 'second invocation').resumeSession, sessionId);
  assert.equal(required(calls[1], 'second invocation').maxBudgetUsd, 6.5);
  assert.equal(required(calls[1], 'second invocation').input, 'Continue.');
  assert.equal(required(coding.interruptions[0], 'provider interruption').kind, 'provider-api-error');
});

test('a capped session does not treat a numeric cost as a broker receipt', () => {
  let calls = 0;
  const coding = runCodingSessionWithRetries({ prompt: 'build', model: 'test-model', retryLimit: 2,
    maxBudgetUsd: 10,
    invoke() {
      calls += 1;
      return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
        api_error_status: 503, session_id: 'paid-session', total_cost_usd: 2 });
    } });
  assert.equal(calls, 1);
  assert.match(required(coding.spawnError, 'cost receipt failure'), /without a complete reconciled broker cost receipt/);
  assert.equal(required(coding.interruptions[0], 'cost receipt interruption').costUsd, null);
});

test('a capped session does not retry with an unreconciled broker receipt', () => {
  let calls = 0;
  const receipt = { ...brokerReceipt(2), reconciled: false, error: 'cost mismatch' };
  const coding = runCodingSessionWithRetries({ prompt: 'build', model: 'test-model', retryLimit: 2,
    maxBudgetUsd: 10,
    invoke() {
      calls += 1;
      return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
        api_error_status: 503, session_id: 'paid-session', total_cost_usd: 2,
        stack_bench_cost_receipt: receipt });
    } });
  assert.equal(calls, 1);
  assert.match(required(coding.spawnError, 'cost receipt failure'), /without a complete reconciled broker cost receipt/);
  assert.equal(required(coding.interruptions[0], 'cost receipt interruption').costUsd, null);
});

test('a capped session does not retry with a receipt for another model', () => {
  let calls = 0;
  const coding = runCodingSessionWithRetries({ prompt: 'build', model: 'test-model', retryLimit: 2,
    maxBudgetUsd: 10,
    invoke() {
      calls += 1;
      return JSON.stringify({ is_error: true, terminal_reason: 'api_error',
        api_error_status: 503, session_id: 'paid-session', total_cost_usd: 2,
        stack_bench_cost_receipt: { ...brokerReceipt(2), model: 'other-model' } });
    } });
  assert.equal(calls, 1);
  assert.match(required(coding.spawnError, 'cost receipt failure'),
    /without a complete reconciled broker cost receipt/);
});

test('a non-OOM kill recovers the container without pretending to resume a missing session', () => {
  const calls: CodingSessionInvocation[] = [];
  const coding = runCodingSessionWithRetries({ prompt: 'full prompt', model: 'test-model', retryLimit: 1,
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
  assert.equal(required(calls[1], 'recovery invocation').recoverStoppedContainer, true);
  assert.equal(required(calls[1], 'recovery invocation').resumeSession, null);
  assert.match(required(calls[1], 'recovery invocation').input,
    /Continue this task from the existing files/);
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
      image: 'image-id', owned: true, running: false, networkMode: 'bridge',
      resourceLimits: { cpuCount: 2, memoryBytes: 2147483648,
        memorySwapBytes: 2147483648, pids: 512 } };
    writeBackendLease(path, lease);
    const calls: Array<[string, readonly string[]]> = [];
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
      image: 'image-id', owned: true, running: false, networkMode: 'bridge',
      resourceLimits: { cpuCount: 2, memoryBytes: 2147483648,
        memorySwapBytes: 2147483648, pids: 512 } };
    writeBackendLease(path, lease);
    let executed = false;
    assert.throws(() => recoverStoppedBuildContainer({
      existing: { id: 'b'.repeat(64), running: false }, containerName: 'leased-build',
      leaseContext: { path, lease }, backend: 'mongodb',
      execute() { executed = true; return { status: 0 }; },
    }), /does not match/);
    assert.equal(executed, false);
    assert.equal(required(readBackendLease(path).resources.buildContainer,
      'recovered build container').id, 'a'.repeat(64));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('missing-container recovery clears only the authenticated lease target', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-build-recovery-'));
  try {
    const path = join(root, 'lease.json');
    const lease = createBackendLease({ runId: 'recover-build-test', backend: 'mongodb',
      track: 'ecommerce', runIndex: 0, database: 'app_ecom_run0',
      container: { name: 'stack-bench-mongodb', id: 'database-id' } });
    lease.state = 'active';
    lease.resources.buildContainer = { name: 'leased-build', id: 'a'.repeat(64),
      image: 'image-id', owned: true, running: false, networkMode: 'bridge',
      resourceLimits: { cpuCount: 2, memoryBytes: 2147483648,
        memorySwapBytes: 2147483648, pids: 512 } };
    writeBackendLease(path, lease);
    const recovered = clearMissingBuildContainerLease({
      containerName: 'leased-build', leaseContext: { path, lease }, backend: 'mongodb',
    });
    assert.equal(recovered.lease.resources.buildContainer, null);
    assert.equal(readBackendLease(path).resources.buildContainer, null);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
