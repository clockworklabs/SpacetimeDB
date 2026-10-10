import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { ANTIGRAVITY_LAUNCHER, antigravityArguments, antigravityFinishedAnswer, parseAntigravityResult } from '../src/agents/antigravity-protocol.js';

const SESSION = '86eb1dda-1452-4d14-ab41-9afdc260bdd2';
// A stream's steps end with the model's finished response unless `steps` says otherwise.
const stream = (result: Record<string, unknown> | null, steps = ['agent_response']) => [
  { event: 'init', conversation_id: SESSION, init: { model: 'gemini-3.8-flash', cwd: '/app' } },
  ...steps.map((type, index) => ({ event: 'step_update',
    step_update: { conversation_id: SESSION, step_index: index + 1, state: 'DONE', step_type: type } })),
  ...(result ? [{ event: 'result', result: { conversation_id: SESSION, ...result } }] : []),
].map(event => JSON.stringify(event)).join('\n');
const usage = (input: number, output: number, thinking: number, cached: number) =>
  ({ input_tokens: input, output_tokens: output, thinking_tokens: thinking, cache_read_tokens: cached,
    total_tokens: input + output });

test('Antigravity reads the prompt as a stream message, pins model and effort, and resumes by conversation', () => {
  assert.deepEqual(antigravityArguments({ model: 'gemini-3.8-flash', effort: 'medium', resumeSession: null }),
    ['--print=', '--input-format', 'stream-json', '--output-format', 'stream-json', '--model', 'gemini-3.8-flash',
      '--effort', 'medium', '--dangerously-skip-permissions']);
  assert.deepEqual(antigravityArguments({ model: 'gemini-3.8-flash', effort: 'high', resumeSession: SESSION }).slice(-2),
    ['--conversation', SESSION]);
  // The CLI's name for Gemini 3.1 Pro covers both API models; the effort selects one.
  const pro = antigravityArguments({ model: 'gemini-3.1-pro-preview-customtools', effort: 'high', resumeSession: null });
  assert.equal(pro[pro.indexOf('--model') + 1], 'gemini-3.1-pro');
});

