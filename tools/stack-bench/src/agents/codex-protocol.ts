import { execFile } from 'node:child_process';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

export function codexTranscriptDirectory(appDir: string): string {
  return join(homedir(), '.codex', 'stack-bench', resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase());
}

export function codexArguments({ model, effort, baseUrl, resumeSession }: {
  model: string; effort: string; baseUrl: string; resumeSession: string | null;
}): string[] {
  return ['exec', ...(resumeSession ? ['resume'] : []), '--json',
    '--skip-git-repo-check', '--dangerously-bypass-approvals-and-sandbox',
    '--ignore-user-config', '--ignore-rules', '--model', model,
    '-c', `model_reasoning_effort=${JSON.stringify(effort)}`,
    '-c', 'model_provider="model_proxy"',
    '-c', 'web_search="disabled"', '-c', 'features.multi_agent=false',
    '-c', 'model_providers.model_proxy.name="Model API"',
    '-c', `model_providers.model_proxy.base_url=${JSON.stringify(baseUrl + '/v1')}`,
    '-c', 'model_providers.model_proxy.env_key="MODEL_PROXY_TOKEN"',
    '-c', 'model_providers.model_proxy.wire_api="responses"',
    ...(resumeSession ? [resumeSession] : []), '-'];
}

type RecordValue = Record<string, unknown>;
function record(value: unknown): value is RecordValue {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export interface CodexUsage {
  input_tokens: number; output_tokens: number; cache_read_input_tokens: number; cache_creation_input_tokens: number;
}

// Codex reports inclusive input tokens. The common receipt uses uncached input.
// A resumed thread reports its whole history, so earlier invocations' usage is subtracted.
export function parseCodexResult(stdout: string, prior: CodexUsage | null = null): RecordValue {
  let sessionId: string | null = null;
  let result = '';
  let turns = 0;
  let completed = false;
  let lastError: string | null = null;
  const errors: string[] = [];
  const usage = { input_tokens: 0, output_tokens: 0, cache_read_input_tokens: 0,
    cache_creation_input_tokens: 0 };
  for (const line of stdout.split(/\r?\n/).filter(line => line.trim())) {
    let event: unknown;
    try { event = JSON.parse(line); }
    catch { errors.push('Malformed Codex event'); continue; }
    if (!record(event)) { errors.push('Invalid Codex event'); continue; }
    if (event.type === 'error' && typeof event.message === 'string') lastError = event.message;
    if (event.type === 'thread.started' && typeof event.thread_id === 'string') sessionId = event.thread_id;
    if (event.type === 'item.completed' && record(event.item)
      && event.item.type === 'agent_message' && typeof event.item.text === 'string') result = event.item.text;
    if (event.type === 'turn.started') completed = false;
    if (event.type === 'turn.failed') {
      completed = false;
      errors.push(record(event.error) && typeof event.error.message === 'string'
        ? event.error.message : 'Codex turn failed');
    }
    if (event.type === 'turn.completed') {
      const counts = record(event.usage) ? event.usage : {};
      const values = [counts.input_tokens, counts.cached_input_tokens, counts.output_tokens];
      if (!values.every(value => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0)
        || Number(counts.cached_input_tokens) > Number(counts.input_tokens)) {
        errors.push('Invalid Codex token usage'); continue;
      }
      usage.input_tokens += Number(counts.input_tokens) - Number(counts.cached_input_tokens);
      usage.cache_read_input_tokens += Number(counts.cached_input_tokens);
      usage.output_tokens += Number(counts.output_tokens);
      completed = true;
      turns++;
    }
  }
  if (prior) for (const key of Object.keys(usage) as Array<keyof CodexUsage>) {
    usage[key] = Math.max(0, usage[key] - prior[key]);
  }
  if (!completed) errors.push(lastError ?? 'Codex returned no completed turn');
  if (!sessionId || !/^[0-9a-f-]{36}$/i.test(sessionId)) errors.push('Codex returned no valid session ID');
  return { type: 'result', session_id: sessionId, is_error: errors.length > 0,
    result: [result, ...errors].filter(Boolean).join('\n'), num_turns: turns, usage,
    ...(errors.length ? { terminal_reason: 'provider_error' } : {}) };
}

// A CLI can give its final answer and still not exit, for example while a command it started
// keeps running. `finishedAnswer` reads that answer from the CLI's own record. Once the same
// answer has stood for the grace period the session is stopped as any other is, and the
// answer is appended to its output as one event for the provider's result parser.
export const SESSION_ENDED_AFTER_ANSWER = 'session_ended_after_answer';
export function runCodexProcess({ command, args, input, env, timeoutMs, terminate, finishedAnswer,
  answerGraceMs = 15_000, answerPollMs = 3_000 }: {
  command: string; args: string[]; input: string; env: NodeJS.ProcessEnv; timeoutMs: number;
  terminate: (child: { kill(signal?: NodeJS.Signals): boolean }) => void;
  finishedAnswer?: () => string | null; answerGraceMs?: number; answerPollMs?: number;
}): Promise<{ status: number | null; signal: NodeJS.Signals | null; stdout: string;
  stderr: string; error: unknown }> {
  return new Promise(resolveResult => {
    let answer: string | null = null;
    let answeredAt = 0;
    let endedAfterAnswer = false;
    const watch = finishedAnswer ? setInterval(() => {
      let current: string | null = null;
      try { current = finishedAnswer(); } catch { /* the record is not readable yet */ }
      if (current === null || current !== answer) { answer = current; answeredAt = Date.now(); return; }
      if (endedAfterAnswer || Date.now() - answeredAt < answerGraceMs) return;
      endedAfterAnswer = true;
      terminate(child);
    }, answerPollMs) : undefined;
    const child = execFile(command, args, { env, encoding: 'utf8', windowsHide: true,
      maxBuffer: 256 * 1024 * 1024 }, (error, stdout, stderr) => {
      clearTimeout(timer);
      clearInterval(watch);
      if (endedAfterAnswer && !timedOut) {
        resolveResult({ status: 0, signal: null, stderr, error: null,
          stdout: `${stdout.replace(/\n?$/, '\n')}${JSON.stringify({ event: SESSION_ENDED_AFTER_ANSWER, response: answer })}\n` });
        return;
      }
      if (error && !timedOut) terminate(child);
      resolveResult({ status: error ? typeof error.code === 'number' ? error.code : 1 : 0,
        signal: error?.signal ?? null, stdout, stderr, error: timedOut
          ? Object.assign(new Error('coding session timed out'), { code: 'ETIMEDOUT' }) : error });
    });
    let timedOut = false;
    const timer = setTimeout(() => { timedOut = true; terminate(child); }, timeoutMs);
    child.stdin!.end(input);
  });
}
