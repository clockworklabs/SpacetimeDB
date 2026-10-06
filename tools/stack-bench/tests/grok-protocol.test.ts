import assert from 'node:assert/strict';
import test from 'node:test';
import { grokArguments, parseGrokResult } from '../src/agents/grok-protocol.js';

const SESSION = '01a112c6-39bd-7d52-a3a9-3158e1b76555';
const stream = (result: Record<string, unknown>) => [
  { type: 'system', subtype: 'init', session_id: SESSION, cwd: '/app', model: 'grok-4.6' },
  { type: 'assistant', message: { content: [{ type: 'text', text: 'done' }] } },
  { type: 'result', session_id: SESSION, ...result },
].map(event => JSON.stringify(event)).join('\n');

test('Grok runs headless from stdin with files and a shell only, and resumes by session', () => {
  const args = grokArguments({ model: 'grok-4.6', effort: 'medium', resumeSession: null });
  assert.deepEqual(args.slice(0, 11), ['--prompt-file', '/dev/stdin', '--output-format', 'streaming-messages-json',
    '-m', 'grok-4.6', '--effort', 'medium', '--always-approve', '--disable-web-search', '--no-subagents']);
  const tools = args[args.indexOf('--tools') + 1]!.split(',');
  assert(tools.includes('run_terminal_command') && tools.includes('read_file'));
  // --no-subagents leaves the subagent tool callable; only the allowlist removes it.
  assert(!tools.includes('spawn_subagent'));
  assert.deepEqual(grokArguments({ model: 'grok-4.6', effort: 'high', resumeSession: SESSION }).slice(-2),
    ['--resume', SESSION]);
});

test('a completed Grok stream maps to the shared result, with cache reads kept separate', () => {
  const result = parseGrokResult(stream({ subtype: 'success', is_error: false, num_turns: 2, result: 'done',
    stop_reason: 'end_turn', usage: { input_tokens: 400, output_tokens: 50, cache_read_input_tokens: 600,
      cache_creation_input_tokens: 0, server_tool_use: { web_search_requests: 0 } } }));
  assert.deepEqual(result, { type: 'result', session_id: SESSION, is_error: false, result: 'done', num_turns: 2,
    usage: { input_tokens: 400, output_tokens: 50, cache_read_input_tokens: 600, cache_creation_input_tokens: 0 } });
});

test('Grok errors, early stops and missing results are provider errors', () => {
  const error = parseGrokResult(stream({ subtype: 'error_during_execution', is_error: true, num_turns: 0, stop_reason: null,
    usage: { input_tokens: 0, output_tokens: 0, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 },
    errors: ['Internal error: upstream refused'] }));
  assert.equal(error.is_error, true);
  assert.match(String(error.result), /upstream refused/);
  assert.equal(error.terminal_reason, 'provider_error');
  assert.equal(error.session_id, SESSION, 'a failed session can still be resumed');
  const stopped = parseGrokResult(stream({ is_error: false, stop_reason: 'max_turns', num_turns: 3,
    usage: { input_tokens: 1, output_tokens: 1, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 } }));
  assert.match(String(stopped.result), /stopped with "max_turns"/);
  assert.match(String(parseGrokResult('').result), /no result[\s\S]*no valid session ID/);
  assert.match(String(parseGrokResult(stream({ is_error: false, stop_reason: 'end_turn',
    usage: { input_tokens: -1, output_tokens: 0, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 } })).result),
  /Invalid Grok token usage/);
});
