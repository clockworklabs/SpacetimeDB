import { AGENT_COST_RECEIPT_TOLERANCE_USD } from './agent-adapter-contract.mjs';

function text(value) {
  return Buffer.isBuffer(value) ? value.toString('utf8') : String(value ?? '');
}

// Provider statuses that mean "the account cannot accept work right now", not
// "this session is broken": 429 is the account rate/usage limit and 529 is
// provider overload. Both are transient by definition, so the recovery loop
// waits them out instead of spending its bounded interruption retries — an
// immediate resume against a throttled account fails in milliseconds and a
// nine-way campaign burns every retry it has while the window is still closed.
const THROTTLE_STATUSES = new Set([429, 529]);
const TRANSIENT_API_STATUSES = new Set([500, 502, 503, 504]);
// Escalating waits, then a steady 15-minute probe. A subscription usage window
// can stay closed for hours; the budget below bounds the total, not the count.
const THROTTLE_DELAYS_MS = [60_000, 120_000, 300_000, 600_000, 900_000];
export const DEFAULT_THROTTLE_MAX_WAIT_MS = 300 * 60_000;
const BROKER_USAGE_FIELDS = ['input', 'output', 'cacheWrite5m', 'cacheWrite1h', 'cacheRead'];

// Synchronous by design: the surrounding loop drives execFileSync invocations,
// and each chunk is at most 15 minutes so a pending SIGTERM is honoured at the
// next chunk boundary rather than never.
function synchronousSleep(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

export function parseCodingSessionResult(raw) {
  const value = text(raw).trim();
  if (!value) return null;
  try { return JSON.parse(value); } catch { /* try the last JSON line */ }
  for (const line of value.split(/\r?\n/).reverse()) {
    try { return JSON.parse(line); } catch { /* keep looking */ }
  }
  return null;
}

function codingProcessDiagnostic(error) {
  const line = text(error?.stderr).split(/\r?\n/)
    .find(item => item.startsWith('STACK_BENCH_CODING_PROCESS_DIAGNOSTIC '));
  if (!line) return null;
  try { return JSON.parse(line.slice('STACK_BENCH_CODING_PROCESS_DIAGNOSTIC '.length)); }
  catch { return null; }
}

function completeBrokerReceiptCost(result) {
  const receipt = result?.stack_bench_cost_receipt;
  const resultCostUsd = result?.total_cost_usd;
  if (!receipt || typeof receipt !== 'object' || Array.isArray(receipt)
    || receipt.schemaVersion !== 2 || receipt.source !== 'credential-broker'
    || typeof receipt.model !== 'string' || !receipt.model
    || !Number.isFinite(receipt.maxBudgetUsd) || receipt.maxBudgetUsd <= 0
    || !Number.isFinite(receipt.costUsd) || receipt.costUsd < 0
    || !Number.isFinite(receipt.cliCostUsd) || receipt.cliCostUsd < 0
    || !Number.isFinite(receipt.calculatedCostUsd) || receipt.calculatedCostUsd < 0
    || receipt.complete !== true || receipt.reconciled !== true || receipt.error !== null
    || !Number.isFinite(resultCostUsd) || resultCostUsd < 0) return null;
  for (const field of ['usage', 'pricingRates']) {
    const values = receipt[field];
    if (!values || typeof values !== 'object' || Array.isArray(values)
      || BROKER_USAGE_FIELDS.some(key => !Number.isFinite(values[key]) || values[key] < 0)) {
      return null;
    }
  }
  return Math.abs(receipt.costUsd - resultCostUsd) <= AGENT_COST_RECEIPT_TOLERANCE_USD
    ? receipt.costUsd : null;
}

export function codingSessionInterruption(error, result) {
  if (result?.terminal_reason === 'api_error'
    && THROTTLE_STATUSES.has(result.api_error_status ?? null)) {
    // A throttled first request has no session yet; a null resumeSession makes
    // the retry restart from the existing files instead of resuming.
    return { kind: 'provider-throttled',
      resumeSession: typeof result.session_id === 'string' && result.session_id
        ? result.session_id : null,
      recoverStoppedContainer: false, terminalReason: 'api_error',
      providerStatus: result.api_error_status };
  }
  if (result?.terminal_reason === 'api_error'
    && TRANSIENT_API_STATUSES.has(result.api_error_status ?? null)
    && typeof result.session_id === 'string' && result.session_id) {
    return { kind: 'provider-api-error', resumeSession: result.session_id,
      recoverStoppedContainer: false, terminalReason: 'api_error',
      providerStatus: result.api_error_status ?? null };
  }
  if (error?.code === 'ETIMEDOUT') {
    return { kind: 'coding-session-timeout', resumeSession: null,
      recoverStoppedContainer: false, terminalReason: null, providerStatus: null };
  }
  if (error?.status !== 137) return null;
  const diagnostic = codingProcessDiagnostic(error);
  const memory = String(diagnostic?.cgroupMemory ?? '');
  const oomEvent = /(?:^|\n)oom(?:_kill)?\s+[1-9]\d*(?:\n|$)/m.test(memory);
  if (diagnostic?.container?.OOMKilled === true || oomEvent) return null;
  return { kind: 'coding-process-killed', resumeSession: null,
    recoverStoppedContainer: true, terminalReason: null, providerStatus: null,
    diagnostic: diagnostic ? {
      status: diagnostic.status ?? null,
      signal: diagnostic.signal ?? null,
      containerExitCode: diagnostic.container?.ExitCode ?? null,
      oomKilled: diagnostic.container?.OOMKilled ?? null,
    } : null };
}

export function aggregateCodingSessionResults(results) {
  const sessions = results.filter(Boolean);
  const last = sessions.at(-1) ?? {};
  const usage = { input_tokens: 0, output_tokens: 0,
    cache_creation_input_tokens: 0, cache_read_input_tokens: 0 };
  for (const result of sessions) {
    for (const key of Object.keys(usage)) usage[key] += Number(result.usage?.[key] ?? 0);
  }
  return {
    ...last,
    total_cost_usd: sessions.reduce((sum, item) => sum + Number(item.total_cost_usd ?? 0), 0),
    num_turns: sessions.reduce((sum, item) => sum + Number(item.num_turns ?? 0), 0),
    usage,
    stack_bench_cost_receipts: sessions.flatMap((item, index) =>
      item.stack_bench_cost_receipt
        ? [{ invocation: index + 1, receipt: structuredClone(item.stack_bench_cost_receipt) }]
        : []),
  };
}

export function codingSessionFailure(error) {
  const status = Number.isInteger(error?.status) ? `exit ${error.status}` : null;
  const code = typeof error?.code === 'string' && error.code ? error.code : null;
  const reason = code ?? status ?? 'nonzero exit';
  const stderr = Buffer.isBuffer(error?.stderr)
    ? error.stderr.toString('utf8') : String(error?.stderr ?? '');
  const stdout = Buffer.isBuffer(error?.stdout)
    ? error.stdout.toString('utf8') : String(error?.stdout ?? '');
  const stdoutTail = stdout.trim().slice(-2000);
  const stderrTail = stderr.trim().slice(-4000);
  const killed = error?.status === 137
    ? ' — process was forcibly killed; use the retained coding-process diagnostic to distinguish memory pressure from another kill'
    : '';
  return `coding session failed (${reason})${killed}`
    + `${stdoutTail ? `\ninner stdout tail:\n${stdoutTail}` : ''}`
    + `${stderrTail ? `\ninner stderr tail:\n${stderrTail}` : ''}`;
}

export function runCodingSessionWithRecovery({ invoke, prompt, retryLimit, maxBudgetUsd = null,
  throttleMaxWaitMs = DEFAULT_THROTTLE_MAX_WAIT_MS, throttleJitterMs = 0,
  sleep = synchronousSleep, log = null }) {
  if (typeof invoke !== 'function') throw new Error('coding session invoke function is required');
  if (!Number.isInteger(retryLimit) || retryLimit < 0 || retryLimit > 3) {
    throw new Error('coding interruption retry limit must be an integer from 0 to 3');
  }
  if (!Number.isInteger(throttleMaxWaitMs) || throttleMaxWaitMs < 0) {
    throw new Error('provider throttle wait budget must be a non-negative integer of milliseconds');
  }
  if (!Number.isInteger(throttleJitterMs) || throttleJitterMs < 0 || throttleJitterMs > 60_000) {
    throw new Error('provider throttle jitter must be an integer from 0 to 60000 milliseconds');
  }
  const say = log ?? (message => console.error(message));
  let raw = '';
  let spawnError = null;
  const sessionResults = [];
  const interruptions = [];
  let resumeSession = null;
  let recoverStoppedContainer = false;
  // Interruption retries are a bounded count; throttle waits are a bounded
  // TIME. A throttled provider can reject dozens of near-free probe requests
  // before the window reopens, and counting those against the retry limit
  // converts a transient account condition into a terminal harness failure.
  let interruptionRetries = 0;
  let throttleWaits = 0;
  let throttleWaitedMs = 0;
  for (let invocation = 0; ; invocation++) {
    const priorCost = sessionResults.reduce((sum, item) =>
      sum + Number(item.total_cost_usd ?? 0), 0);
    const invocationBudget = maxBudgetUsd == null ? null
      : Number((maxBudgetUsd - priorCost).toFixed(6));
    if (invocationBudget !== null && invocationBudget <= 0) {
      spawnError = `coding session exhausted its $${maxBudgetUsd} cost cap before recovery`;
      break;
    }
    const input = resumeSession
      ? 'The provider interrupted the previous response. Continue the same task from the existing files. '
        + 'Verify the application is running and finish with the completion marker requested earlier.'
      : invocation === 0 ? prompt
        : 'A prior coding process was terminated. Continue this task from the existing files; do not start over.\n\n'
          + prompt;
    let error = null;
    try {
      raw = text(invoke({ input, maxBudgetUsd: invocationBudget, resumeSession,
        recoverStoppedContainer, invocation }));
    } catch (err) {
      error = err;
      raw = text(err.stdout);
    }
    const result = parseCodingSessionResult(raw);
    if (result) sessionResults.push(result);
    if (!error && result?.is_error === false) break;
    const interruption = codingSessionInterruption(error, result);
    const receiptCostUsd = completeBrokerReceiptCost(result);
    if (maxBudgetUsd !== null && interruption && receiptCostUsd === null) {
      interruptions.push({ ...interruption, invocation: invocation + 1,
        sessionId: result?.session_id ?? null, costUsd: null });
      spawnError = 'coding session was interrupted without a complete reconciled broker cost receipt; automatic recovery is disabled';
      break;
    }
    if (interruption?.kind === 'provider-throttled') {
      const delay = THROTTLE_DELAYS_MS[Math.min(throttleWaits, THROTTLE_DELAYS_MS.length - 1)]
        + throttleJitterMs;
      if (throttleWaitedMs + delay > throttleMaxWaitMs) {
        spawnError = `provider stayed throttled (status ${interruption.providerStatus}) after `
          + `${Math.round(throttleWaitedMs / 60_000)} minutes of waiting across `
          + `${throttleWaits} retry attempt(s)`;
        break;
      }
      say(`provider throttled (status ${interruption.providerStatus}); waiting `
        + `${Math.round(delay / 1000)}s before `
        + `${interruption.resumeSession ? `resuming session ${interruption.resumeSession}`
          : 'restarting the coding session'} `
        + `(${Math.round((throttleWaitedMs + delay) / 60_000)} of `
        + `${Math.round(throttleMaxWaitMs / 60_000)} wait minutes used)`);
      sleep(delay);
      throttleWaits += 1;
      throttleWaitedMs += delay;
      interruptions.push({ ...interruption, invocation: invocation + 1,
        sessionId: result?.session_id ?? null, waitedMs: delay,
        costUsd: Number((result?.total_cost_usd ?? 0).toFixed(6)) });
      resumeSession = interruption.resumeSession;
      recoverStoppedContainer = interruption.recoverStoppedContainer;
      continue;
    }
    if (!interruption || interruptionRetries === retryLimit) {
      spawnError = codingSessionFailure(error ?? {
        status: 1, stdout: raw, stderr: result?.result ?? 'coding session reported failure',
      });
      break;
    }
    interruptionRetries += 1;
    interruptions.push({ ...interruption, invocation: invocation + 1,
      sessionId: result?.session_id ?? null,
      costUsd: Number((result?.total_cost_usd ?? 0).toFixed(6)) });
    resumeSession = interruption.resumeSession;
    recoverStoppedContainer = interruption.recoverStoppedContainer;
  }
  return { raw, spawnError, sessionResults, interruptions,
    throttle: { waits: throttleWaits, waitedMs: throttleWaitedMs,
      maxWaitMs: throttleMaxWaitMs, jitterMs: throttleJitterMs },
    result: aggregateCodingSessionResults(sessionResults) };
}
