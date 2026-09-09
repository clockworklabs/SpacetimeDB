import assert from 'node:assert/strict';
import test from 'node:test';
import { validateClaudeContinuationTranscript, validateCodexContinuationTranscript } from '../src/agents/native-session-validation.js';
import { classifyProviderFailure } from '../src/agents/provider-failure.js';

test('native continuation requires a complete conversation with settled tools', () => {
  const sessionId = 'session';
  const rows = [{ sessionId, uuid: 'root', parentUuid: null, type: 'user', message: { content: 'build the app' } },
    { sessionId, uuid: 'call', parentUuid: 'root', type: 'assistant', message: { content: [{ type: 'tool_use', id: 'tool1' }] } },
    { sessionId, uuid: 'result', parentUuid: 'call', type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'tool1' }] } }];
  const encode = (value: unknown[]) => value.map(row => JSON.stringify(row)).join('\n');
  assert.doesNotThrow(() => validateClaudeContinuationTranscript(encode(rows), sessionId));
  assert.doesNotThrow(() => validateClaudeContinuationTranscript(encode([rows[1], rows[0], rows[2]]), sessionId));
  assert.throws(() => validateClaudeContinuationTranscript(encode(rows.slice(0, 2)), sessionId), /unresolved/);
  assert.throws(() => validateClaudeContinuationTranscript(encode(rows.slice(1)), sessionId), /orphan/);
  assert.throws(() => validateClaudeContinuationTranscript(encode([{ ...rows[2], parentUuid: null }]), sessionId), /original task/);
  assert.throws(() => validateClaudeContinuationTranscript(encode(rows), 'wrong'), /identity changed/);
  assert.throws(() => validateClaudeContinuationTranscript(encode(rows) + '\n{', sessionId));
  assert.throws(() => validateClaudeContinuationTranscript(encode([{ ...rows[0], isSidechain: true }]), sessionId), /Sidechain/);
});

test('Claude compaction retains a connected history and settled tool exchanges', () => {
  const rows = [
    { uuid: 'root', parentUuid: null, type: 'user', message: { content: 'build the app' } },
    { uuid: 'call', parentUuid: 'root', type: 'assistant', message: { content: [{ type: 'tool_use', id: 'tool' }] } },
    { uuid: 'result', parentUuid: 'call', type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'tool' }] } },
    { uuid: 'boundary', parentUuid: null, type: 'system', subtype: 'compact_boundary', logicalParentUuid: 'result' },
    { uuid: 'summary', parentUuid: 'boundary', type: 'user', isCompactSummary: true, message: { content: 'Continue the app' } },
    { uuid: 'next', parentUuid: 'summary', type: 'assistant', message: { content: 'Continuing' } },
  ];
  const validate = (value: unknown[]) => validateClaudeContinuationTranscript(value.map(row => JSON.stringify(row)).join('\n'), 'session');
  assert.doesNotThrow(() => validate(rows));
  assert.doesNotThrow(() => validate([...rows].reverse()));
  assert.doesNotThrow(() => validate([...rows,
    { ...rows[3], uuid: 'boundary2', logicalParentUuid: 'next' },
    { ...rows[4], uuid: 'summary2', parentUuid: 'boundary2' }]));
  for (const logicalParentUuid of [undefined, '', 'missing', 'summary']) {
    assert.throws(() => validate(rows.map((row, i) => i === 3 ? { ...row, logicalParentUuid } : row)), /compaction|orphan|cycle/);
  }
  assert.throws(() => validate(rows.filter((_, i) => i !== 4)), /summary/);
  assert.throws(() => validate(rows.map((row, i) => i === 4 ? { ...row, message: { content: '' } } : row)), /summary/);
  assert.throws(() => validate(rows.map((row, i) => i === 3 ? { ...row, subtype: 'other' } : row)), /disconnected/);
  assert.throws(() => validate(rows.map((row, i) => i === 2 ? { ...row, message: { content: [] } } : row)), /unresolved/);
  assert.throws(() => validate(rows.map((row, i) => i === 2 ? { ...row, parentUuid: 'root' } : row)), /ancestor/);
  assert.throws(() => validate(rows.map((row, i) => i === 3 ? { ...row, sessionId: 'different' } : row)), /identity/);
});

