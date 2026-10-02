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

// Codex reports inclusive input tokens. The common receipt uses uncached input.
export function parseCodexResult(stdout: string): RecordValue {
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
  if (!completed) errors.push(lastError ?? 'Codex returned no completed turn');
  if (!sessionId || !/^[0-9a-f-]{36}$/i.test(sessionId)) errors.push('Codex returned no valid session ID');
  return { type: 'result', session_id: sessionId, is_error: errors.length > 0,
    result: [result, ...errors].filter(Boolean).join('\n'), num_turns: turns, usage,
    ...(errors.length ? { terminal_reason: 'provider_error' } : {}) };
}

export function runCodexProcess({ command, args, input, env, timeoutMs, terminate }: {
  command: string; args: string[]; input: string; env: NodeJS.ProcessEnv; timeoutMs: number;
  terminate: (child: { kill(signal?: NodeJS.Signals): boolean }) => void;
}): Promise<{ status: number | null; signal: NodeJS.Signals | null; stdout: string;
  stderr: string; error: unknown }> {
  return new Promise(resolveResult => {
    const child = execFile(command, args, { env, encoding: 'utf8', windowsHide: true,
      maxBuffer: 256 * 1024 * 1024 }, (error, stdout, stderr) => {
      clearTimeout(timer);
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
