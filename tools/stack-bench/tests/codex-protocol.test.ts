import { test } from 'node:test';
import assert from 'node:assert/strict';
import { codexArguments, parseCodexResult, runCodexProcess } from '../src/agents/codex-protocol.js';

test('Codex terminal receipt excludes cached input and requires a complete valid turn', () => {
  const events = [
    { type: 'thread.started', thread_id: '12345678-1234-4234-8234-123456789abc' },
    { type: 'item.completed', item: { type: 'agent_message', text: 'APP_READY' } },
    { type: 'turn.completed', usage: { input_tokens: 30, cached_input_tokens: 20, output_tokens: 5 } },
  ];
  const jsonl = events.map(event => JSON.stringify(event)).join('\n');
  const result = parseCodexResult(jsonl);
  assert.equal(result.is_error, false);
  assert.equal(result.result, 'APP_READY');
  assert.equal(result.total_cost_usd, undefined);
  assert.deepEqual(result.usage, { input_tokens: 10, cache_read_input_tokens: 20,
    output_tokens: 5, cache_creation_input_tokens: 0 });
  assert.equal(parseCodexResult(jsonl + '\n{"type":"turn.failed"}').is_error, true);
  assert.equal(parseCodexResult(jsonl + '\n{"type":"turn.started"}').is_error, true);
  assert.equal(parseCodexResult(jsonl.replace('"input_tokens":30', '"input_tokens":2')).is_error, true);
  assert.equal(parseCodexResult('{').is_error, true);
  assert.match(String(parseCodexResult('{"type":"error","message":"HTTP 429 quota exhausted"}').result),
    /429 quota exhausted/);
});

test('Codex uses only the temporary broker token and preserves explicit resume identity', () => {
  const args = codexArguments({ model: 'gpt-test', effort: 'high', baseUrl: 'http://127.0.0.1:123',
    resumeSession: '12345678-1234-4234-8234-123456789abc' });
  assert.deepEqual(args.slice(0, 2), ['exec', 'resume']);
  assert.ok(args.includes('model_providers.model_proxy.env_key="MODEL_PROXY_TOKEN"'));
  assert.ok(args.includes('--ignore-user-config'));
  assert.ok(args.includes('web_search="disabled"'));
  assert.ok(args.includes('features.multi_agent=false'));
  assert.deepEqual(args.slice(-2), ['12345678-1234-4234-8234-123456789abc', '-']);
});

test('Codex process capture runs without provider access', async () => {
  const result = await runCodexProcess({ command: process.execPath,
    args: ['-e', 'process.stdin.pipe(process.stdout)'], input: 'events', env: process.env,
    timeoutMs: 5_000, terminate: child => { child.kill(); } });
  assert.equal(result.status, 0);
  assert.equal(result.stdout, 'events');
  let terminated = false;
  const timedOut = await runCodexProcess({ command: process.execPath,
    args: ['-e', 'setInterval(() => {}, 1000)'], input: '', env: process.env,
    timeoutMs: 30, terminate: child => { terminated = true; child.kill(); } });
  assert.equal(terminated, true);
  assert.notEqual(timedOut.status, 0);
  assert.match(String(timedOut.error), /timed out/);
});