test('structured provider rejection distinguishes quota from rate limits and auth', () => {
  const body = (code: string) => Buffer.from(JSON.stringify({ error: { code } }));
  assert.equal(classifyProviderFailure(429, body('insufficient_quota')).category, 'quota');
  assert.equal(classifyProviderFailure(429, Buffer.from('unknown')).category, 'rate-limit');
  assert.equal(classifyProviderFailure(401, body('authentication_error')).category, 'authentication');
  assert.equal(classifyProviderFailure(400, body('invalid_request_error')).category, 'request');
  assert.equal(classifyProviderFailure(402, Buffer.from('')).category, 'quota');
  assert.equal(classifyProviderFailure(429, body('secret value should not be copied')).code, null);
});


test('Codex continuation validates native task, model, and settled tool exchanges', () => {
  const sessionId = 'session';
  const rows = [
    { type: 'session_meta', payload: { id: sessionId, cwd: '/app', model_provider: 'model_proxy', base_instructions: { text: 'coding agent' } } },
    { type: 'turn_context', payload: { model: 'gpt-test', cwd: '/app' } },
    { type: 'response_item', payload: { type: 'message', role: 'user', content: [{ type: 'input_text', text: 'build the app' }] } },
    { type: 'event_msg', payload: { type: 'item_completed', item: { type: 'UserMessage', content: [{ type: 'text', text: 'build the app' }] } } },
    { type: 'response_item', payload: { type: 'function_call', call_id: 'tool1', name: 'exec_command' } },
    { type: 'response_item', payload: { type: 'function_call_output', call_id: 'tool1', output: 'done' } },
  ];
  const encode = (value: unknown[]) => value.map(row => JSON.stringify(row)).join('\n');
  assert.doesNotThrow(() => validateCodexContinuationTranscript(encode(rows), sessionId, 'gpt-test'));
  assert.throws(() => validateCodexContinuationTranscript(encode(rows.slice(0, -1)), sessionId, 'gpt-test'), /unresolved/);
  assert.throws(() => validateCodexContinuationTranscript(encode(rows), sessionId, 'different'), /model/);
  assert.throws(() => validateCodexContinuationTranscript(encode(rows.slice(1)), sessionId, 'gpt-test'), /identity/);
  assert.throws(() => validateCodexContinuationTranscript(encode(rows.filter((_, index) => index !== 2)), sessionId, 'gpt-test'), /task/);
  assert.throws(() => validateCodexContinuationTranscript(encode([...rows, rows[5]]), sessionId, 'gpt-test'), /ambiguous/);
  assert.throws(() => validateCodexContinuationTranscript(encode(rows.filter((_, index) => index !== 3)), sessionId, 'gpt-test'), /task/);
  assert.throws(() => validateCodexContinuationTranscript(encode([...rows, { type: 'response_item', payload: { type: 'local_shell_call' } }]), sessionId, 'gpt-test'), /tool type/);
  assert.throws(() => validateCodexContinuationTranscript(encode([...rows, { type: 'compacted', payload: { replacement_history: [] } }]), sessionId, 'gpt-test'), /compacted/);
  const compacted = { type: 'compacted', payload: { replacement_history: [rows[2]!.payload, rows[4]!.payload, rows[5]!.payload] } };
  assert.doesNotThrow(() => validateCodexContinuationTranscript(encode([...rows, compacted]), sessionId, 'gpt-test'));
  assert.doesNotThrow(() => validateCodexContinuationTranscript(encode([...rows, { type: 'compacted', payload: { message: 'App work summary' } }]), sessionId, 'gpt-test'));
  assert.throws(() => validateCodexContinuationTranscript(encode([...rows, { ...compacted, payload: { replacement_history: [rows[4]!.payload] } }]), sessionId, 'gpt-test'), /unresolved/);
});
