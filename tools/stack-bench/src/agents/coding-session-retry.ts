import { AGENT_COST_RECEIPT_TOLERANCE_USD, validateAgentCostReceipt }
  from './agent-result-contract.js';

type UnknownRecord = Record<string, unknown>;

interface CodingSessionResult {
  api_error_status?: number | null;
  terminal_reason?: string;
  result?: string;
  is_error?: boolean;
  session_id?: string;
  total_cost_usd?: number;
  num_turns?: number;
  usage?: UnknownRecord;
  stack_bench_cost_receipt?: UnknownRecord;
  stack_bench_credential_broker?: CodingSessionCredentialBroker;
  terminal_recovery?: unknown;
}

interface CodingSessionCredentialBroker {
  endpointKind?: string;
}

interface CodingProcessError {
  code?: string;
  status?: number;
  stdout?: unknown;
  stderr?: unknown;
}

interface CodingProcessDiagnostic {
  status?: unknown;
  signal?: unknown;
  cgroupMemory?: unknown;
  container?: { ExitCode?: unknown; OOMKilled?: unknown };
}

interface ProviderSessionFailure {
  code: 'provider-throttle' | 'provider-api-error' | 'credential-broker-unavailable'
    | 'provider-connection-error' | 'provider-session-error';
  status: number | null;
}

interface CodingSessionInterruption {
  kind: 'provider-throttled' | 'provider-api-error' | 'credential-broker-unavailable'
    | 'provider-connection-error' | 'coding-session-timeout' | 'coding-process-killed';
  resumeSession: string | null;
  recoverStoppedContainer: boolean;
  terminalReason: string | null;
  providerStatus: number | null;
  diagnostic?: {
    status: unknown;
    signal: unknown;
    containerExitCode: unknown;
    oomKilled: unknown;
  } | null;
}

interface RecordedCodingSessionInterruption extends CodingSessionInterruption {
  invocation: number;
  sessionId: string | null;
  costUsd: number | null;
  waitedMs?: number;
}

export interface CodingSessionInvocation {
  input: string;
  maxBudgetUsd: number | null;
  resumeSession: string | null;
  recoverStoppedContainer: boolean;
  invocation: number;
}

interface CodingSessionRetryOptions {
  invoke: (invocation: CodingSessionInvocation) => unknown;
  prompt: string;
  model: string;
  retryLimit: number;
  maxBudgetUsd?: number | null;
  throttleMaxWaitMs?: number;
  throttleJitterMs?: number;
  sleep?: (milliseconds: number) => void;
  log?: ((message: string) => void) | null;
}

interface AggregatedCodingSessionResult extends CodingSessionResult {
  total_cost_usd: number;
  num_turns: number;
  usage: {
    input_tokens: number;
    output_tokens: number;
    cache_creation_input_tokens: number;
    cache_read_input_tokens: number;
  };
  stack_bench_cost_receipts: Array<{ invocation: number; receipt: unknown }>;
}

export interface CodingSessionRetryResult {
  raw: string;
  spawnError: string | null;
  sessionResults: CodingSessionResult[];
  interruptions: RecordedCodingSessionInterruption[];
  throttle: {
    waits: number;
    waitedMs: number;
    maxWaitMs: number;
    jitterMs: number;
  };
  result: AggregatedCodingSessionResult;
}

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function text(value: unknown): string {
  return Buffer.isBuffer(value) ? value.toString('utf8') : String(value ?? '');
}

// Throttling consumes wait time, not interruption retries.
const THROTTLE_STATUSES = new Set([429, 529]);
const TRANSIENT_API_STATUSES = new Set([500, 502, 503, 504]);
const THROTTLE_DELAYS_MS = [60_000, 120_000, 300_000, 600_000];
export const DEFAULT_THROTTLE_MAX_WAIT_MS = 15 * 60_000;

export function providerSessionFailure(
  result: CodingSessionResult | null | undefined,
): ProviderSessionFailure | null {
  const status = result?.api_error_status ?? null;
  if (result?.terminal_reason === 'api_error' && status !== null
    && THROTTLE_STATUSES.has(status)) {
    return { code: 'provider-throttle', status };
  }
  if (result?.terminal_reason === 'api_error' && status !== null
    && TRANSIENT_API_STATUSES.has(status)) {
    return { code: 'provider-api-error', status };
  }
  const detail = typeof result?.result === 'string' ? result.result : '';
  if (result?.stack_bench_credential_broker?.endpointKind === 'local-credential-broker'
    && /API Error:.*(?:Unable to connect to API|ConnectionRefused)/i.test(detail)) {
    return { code: 'credential-broker-unavailable', status };
  }
  if (/API Error:.*(?:Unable to connect to API|ConnectionRefused)/i.test(detail)) {
    return { code: 'provider-connection-error', status };
  }
  if (result?.terminal_reason === 'api_error') {
    return { code: 'provider-session-error', status };
  }
  return null;
}

