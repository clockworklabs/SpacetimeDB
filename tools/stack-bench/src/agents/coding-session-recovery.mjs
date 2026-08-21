function text(value) {
  return Buffer.isBuffer(value) ? value.toString('utf8') : String(value ?? '');
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

export function codingSessionInterruption(error, result) {
  if (result?.terminal_reason === 'api_error' && typeof result.session_id === 'string'
    && result.session_id) {
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

export function runCodingSessionWithRecovery({ invoke, prompt, retryLimit, maxBudgetUsd = null }) {
  if (typeof invoke !== 'function') throw new Error('coding session invoke function is required');
  if (!Number.isInteger(retryLimit) || retryLimit < 0 || retryLimit > 3) {
    throw new Error('coding interruption retry limit must be an integer from 0 to 3');
  }
  let raw = '';
  let spawnError = null;
  const sessionResults = [];
  const interruptions = [];
  let resumeSession = null;
  let recoverStoppedContainer = false;
  for (let invocation = 0; invocation <= retryLimit; invocation++) {
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
    if (!interruption || invocation === retryLimit) {
      spawnError = codingSessionFailure(error ?? {
        status: 1, stdout: raw, stderr: result?.result ?? 'coding session reported failure',
      });
      break;
    }
    interruptions.push({ ...interruption, invocation: invocation + 1,
      sessionId: result?.session_id ?? null,
      costUsd: Number((result?.total_cost_usd ?? 0).toFixed(6)) });
    resumeSession = interruption.resumeSession;
    recoverStoppedContainer = interruption.recoverStoppedContainer;
  }
  return { raw, spawnError, sessionResults, interruptions,
    result: aggregateCodingSessionResults(sessionResults) };
}
