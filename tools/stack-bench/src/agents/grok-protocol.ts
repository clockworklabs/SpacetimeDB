import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import type { CodexUsage } from './codex-protocol.js';

export function grokTranscriptDirectory(appDir: string): string {
  return join(homedir(), '.grok', 'stack-bench', resolve(appDir).replace(/[\\/:]/g, '-').toLowerCase());
}

// Files and a shell, as the other agents have. The allowlist governs only the file and
// shell tools; the rest must be removed by name. Removed: subagent workflows (as
// `--no-subagents`), media generation, MCP tools, and prompts for an absent user.
const GROK_TOOLS = ['run_terminal_command', 'monitor', 'read_file', 'search_replace', 'list_dir', 'grep', 'write',
  'todo_write'];
const GROK_REMOVED_TOOLS = ['workflow', 'image_gen', 'image_edit', 'image_to_video', 'reference_to_video', 'search_tool',
  'use_tool', 'send_feedback', 'ask_user_question', 'enter_plan_mode', 'exit_plan_mode'];

// Claude's audit names for Grok's file and shell tools, and the input field holding the path.
export const GROK_AUDIT_TOOLS: Readonly<Record<string, { name: string; path?: string }>> = {
  run_terminal_command: { name: 'Bash' }, monitor: { name: 'Bash' },
  read_file: { name: 'Read', path: 'target_file' }, list_dir: { name: 'Glob', path: 'target_directory' },
  grep: { name: 'Grep', path: 'path' }, search_replace: { name: 'Edit', path: 'file_path' },
  write: { name: 'Edit', path: 'file_path' },
};

// The prompt arrives on stdin. The stream is in the Anthropic Messages shape, so it
// is also the session's audited transcript.
export function grokArguments({ model, effort, resumeSession }: {
  model: string; effort: string; resumeSession: string | null;
}): string[] {
  return ['--prompt-file', '/dev/stdin', '--output-format', 'streaming-messages-json', '-m', model, '--effort', effort,
    '--always-approve', '--disable-web-search', '--no-subagents', '--tools', GROK_TOOLS.join(','),
    '--disallowed-tools', GROK_REMOVED_TOOLS.join(','),
    ...(resumeSession ? ['--resume', resumeSession] : [])];
}

type RecordValue = Record<string, unknown>;
const record = (value: unknown): value is RecordValue =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const count = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;

// The stream ends with one `result` event, as Claude Code's does. Its input tokens
// already exclude cache reads.
export function parseGrokResult(stdout: string): RecordValue {
  const errors: string[] = [];
  let sessionId: string | null = null;
  let value: RecordValue | null = null;
  for (const line of stdout.split(/\r?\n/).filter(line => line.trim())) {
    let event: unknown;
    try { event = JSON.parse(line); } catch { continue; }
    if (!record(event)) continue;
    if (typeof event.session_id === 'string') sessionId = event.session_id;
    if (event.type === 'result') value = event;
  }
  if (!value) errors.push('Grok returned no result');
  const result = value ?? {};
  if (sessionId !== null && !/^[0-9a-f-]{36}$/i.test(sessionId)) sessionId = null;
  if (!sessionId) errors.push('Grok returned no valid session ID');
  if (result.is_error === true) {
    errors.push(...(Array.isArray(result.errors) ? result.errors.map(String) : ['Grok reported an error']));
  } else if (value && result.stop_reason !== 'end_turn') {
    errors.push(`Grok stopped with ${JSON.stringify(result.stop_reason ?? null)}`);
  }
  const raw = record(result.usage) ? result.usage : {};
  const fields = [raw.input_tokens, raw.output_tokens, raw.cache_read_input_tokens, raw.cache_creation_input_tokens];
  const usage: CodexUsage = { input_tokens: 0, output_tokens: 0, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 };
  if (!value) { /* no usage to read */ }
  else if (!fields.every(count)) errors.push('Invalid Grok token usage');
  else Object.assign(usage, { input_tokens: raw.input_tokens, output_tokens: raw.output_tokens,
    cache_read_input_tokens: raw.cache_read_input_tokens, cache_creation_input_tokens: raw.cache_creation_input_tokens });
  return { type: 'result', session_id: sessionId, is_error: errors.length > 0,
    result: [typeof result.result === 'string' ? result.result : '', ...errors].filter(Boolean).join('\n'),
    num_turns: count(result.num_turns) ? result.num_turns : 0, usage,
    ...(errors.length ? { terminal_reason: 'provider_error' } : {}) };
}
