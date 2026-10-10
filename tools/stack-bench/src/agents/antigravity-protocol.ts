import { readFileSync, readdirSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { SESSION_ENDED_AFTER_ANSWER } from './codex-protocol.js';
import type { CodexUsage } from './codex-protocol.js';

export function antigravityTranscriptDirectory(appDir: string): string {
  return join(homedir(), '.gemini', 'stack-bench', resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase());
}

// Google's model for the conversation title, and the only system prompt that call carries.
export const ANTIGRAVITY_TITLE_MODEL = 'gemini-3.1-flash-lite-preview';
export const ANTIGRAVITY_TITLE_PROMPT = 'You are a conversation title generator.';

// With a Google account sign-in the CLI calls Google's Code Assist service, which names
// models its own way: one name for each effort of the plan's API model, and its title model.
// Each name here was seen in a live request for that model and effort.
export const ANTIGRAVITY_ACCOUNT_HOST = 'daily-cloudcode-pa.googleapis.com';
export const ANTIGRAVITY_ACCOUNT_MODELS: Readonly<Record<string, readonly string[]>> = {
  'gemini-3.8-flash': ['gemini-3.8-flash-low', 'gemini-3.8-flash-medium', 'gemini-3.8-flash-high'],
  'gemini-3.1-pro-preview': ['gemini-3.1-pro-low'],
  'gemini-3.1-pro-preview-customtools': ['gemini-pro-agent'],
};
export const ANTIGRAVITY_ACCOUNT_TITLE_MODEL = 'gemini-3.5-flash-lite';
// Before a session the CLI checks its account against Google's user info service itself.
export const ANTIGRAVITY_ACCOUNT_LOOKUP = { hostname: 'www.googleapis.com', path: '/oauth2/v2/userinfo' };
// The account reads the CLI makes around its model calls.
export const ANTIGRAVITY_ACCOUNT_CONTROL_PATHS = ['loadCodeAssist', 'retrieveUserQuotaSummary', 'fetchUserInfo',
  'fetchAdminControls', 'fetchAvailableModels', 'listExperiments'].map(method => `/v1internal:${method}`);

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
// the prompt it receives. The sign-in mode and telemetry are settings, rewritten each
// session. On the account route the broker holds the real sign-in: the CLI gets a
// stand-in carrying the broker's session token that never expires, so it never renews. Its
// HTTPS proxy is the broker, which answers its account check; the CLI trusts the broker's
// certificate authority on top of the system's.
export const ANTIGRAVITY_LAUNCHER = [
  'set -e',
  'state="$HOME/.gemini/antigravity-cli"',
  'mkdir -p "$state"',
  'if [ -n "$CLOUD_CODE_URL" ]; then',
  `  printf '%s\\n' '{"enableTelemetry":false}' > "$state/settings.json"`,
  `  printf '{"token":{"access_token":"%s","token_type":"Bearer","refresh_token":"stack-bench","expiry":"2099-01-01T00:00:00Z"},"auth_method":"consumer"}\\n' "$GEMINI_API_KEY" > "$state/antigravity-oauth-token"`,
  '  unset GEMINI_API_KEY',
  '  if [ -n "$BROKER_CA_CERT" ]; then',
  '    export SSL_CERT_FILE="$(mktemp)"',
  `    { cat /etc/ssl/certs/ca-certificates.crt; printf '%s\\n' "$BROKER_CA_CERT"; } > "$SSL_CERT_FILE"`,
  '    unset BROKER_CA_CERT',
  '  fi',
  'else',
  `  printf '%s\\n' '{"modelProvider":"gemini","enableTelemetry":false}' > "$state/settings.json"`,
  'fi',
  // The CLI replaces this shell, so a signal for the session reaches the CLI itself.
  'message="$(mktemp)"',
  'node -e \'let s="";process.stdin.setEncoding("utf8").on("data",d=>s+=d).on("end",()=>'
    + 'process.stdout.write(JSON.stringify({event:"user",message:{role:"user",content:s}})+"\\n"))\' > "$message"',
  'exec agy "$@" < "$message"',
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

// The CLI does not exit, or stream its answer, while a command it started is still running.
// Its transcript has the answer: the last entry of a conversation written since `sinceMs`
// is the model's finished response, with no further tool call, containing the marker.
export function antigravityFinishedAnswer(stateDirectory: string, marker: string, sinceMs: number): string | null {
  const brain = join(stateDirectory, 'brain');
  for (const conversation of readdirSync(brain)) {
    const transcript = join(brain, conversation, '.system_generated', 'logs', 'transcript.jsonl');
    let last: unknown;
    try {
      if (statSync(transcript).mtimeMs < sinceMs) continue;
      last = JSON.parse(readFileSync(transcript, 'utf8').trimEnd().split('\n').at(-1) ?? '');
    } catch { continue; }
    if (record(last) && last.source === 'MODEL' && last.type === 'PLANNER_RESPONSE' && last.status === 'DONE'
      && !(Array.isArray(last.tool_calls) && last.tool_calls.length) && typeof last.content === 'string'
      && last.content.includes(marker)) return last.content;
  }
  return null;
}
const count = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;

// The stream ends with one `result` event. Its usage covers the whole conversation, so a
// resumed session subtracts what earlier invocations reported. Input excludes cache reads, and
// output already includes thinking (checked against the broker on a live session).
export function parseAntigravityResult(stdout: string, prior: CodexUsage | null = null): RecordValue {
  const errors: string[] = [];
  let sessionId: string | null = null;
  let value: RecordValue | null = null;
  let finished = false;
  let endedAfter: string | null = null;
  for (const line of stdout.split(/\r?\n/).filter(line => line.trim())) {
    let event: unknown;
    try { event = JSON.parse(line); } catch { continue; }
    if (!record(event)) continue;
    if (typeof event.conversation_id === 'string') sessionId = event.conversation_id;
    if (event.event === 'result' && record(event.result)) value = event.result;
    if (event.event === SESSION_ENDED_AFTER_ANSWER && typeof event.response === 'string') endedAfter = event.response;
    if (event.event === 'step_update' && record(event.step_update)) {
      const step = event.step_update;
      finished = step.step_type === 'agent_response' && step.state === 'DONE';
    }
  }
  if (!value) errors.push('Antigravity returned no result');
  const result = value ?? {};
  if (typeof result.conversation_id === 'string') sessionId = result.conversation_id;
  if (sessionId !== null && !/^[0-9a-f-]{36}$/i.test(sessionId)) sessionId = null;
  if (!sessionId) errors.push('Antigravity returned no valid conversation ID');
  // The result names the conversation's last error even when the CLI retried past it, was
  // resumed after it, or was stopped after its finished answer. A failure that ends the
  // invocation leaves an error step last in the stream; one that ends with the model's
  // finished response, in the stream or recorded by the runner, did not fail.
  const answered = endedAfter !== null;
  if (value && result.status !== 'SUCCESS' && !finished && !answered) {
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
    result: [answered ? endedAfter : typeof result.response === 'string' ? result.response : '', ...errors]
      .filter(Boolean).join('\n'),
    num_turns: count(result.num_turns) ? result.num_turns : 0, usage,
    ...(errors.length ? { terminal_reason: 'provider_error' } : {}) };
}
