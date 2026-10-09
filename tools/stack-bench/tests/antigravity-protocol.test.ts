import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { ANTIGRAVITY_LAUNCHER, antigravityArguments, parseAntigravityResult } from '../src/agents/antigravity-protocol.js';

const SESSION = '86eb1dda-1452-4d14-ab41-9afdc260bdd2';
const stream = (result: Record<string, unknown> | null) => [
  { event: 'init', conversation_id: SESSION, init: { model: 'gemini-3.8-flash', cwd: '/app' } },
  { event: 'step_update', step_update: { conversation_id: SESSION, step_index: 1, state: 'DONE', step_type: 'agent_response' } },
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

test('Antigravity errors, bad usage and missing results are provider errors', () => {
  const failed = parseAntigravityResult(stream({ status: 'ERROR', error: 'quota exceeded', response: '',
    num_turns: 0, usage: usage(0, 0, 0, 0) }));
  assert.equal(failed.is_error, true);
  assert.match(String(failed.result), /quota exceeded/);
  assert.equal(parseAntigravityResult(stream({ status: 'SUCCESS', response: 'x', num_turns: 1,
    usage: { input_tokens: -1, output_tokens: 1, cache_read_tokens: 0 } })).is_error, true);
  const missing = parseAntigravityResult(stream(null));
  assert.equal(missing.is_error, true);
  assert.match(String(missing.result), /no result/);
});