function synchronousSleep(ms: number): void {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

function codingSessionResult(value: unknown): CodingSessionResult | null {
  if (!object(value)) return null;
  if (value.api_error_status !== undefined && value.api_error_status !== null
    && typeof value.api_error_status !== 'number') return null;
  for (const key of ['terminal_reason', 'result', 'session_id'] as const) {
    if (value[key] !== undefined && typeof value[key] !== 'string') return null;
  }
  if (value.is_error !== undefined && typeof value.is_error !== 'boolean') return null;
  for (const key of ['total_cost_usd', 'num_turns'] as const) {
    if (value[key] !== undefined && (!Number.isFinite(value[key]) || (value[key] as number) < 0)) {
      return null;
    }
  }
  if (value.usage !== undefined && !object(value.usage)) return null;
  if (value.stack_bench_cost_receipt !== undefined
    && !object(value.stack_bench_cost_receipt)) return null;
  if (value.stack_bench_credential_broker !== undefined
    && !object(value.stack_bench_credential_broker)) return null;
  if (object(value.stack_bench_credential_broker)
    && value.stack_bench_credential_broker.endpointKind !== undefined
    && typeof value.stack_bench_credential_broker.endpointKind !== 'string') return null;
  return value;
}

function parseResultLine(value: string): CodingSessionResult | null {
  try { return codingSessionResult(JSON.parse(value)); } catch { return null; }
}

export function parseCodingSessionResult(raw: unknown): CodingSessionResult | null {
  const value = text(raw).trim();
  if (!value) return null;
  const whole = parseResultLine(value);
  if (whole) return whole;
  for (const line of value.split(/\r?\n/).reverse()) {
    const result = parseResultLine(line);
    if (result) return result;
  }
  return null;
}

function codingProcessDiagnostic(
  error: CodingProcessError | null | undefined,
): CodingProcessDiagnostic | null {
  const line = text(error?.stderr).split(/\r?\n/)
    .find(item => item.startsWith('STACK_BENCH_CODING_PROCESS_DIAGNOSTIC '));
  if (!line) return null;
  try {
    const parsed: unknown = JSON.parse(
      line.slice('STACK_BENCH_CODING_PROCESS_DIAGNOSTIC '.length));
    return object(parsed) ? parsed : null;
  }
  catch { return null; }
}

function completeBrokerReceiptCost(result: CodingSessionResult | null, model: string): number | null {
  const resultCostUsd = result?.total_cost_usd;
  if (typeof resultCostUsd !== 'number' || !Number.isFinite(resultCostUsd) || resultCostUsd < 0) {
    return null;
  }
  try {
    const receipt = validateAgentCostReceipt(result?.stack_bench_cost_receipt, model,
      'coding session cost receipt');
    return receipt.complete && receipt.reconciled && receipt.error === null
      && Math.abs(receipt.costUsd - resultCostUsd) <= AGENT_COST_RECEIPT_TOLERANCE_USD
      ? receipt.costUsd : null;
  } catch { return null; }
}

export function codingSessionInterruption(
  error: CodingProcessError | null,
  result: CodingSessionResult | null,
): CodingSessionInterruption | null {
  const apiStatus = result?.api_error_status ?? null;
  if (result?.terminal_reason === 'api_error' && apiStatus !== null
    && THROTTLE_STATUSES.has(apiStatus)) {
    // A throttled first request has no session yet; a null resumeSession makes
    // the retry restart from the existing files instead of resuming.
    return { kind: 'provider-throttled',
      resumeSession: typeof result.session_id === 'string' && result.session_id
        ? result.session_id : null,
      recoverStoppedContainer: false, terminalReason: 'api_error',
      providerStatus: apiStatus };
  }
  if (result?.terminal_reason === 'api_error' && apiStatus !== null
    && TRANSIENT_API_STATUSES.has(apiStatus)
    && typeof result.session_id === 'string' && result.session_id) {
    return { kind: 'provider-api-error', resumeSession: result.session_id,
      recoverStoppedContainer: false, terminalReason: 'api_error',
      providerStatus: result.api_error_status ?? null };
  }
  const providerFailure = providerSessionFailure(result);
  if (providerFailure?.code === 'credential-broker-unavailable'
    && result && typeof result.session_id === 'string' && result.session_id) {
    return { kind: 'credential-broker-unavailable', resumeSession: result.session_id,
      recoverStoppedContainer: false, terminalReason: result.terminal_reason ?? null,
      providerStatus: null };
  }
  if (providerFailure?.code === 'provider-connection-error'
    && result && typeof result.session_id === 'string' && result.session_id) {
    return { kind: 'provider-connection-error', resumeSession: result.session_id,
      recoverStoppedContainer: false, terminalReason: result.terminal_reason ?? null,
      providerStatus: providerFailure.status };
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

export function aggregateCodingSessionResults(
  results: Array<CodingSessionResult | null | undefined>,
): AggregatedCodingSessionResult {
  const sessions = results.filter((result): result is CodingSessionResult => Boolean(result));
  const last = sessions.at(-1) ?? {};
  const usage = { input_tokens: 0, output_tokens: 0,
    cache_creation_input_tokens: 0, cache_read_input_tokens: 0 };
  const usageKeys = ['input_tokens', 'output_tokens', 'cache_creation_input_tokens',
    'cache_read_input_tokens'] as const;
  for (const result of sessions) {
    for (const key of usageKeys) usage[key] += Number(result.usage?.[key] ?? 0);
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

export function codingSessionFailure(error: CodingProcessError): string {
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

export function runCodingSessionWithRetries({ invoke, prompt, model, retryLimit, maxBudgetUsd = null,
  throttleMaxWaitMs = DEFAULT_THROTTLE_MAX_WAIT_MS, throttleJitterMs = 0,
  sleep = synchronousSleep, log = null }: CodingSessionRetryOptions): CodingSessionRetryResult {
  if (typeof invoke !== 'function') throw new Error('coding session invoke function is required');
  if (typeof model !== 'string' || !model) throw new Error('coding session model is required');
  if (!Number.isInteger(retryLimit) || retryLimit < 0 || retryLimit > 3) {
    throw new Error('coding interruption retry limit must be an integer from 0 to 3');
  }
  if (!Number.isInteger(throttleMaxWaitMs) || throttleMaxWaitMs < 0) {
    throw new Error('provider throttle wait budget must be a non-negative integer of milliseconds');
  }
  if (!Number.isInteger(throttleJitterMs) || throttleJitterMs < 0 || throttleJitterMs > 60_000) {
    throw new Error('provider throttle jitter must be an integer from 0 to 60000 milliseconds');
  }
  const say = log ?? ((message: string) => console.error(message));
  let raw = '';
  let spawnError: string | null = null;
  const sessionResults: CodingSessionResult[] = [];
  const interruptions: RecordedCodingSessionInterruption[] = [];
  let resumeSession: string | null = null;
  let recoverStoppedContainer = false;
  // Track interruption attempts and throttle time separately.
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
    let error: CodingProcessError | null = null;
    try {
      raw = text(invoke({ input, maxBudgetUsd: invocationBudget, resumeSession,
        recoverStoppedContainer, invocation }));
    } catch (err) {
      error = object(err) ? err : {};
      raw = text(error.stdout);
    }
    const result = parseCodingSessionResult(raw);
    if (result) sessionResults.push(result);
    if (!error && result?.is_error === false) break;
    const interruption = codingSessionInterruption(error, result);
    const receiptCostUsd = completeBrokerReceiptCost(result, model);
    if (interruption?.kind === 'credential-broker-unavailable') {
      interruptions.push({ ...interruption, invocation: invocation + 1,
        sessionId: result?.session_id ?? null, costUsd: receiptCostUsd });
      spawnError = receiptCostUsd === null
        ? 'local credential broker failed without a complete reconciled cost receipt; automatic recovery is disabled'
        : 'local credential broker failed; automatic recovery is disabled';
      break;
    }
    if (maxBudgetUsd !== null && interruption && receiptCostUsd === null) {
      interruptions.push({ ...interruption, invocation: invocation + 1,
        sessionId: result?.session_id ?? null, costUsd: null });
      spawnError = 'coding session was interrupted without a complete reconciled broker cost receipt; automatic recovery is disabled';
      break;
    }
    if (interruption?.kind === 'provider-throttled') {
      const delay = THROTTLE_DELAYS_MS[
        Math.min(throttleWaits, THROTTLE_DELAYS_MS.length - 1)]! + throttleJitterMs;
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
