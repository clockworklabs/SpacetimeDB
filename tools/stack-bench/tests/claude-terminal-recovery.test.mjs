import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { recoverClaudeTerminalResult, runTranscriptAwareProcess,
  snapshotClaudeTranscripts } from '../src/agents/claude-terminal-recovery.mjs';

const sessionId = '950df556-38bb-429c-aee9-1af4a00a6c7a';
const assistant = ({ text = 'FIX_COMPLETE', stop = 'end_turn', request = 'request-1',
  model = 'claude-sonnet-5' } = {}) => ({
  type: 'assistant', isSidechain: false, sessionId, requestId: request,
  message: { id: `message-${request}`, model, stop_reason: stop,
    content: [{ type: 'text', text }], usage: {
      input_tokens: 10, output_tokens: 20, cache_creation_input_tokens: 30,
      cache_read_input_tokens: 40,
      cache_creation: { ephemeral_5m_input_tokens: 30, ephemeral_1h_input_tokens: 0 },
    } },
});

test('terminal recovery reads only records appended by the active invocation', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const transcript = join(root, `${sessionId}.jsonl`);
    writeFileSync(transcript, `${JSON.stringify(assistant({ request: 'old' }))}\n`);
    const snapshot = snapshotClaudeTranscripts(root);
    writeFileSync(transcript, `${JSON.stringify(assistant())}\n`, { flag: 'a' });
    const result = recoverClaudeTerminalResult({ directory: root, snapshot,
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5', resumeSession: sessionId });
    assert.equal(result.session_id, sessionId);
    assert.equal(result.result, 'FIX_COMPLETE');
    assert.equal(result.num_turns, 1);
    assert.deepEqual(result.usage, { input_tokens: 10, output_tokens: 20,
      cache_creation_input_tokens: 30, cache_read_input_tokens: 40 });
    assert.equal(result.total_cost_usd, 0.000303);
    assert.equal(result.terminal_recovery.costSource, 'transcript-usage');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('terminal recovery rejects old markers, tool turns, sidechains, and unknown pricing', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const transcript = join(root, `${sessionId}.jsonl`);
    writeFileSync(transcript, `${JSON.stringify(assistant())}\n`);
    const snapshot = snapshotClaudeTranscripts(root);
    writeFileSync(transcript, [
      JSON.stringify(assistant({ stop: 'tool_use', request: 'tool' })),
      JSON.stringify({ ...assistant({ request: 'sidechain' }), isSidechain: true }),
    ].join('\n') + '\n', { flag: 'a' });
    assert.equal(recoverClaudeTerminalResult({ directory: root, snapshot,
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5', resumeSession: sessionId }), null);
    writeFileSync(transcript,
      `${JSON.stringify(assistant({ request: 'done', model: 'claude-unknown' }))}\n`, { flag: 'a' });
    assert.throws(() => recoverClaudeTerminalResult({ directory: root, snapshot,
      marker: 'FIX_COMPLETE', model: 'claude-unknown', resumeSession: sessionId }),
    /no recorded pricing/);
    const exactRates = { input: 1, output: 1, cacheWrite5m: 1,
      cacheWrite1h: 1, cacheRead: 1 };
    const recovered = recoverClaudeTerminalResult({ directory: root, snapshot,
      marker: 'FIX_COMPLETE', model: 'claude-unknown', pricingRates: exactRates,
      resumeSession: sessionId });
    assert.equal(recovered.total_cost_usd, 0.0002);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('terminal recovery includes subagent usage created by the active session', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const snapshot = snapshotClaudeTranscripts(root);
    writeFileSync(join(root, `${sessionId}.jsonl`), `${JSON.stringify(assistant())}\n`);
    const subagents = join(root, sessionId, 'subagents');
    mkdirSync(subagents, { recursive: true });
    writeFileSync(join(subagents, 'agent-worker.jsonl'),
      `${JSON.stringify({ ...assistant({ request: 'subagent' }), isSidechain: true })}\n`);
    const result = recoverClaudeTerminalResult({ directory: root, snapshot,
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5' });
    assert.equal(result.num_turns, 2);
    assert.deepEqual(result.usage, { input_tokens: 20, output_tokens: 40,
      cache_creation_input_tokens: 60, cache_read_input_tokens: 80 });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a hung process becomes a successful transcript recovery after the exit grace', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const transcript = join(root, `${sessionId}.jsonl`);
    const snapshot = snapshotClaudeTranscripts(root);
    const script = `const fs=require('node:fs');`
      + `fs.writeFileSync(${JSON.stringify(transcript)}, JSON.stringify(${JSON.stringify(assistant())})+'\\n');`
      + `setInterval(()=>{},1000);`;
    const result = await runTranscriptAwareProcess({ command: process.execPath,
      args: ['-e', script], input: '', env: process.env, timeoutMs: 5_000,
      transcriptDirectory: root, transcriptSnapshot: snapshot,
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5',
      exitGraceMs: 20, pollMs: 10,
      terminate: child => child.kill('SIGTERM') });
    assert.equal(result.status, 0);
    assert.equal(JSON.parse(result.stdout).session_id, sessionId);
    assert.equal(result.terminalRecovery.kind, 'terminal-transcript');
    assert.equal(result.error, null);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a normal CLI exit keeps its authoritative result instead of using recovery', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const transcript = join(root, `${sessionId}.jsonl`);
    const snapshot = snapshotClaudeTranscripts(root);
    const cliResult = { is_error: false, session_id: sessionId,
      result: 'FIX_COMPLETE', total_cost_usd: 9.25, num_turns: 7, usage: {
        input_tokens: 1, output_tokens: 2, cache_creation_input_tokens: 3,
        cache_read_input_tokens: 4,
      } };
    const script = `const fs=require('node:fs');`
      + `fs.writeFileSync(${JSON.stringify(transcript)}, JSON.stringify(${JSON.stringify(assistant())})+'\\n');`
      + `process.stdout.write(${JSON.stringify(`${JSON.stringify(cliResult)}\n`)});`;
    const result = await runTranscriptAwareProcess({ command: process.execPath,
      args: ['-e', script], input: '', env: process.env, timeoutMs: 5_000,
      transcriptDirectory: root, transcriptSnapshot: snapshot,
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5', exitGraceMs: 100, pollMs: 10,
      terminate: child => child.kill('SIGTERM') });
    assert.equal(result.status, 0);
    assert.equal(result.stdout, `${JSON.stringify(cliResult)}\n`);
    assert.equal(result.terminalRecovery, undefined);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('complete CLI JSON stays authoritative when process close exceeds transcript grace', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const transcript = join(root, `${sessionId}.jsonl`);
    const snapshot = snapshotClaudeTranscripts(root);
    const cliResult = { is_error: false, session_id: sessionId,
      result: 'the authoritative response\nFIX_COMPLETE', total_cost_usd: 9.25,
      num_turns: 7, usage: { input_tokens: 101, output_tokens: 202,
        cache_creation_input_tokens: 303, cache_read_input_tokens: 404 } };
    const script = `const fs=require('node:fs');`
      + `fs.writeFileSync(${JSON.stringify(transcript)}, JSON.stringify(${JSON.stringify(assistant())})+'\\n');`
      + `process.stdout.write(${JSON.stringify(`${JSON.stringify(cliResult)}\n`)});`
      + `setInterval(()=>{},1000);`;
    const result = await runTranscriptAwareProcess({ command: process.execPath,
      args: ['-e', script], input: '', env: process.env, timeoutMs: 5_000,
      transcriptDirectory: root, transcriptSnapshot: snapshot,
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5', exitGraceMs: 20, pollMs: 10,
      terminate: child => child.kill('SIGTERM') });
    const returned = JSON.parse(result.stdout);
    assert.equal(result.status, 0);
    assert.equal(returned.total_cost_usd, cliResult.total_cost_usd);
    assert.deepEqual(returned.usage, cliResult.usage);
    assert.equal(returned.num_turns, cliResult.num_turns);
    assert.equal(returned.result, cliResult.result);
    assert.equal(returned.terminal_recovery.kind, 'terminal-process');
    assert.equal(returned.terminal_recovery.resultSource, 'cli-json');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a terminal result with unknown pricing fails closed after the grace period', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const transcript = join(root, `${sessionId}.jsonl`);
    const snapshot = snapshotClaudeTranscripts(root);
    const script = `const fs=require('node:fs');`
      + `fs.writeFileSync(${JSON.stringify(transcript)}, JSON.stringify(${JSON.stringify(
        assistant({ model: 'claude-unknown' }))})+'\\n');`
      + `setInterval(()=>{},1000);`;
    const result = await runTranscriptAwareProcess({ command: process.execPath,
      args: ['-e', script], input: '', env: process.env, timeoutMs: 5_000,
      transcriptDirectory: root, transcriptSnapshot: snapshot,
      marker: 'FIX_COMPLETE', model: 'claude-unknown',
      exitGraceMs: 20, pollMs: 10,
      terminate: child => child.kill('SIGTERM') });
    assert.notEqual(result.status, 0);
    assert.equal(result.error.code, 'CLAUDE_TERMINAL_RECOVERY_UNAVAILABLE');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
