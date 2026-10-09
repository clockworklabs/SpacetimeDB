import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import type { CodexUsage } from './codex-protocol.js';

export function antigravityTranscriptDirectory(appDir: string): string {
  return join(homedir(), '.gemini', 'stack-bench', resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase());
}

// Google's model for the conversation title, and the only system prompt that call carries.
export const ANTIGRAVITY_TITLE_MODEL = 'gemini-3.1-flash-lite-preview';
export const ANTIGRAVITY_TITLE_PROMPT = 'You are a conversation title generator.';

// Files and a shell, as the other agents have. The CLI has no flag to remove a tool, so the
// broker drops every other function from the request: subagents, scheduling and messaging,
// web search and fetch, and questions for an absent user.
export const ANTIGRAVITY_TOOLS = ['view_file', 'run_command', 'write_to_file', 'replace_file_content', 'manage_task'];

// Claude's audit names for Antigravity's file and shell tools, and the parameter holding the path.
export const ANTIGRAVITY_AUDIT_TOOLS: Readonly<Record<string, { name: string; path?: string }>> = {
  run_command: { name: 'Bash' }, view_file: { name: 'Read', path: 'AbsolutePath' },
  write_to_file: { name: 'Edit', path: 'TargetFile' }, replace_file_content: { name: 'Edit', path: 'TargetFile' },
};

// The CLI reads a prompt from stdin only as a stream-json message, so the launcher wraps
// the prompt it receives. API-key mode and telemetry are settings, rewritten each session.
export const ANTIGRAVITY_LAUNCHER = [
  'set -e',
  'mkdir -p "$HOME/.gemini/antigravity-cli"',
  `printf '%s\\n' '{"modelProvider":"gemini","enableTelemetry":false}' > "$HOME/.gemini/antigravity-cli/settings.json"`,
  'node -e \'let s="";process.stdin.setEncoding("utf8").on("data",d=>s+=d).on("end",()=>'
    + 'process.stdout.write(JSON.stringify({event:"user",message:{role:"user",content:s}})+"\\n"))\' | exec agy "$@"',
].join('\n');

// Plans name the API model the broker allows. The CLI names some models differently and picks
// the API model from the effort: Gemini 3.1 Pro at high effort calls the custom-tools endpoint.
const ANTIGRAVITY_CLI_MODELS: Readonly<Record<string, string>> = {
  'gemini-3.1-pro-preview-customtools': 'gemini-3.1-pro', 'gemini-3.1-pro-preview': 'gemini-3.1-pro',
};

export function antigravityArguments({ model, effort, resumeSession }: {
  model: string; effort: string; resumeSession: string | null;
}): string[] {
  return ['--print=', '--input-format', 'stream-json', '--output-format', 'stream-json',
    '--model', ANTIGRAVITY_CLI_MODELS[model] ?? model,
    '--effort', effort, '--dangerously-skip-permissions', ...(resumeSession ? ['--conversation', resumeSession] : [])];
}

type RecordValue = Record<string, unknown>;
const record = (value: unknown): value is RecordValue =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const count = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;

// The stream ends with one `result` event. Its usage covers the whole conversation, so a
// resumed session subtracts what earlier invocations reported. Input excludes cache reads, and
// output already includes thinking (checked against the broker on a live session).
export function parseAntigravityResult(stdout: string, prior: CodexUsage | null = null): RecordValue {
  const errors: string[] = [];
  let sessionId: string | null = null;
  let value: RecordValue | null = null;
  for (const line of stdout.split(/\r?\n/).filter(line => line.trim())) {
    let event: unknown;
    try { event = JSON.parse(line); } catch { continue; }
    if (!record(event)) continue;
    if (typeof event.conversation_id === 'string') sessionId = event.conversation_id;
    if (event.event === 'result' && record(event.result)) value = event.result;
  }
  if (!value) errors.push('Antigravity returned no result');
  const result = value ?? {};
  if (typeof result.conversation_id === 'string') sessionId = result.conversation_id;
  if (sessionId !== null && !/^[0-9a-f-]{36}$/i.test(sessionId)) sessionId = null;
  if (!sessionId) errors.push('Antigravity returned no valid conversation ID');
  if (value && result.status !== 'SUCCESS') {
    errors.push(typeof result.error === 'string' && result.error ? result.error
      : `Antigravity ended with status ${JSON.stringify(result.status ?? null)}`);
  }
  const raw = record(result.usage) ? result.usage : {};
  const usage: CodexUsage = { input_tokens: 0, output_tokens: 0, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 };
  if (!value) { /* no usage to read */ }
  else if (![raw.input_tokens, raw.output_tokens, raw.cache_read_tokens].every(count)) {
    errors.push('Invalid Antigravity token usage');
  } else {
    const total = { input_tokens: raw.input_tokens as number, output_tokens: raw.output_tokens as number,
      cache_read_input_tokens: raw.cache_read_tokens as number, cache_creation_input_tokens: 0 };
    const keys = Object.keys(total) as (keyof CodexUsage)[];
    if (prior && keys.some(key => total[key] < prior[key])) errors.push('Antigravity usage went backwards on resume');
    else for (const key of keys) usage[key] = total[key] - (prior?.[key] ?? 0);
  }
  return { type: 'result', session_id: sessionId, is_error: errors.length > 0,
    result: [typeof result.response === 'string' ? result.response : '', ...errors].filter(Boolean).join('\n'),
    num_turns: count(result.num_turns) ? result.num_turns : 0, usage,
    ...(errors.length ? { terminal_reason: 'provider_error' } : {}) };
}