test('the launcher passes the prompt unchanged as one stream message and selects API-key mode', {
  skip: process.platform === 'win32' ? 'needs a POSIX shell' : false,
}, () => {
  const root = mkdtempSync(join(tmpdir(), 'antigravity-launcher-'));
  try {
    // A stand-in agy that records its stdin and arguments.
    writeFileSync(join(root, 'agy'), `#!/bin/sh\ncat > "${root}/stdin"\nprintf '%s\\n' "$@" > "${root}/args"\n`);
    chmodSync(join(root, 'agy'), 0o755);
    const prompt = 'Build the "store".\nUse $HOME and `backticks` and \'quotes\'.\n';
    execFileSync('sh', ['-c', ANTIGRAVITY_LAUNCHER, 'agy', '--model', 'gemini-3.8-flash'], { input: prompt,
      env: { PATH: `${root}:${process.env.PATH}`, HOME: root } });
    assert.deepEqual(JSON.parse(readFileSync(join(root, 'stdin'), 'utf8')),
      { event: 'user', message: { role: 'user', content: prompt } });
    assert.equal(readFileSync(join(root, 'args'), 'utf8'), '--model\ngemini-3.8-flash\n');
    assert.deepEqual(JSON.parse(readFileSync(join(root, '.gemini', 'antigravity-cli', 'settings.json'), 'utf8')),
      { modelProvider: 'gemini', enableTelemetry: false });
    // On the account route the CLI signs in with a stand-in carrying the broker's session token.
    writeFileSync(join(root, 'agy'), `#!/bin/sh\ncat > /dev/null\nprintf '%s' "\${GEMINI_API_KEY-unset}" > "${root}/key"\n`);
    execFileSync('sh', ['-c', ANTIGRAVITY_LAUNCHER, 'agy'], { input: prompt,
      env: { PATH: `${root}:${process.env.PATH}`, HOME: root, CLOUD_CODE_URL: 'http://127.0.0.1:1', GEMINI_API_KEY: 'session-1' } });
    const state = join(root, '.gemini', 'antigravity-cli');
    assert.deepEqual(JSON.parse(readFileSync(join(state, 'settings.json'), 'utf8')), { enableTelemetry: false });
    const standIn = JSON.parse(readFileSync(join(state, 'antigravity-oauth-token'), 'utf8'));
    assert.equal(standIn.token.access_token, 'session-1');
    assert.ok(Date.parse(standIn.token.expiry) > Date.parse('2090-01-01'));
    assert.equal(readFileSync(join(root, 'key'), 'utf8'), 'unset');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

// The final usage of a live L1 session: input without the cache reads, output with thinking.
test('an Antigravity result maps to the shared result as reported', () => {
  const result = parseAntigravityResult(stream({ status: 'SUCCESS', response: 'done\n', num_turns: 1,
    usage: usage(732018, 74097, 35246, 10839770) }));
  assert.deepEqual(result, { type: 'result', session_id: SESSION, is_error: false, result: 'done\n', num_turns: 1,
    usage: { input_tokens: 732018, output_tokens: 74097, cache_read_input_tokens: 10839770,
      cache_creation_input_tokens: 0 } });
});

test('a resumed Antigravity conversation reports only this invocation', () => {
  const prior = { input_tokens: 600, output_tokens: 80, cache_read_input_tokens: 400, cache_creation_input_tokens: 0 };
  const resumed = parseAntigravityResult(stream({ status: 'SUCCESS', response: 'ok', num_turns: 2,
    usage: usage(1600, 160, 40, 900) }), prior);
  assert.deepEqual(resumed.usage, { input_tokens: 1000, output_tokens: 80, cache_read_input_tokens: 500,
    cache_creation_input_tokens: 0 });
  assert.equal(parseAntigravityResult(stream({ status: 'SUCCESS', response: 'ok', num_turns: 2,
    usage: usage(100, 1, 0, 0) }), prior).is_error, true);
});

// Seen live: the resumed level-3 session finished, and its result repeated the 429 it was resumed after.
test('a resumed Antigravity conversation that finishes is not failed by the error it was resumed after', () => {
  const prior = { input_tokens: 600, output_tokens: 80, cache_read_input_tokens: 400, cache_creation_input_tokens: 0 };
  const repeated = { status: 'ERROR', error: 'Error 429, RESOURCE_EXHAUSTED', response: 'UPGRADE_COMPLETE\n', num_turns: 2,
    usage: usage(1600, 160, 40, 900) };
  const resumed = parseAntigravityResult(stream(repeated), prior);
  assert.equal(resumed.is_error, false);
  assert.equal(resumed.result, 'UPGRADE_COMPLETE\n');
  assert.equal(parseAntigravityResult(stream(repeated, ['agent_response', 'error_message']), prior).is_error, true);
  // Seen live at level 3: a first invocation retried past a 429, finished, and still named it.
  const retried = parseAntigravityResult(stream(repeated, ['tool', 'error_message', 'tool', 'agent_response']));
  assert.equal(retried.is_error, false);
  assert.equal(retried.result, 'UPGRADE_COMPLETE\n');
});

// Seen live: the agent started the app as a background command, so the CLI never exited or
// streamed its DEPLOY_COMPLETE; stopped, it reported "interrupted" with its usage.
test('an Antigravity session stopped after its finished answer returns that answer', () => {
  const root = mkdtempSync(join(tmpdir(), 'antigravity-answer-'));
  try {
    const logs = join(root, 'brain', SESSION, '.system_generated', 'logs');
    mkdirSync(logs, { recursive: true });
    const entry = (fields: Record<string, unknown>) => JSON.stringify({ source: 'MODEL', type: 'PLANNER_RESPONSE',
      status: 'DONE', ...fields });
    const write = (...entries: string[]) => writeFileSync(join(logs, 'transcript.jsonl'), `${entries.join('\n')}\n`);
    write(entry({ content: 'DEPLOY_COMPLETE soon', tool_calls: [{ name: 'run_command' }] }));
    assert.equal(antigravityFinishedAnswer(root, 'DEPLOY_COMPLETE', 0), null, 'a response with a tool call is not final');
    write(entry({ tool_calls: [{ name: 'run_command' }] }), entry({ content: 'DEPLOY_COMPLETE' }));
    assert.equal(antigravityFinishedAnswer(root, 'DEPLOY_COMPLETE', 0), 'DEPLOY_COMPLETE');
    assert.equal(antigravityFinishedAnswer(root, 'UPGRADE_COMPLETE', 0), null);
    assert.equal(antigravityFinishedAnswer(root, 'DEPLOY_COMPLETE', Date.now() + 60_000), null, 'an earlier session is not this one');
  } finally { rmSync(root, { recursive: true, force: true }); }
  const interrupted = { status: 'ERROR', error: 'interrupted', response: '', num_turns: 1, usage: usage(1000, 80, 40, 400) };
  // The stream stalls at the command that is still running.
  const ended = `${stream(interrupted, ['tool'])}\n${JSON.stringify({ event: 'session_ended_after_answer', response: 'DEPLOY_COMPLETE' })}`;
  const result = parseAntigravityResult(ended);
  assert.equal(result.is_error, false);
  assert.equal(result.result, 'DEPLOY_COMPLETE');
  assert.deepEqual(result.usage, { input_tokens: 1000, output_tokens: 80, cache_read_input_tokens: 400,
    cache_creation_input_tokens: 0 });
  assert.equal(parseAntigravityResult(stream(interrupted, ['tool'])).is_error, true, 'an interruption alone is an error');
  // Seen live at level 3: stopped after UPGRADE_COMPLETE, the CLI repeated a 503 it had retried past.
  const repeated = { ...interrupted, error: 'API error (attempt 1): Error 503, Status: UNAVAILABLE' };
  const afterRetry = parseAntigravityResult(
    `${stream(repeated, ['tool'])}\n${JSON.stringify({ event: 'session_ended_after_answer', response: 'UPGRADE_COMPLETE' })}`);
  assert.equal(afterRetry.is_error, false);
  assert.equal(afterRetry.result, 'UPGRADE_COMPLETE');
});

test('Antigravity errors, bad usage and missing results are provider errors', () => {
  const failed = parseAntigravityResult(stream({ status: 'ERROR', error: 'quota exceeded', response: '',
    num_turns: 0, usage: usage(0, 0, 0, 0) }, ['tool', 'error_message']));
  assert.equal(failed.is_error, true);
  assert.match(String(failed.result), /quota exceeded/);
  assert.equal(parseAntigravityResult(stream({ status: 'SUCCESS', response: 'x', num_turns: 1,
    usage: { input_tokens: -1, output_tokens: 1, cache_read_tokens: 0 } })).is_error, true);
  const missing = parseAntigravityResult(stream(null));
  assert.equal(missing.is_error, true);
  assert.match(String(missing.result), /no result/);
});
