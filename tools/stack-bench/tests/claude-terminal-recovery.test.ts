import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { recoverClaudeTerminalResult, runTranscriptAwareProcess,
  type ClaudeTranscriptReader } from '../src/agents/claude-terminal-recovery.js';
import { CONTAINER_CLAUDE_TRANSCRIPT_READ, containerClaudeTranscriptReader }
  from '../container/claude-transcript-reader.js';

const sessionId = '950df556-38bb-429c-aee9-1af4a00a6c7a';

function localReader(root: string): ClaudeTranscriptReader {
  return {
    snapshot: () => new Map(readdirSync(root, { recursive: true, withFileTypes: true })
      .filter(entry => entry.isFile() && entry.name.endsWith('.jsonl'))
      .map(entry => join(entry.parentPath, entry.name)).map(path => [path, statSync(path).size])),
    read: (path, start, length) => readFileSync(path).subarray(start, start + length),
  };
}
interface AssistantOptions {
  text?: string;
  stop?: string;
  request?: string;
  model?: string;
}

const assistant = ({ text = 'FIX_COMPLETE', stop = 'end_turn', request = 'request-1',
  model = 'claude-sonnet-5' }: AssistantOptions = {}) => ({
  type: 'assistant', isSidechain: false, sessionId, requestId: request,
  message: { id: `message-${request}`, model, stop_reason: stop,
    content: [{ type: 'text', text }], usage: {
      input_tokens: 10, output_tokens: 20, cache_creation_input_tokens: 30,
      cache_read_input_tokens: 40,
      cache_creation: { ephemeral_5m_input_tokens: 30, ephemeral_1h_input_tokens: 0 },
    } },
});

test('container transcript command reads private ranges and nested usage only inside the attempt', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  const directory = join(root, 'attempt');
  mkdirSync(directory);
  try {
    const transcript = join(directory, `${sessionId}.jsonl`);
    const contents = `${JSON.stringify(assistant())}\n`;
    writeFileSync(transcript, contents, { mode: 0o600 });
    const nested = join(directory, sessionId, 'subagents');
    mkdirSync(nested, { recursive: true });
    writeFileSync(join(nested, 'agent-worker.jsonl'),
      `${JSON.stringify({ ...assistant({ request: 'nested' }), isSidechain: true })}\n`, { mode: 0o600 });
    writeFileSync(join(root, 'foreign.jsonl'), 'private');
    const command = (name: string, start = 0, length = 0) => execFileSync(process.execPath,
      ['-e', CONTAINER_CLAUDE_TRANSCRIPT_READ, directory, name, String(start), String(length)],
      { stdio: ['ignore', 'pipe', 'pipe'] });
    const entries: [string, number][] = JSON.parse(command('').toString('utf8'));
    assert.equal(entries.length, 2);
    assert.equal(command(`${sessionId}.jsonl`, 3, 12).toString(), contents.slice(3, 15));
    assert.throws(() => command('../foreign.jsonl', 0, 7), /outside the attempt directory/);
    const reader = {
      snapshot: () => new Map(entries.map(([name, size]) => [join(directory, name), size])),
      read: (path: string, start: number, length: number) =>
        command(path.slice(directory.length + 1), start, length),
    };
    const recovered = recoverClaudeTerminalResult({ directory, snapshot: new Map(), reader,
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5' });
    assert.equal(recovered?.num_turns, 2);
    assert.throws(() => containerClaudeTranscriptReader('mutable-name', directory, {}), /exact container ID/);
    const owned = containerClaudeTranscriptReader('a'.repeat(64), directory, {});
    assert.throws(() => owned.read(join(root, 'foreign.jsonl'), 0, 7), /outside the attempt directory/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('hosted fallback reads through its supplied owner and polls only the transcript tail', async () => {
  const directory = join(tmpdir(), 'not-a-host-transcript-directory');
  const path = join(directory, `${sessionId}.jsonl`);
  const content = Buffer.from(' '.repeat(200_000) + '\n' + JSON.stringify(assistant()) + '\n');
  const reads: [number, number][] = [];
  const result = await runTranscriptAwareProcess({ command: process.execPath,
    args: ['-e', 'setInterval(()=>{},1000)'], timeoutMs: 5_000,
    transcriptDirectory: directory, transcriptSnapshot: new Map(),
    transcriptReader: {
      snapshot: () => new Map([[path, content.length]]),
      read(file, start, length) {
        assert.equal(file, path);
        reads.push([start, length]);
        return content.subarray(start, start + length);
      },
    },
    marker: 'FIX_COMPLETE', model: 'claude-sonnet-5', exitGraceMs: 80, pollMs: 10,
    terminate: child => child.kill('SIGTERM'),
  });
  assert.equal(result.status, 0);
  assert.equal(result.terminalRecovery?.kind, 'terminal-transcript');
  assert.deepEqual(reads[0], [content.length - 128 * 1024, 128 * 1024]);
  assert(reads.length > 3, 'the grace period must include multiple tail polls');
  assert.equal(reads.filter(([start]) => start === 0).length, 1,
    'full records must be read only once after grace expires');
});

test('new assistant activity invalidates an earlier completion marker during grace', async () => {
  const directory = join(tmpdir(), 'not-a-host-transcript-directory');
  const path = join(directory, `${sessionId}.jsonl`);
  let snapshots = 0;
  let content = Buffer.from(JSON.stringify(assistant()) + '\n');
  const result = await runTranscriptAwareProcess({ command: process.execPath,
    args: ['-e', 'setTimeout(()=>process.exit(1),250)'], timeoutMs: 5_000,
    transcriptDirectory: directory, transcriptSnapshot: new Map(),
    transcriptReader: {
      snapshot() {
        if (++snapshots === 3) content = Buffer.concat([content,
          Buffer.from(JSON.stringify(assistant({ text: 'more work', stop: 'tool_use', request: 'later' })) + '\n')]);
        return new Map([[path, content.length]]);
      },
      read: (_, start, length) => content.subarray(start, start + length),
    },
    marker: 'FIX_COMPLETE', model: 'claude-sonnet-5', exitGraceMs: 50, pollMs: 10,
    terminate() { assert.fail('new activity must prevent terminal recovery'); },
  });
  assert.equal(result.status, 1);
  assert.equal(result.terminalRecovery, undefined);
});

test('an unreadable fallback does not replace the provider process failure', async () => {
  const result = await runTranscriptAwareProcess({ command: process.execPath,
    args: ['-e', 'setTimeout(()=>{process.stdout.write("API Error: Server error mid-response");process.exit(1)},200)'],
    timeoutMs: 5_000, transcriptDirectory: '/private', transcriptSnapshot: new Map(),
    transcriptReader: { snapshot() { throw Object.assign(new Error('private transcript'), { code: 'EACCES' }); },
      read() { throw new Error('must not read'); } },
    marker: 'FIX_COMPLETE', model: 'claude-sonnet-5', pollMs: 10,
  });
  assert.equal(result.status, 1);
  assert.equal(result.error, null);
  assert.match(result.stdout, /API Error: Server error mid-response/);
  assert.match(result.stderr, /Transcript fallback unavailable: private transcript/);
});

test('terminal recovery reads only records appended by the active invocation', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const transcript = join(root, `${sessionId}.jsonl`);
    writeFileSync(transcript, `${JSON.stringify(assistant({ request: 'old' }))}\n`);
    const snapshot = localReader(root).snapshot();
    writeFileSync(transcript, `${JSON.stringify(assistant())}\n`, { flag: 'a' });
    const result = recoverClaudeTerminalResult({ directory: root, snapshot, reader: localReader(root),
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5', resumeSession: sessionId });
    assert(result, 'the active transcript must recover a result');
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
    const snapshot = localReader(root).snapshot();
    writeFileSync(transcript, [
      JSON.stringify(assistant({ stop: 'tool_use', request: 'tool' })),
      JSON.stringify({ ...assistant({ request: 'sidechain' }), isSidechain: true }),
    ].join('\n') + '\n', { flag: 'a' });
    assert.equal(recoverClaudeTerminalResult({ directory: root, snapshot, reader: localReader(root),
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5', resumeSession: sessionId }), null);
    writeFileSync(transcript,
      `${JSON.stringify(assistant({ request: 'done', model: 'claude-unknown' }))}\n`, { flag: 'a' });
    assert.throws(() => recoverClaudeTerminalResult({ directory: root, snapshot, reader: localReader(root),
      marker: 'FIX_COMPLETE', model: 'claude-unknown', resumeSession: sessionId }),
    /no recorded pricing/);
    const exactRates = { input: 1, output: 1, cacheWrite5m: 1,
      cacheWrite1h: 1, cacheRead: 1 };
    const recovered = recoverClaudeTerminalResult({ directory: root, snapshot, reader: localReader(root),
      marker: 'FIX_COMPLETE', model: 'claude-unknown', pricingRates: exactRates,
      resumeSession: sessionId });
    assert(recovered, 'the priced transcript must recover a result');
    assert.equal(recovered.total_cost_usd, 0.0002);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('terminal recovery includes subagent usage created by the active session', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const snapshot = localReader(root).snapshot();
    writeFileSync(join(root, `${sessionId}.jsonl`), `${JSON.stringify(assistant())}\n`);
    const subagents = join(root, sessionId, 'subagents');
    mkdirSync(subagents, { recursive: true });
    writeFileSync(join(subagents, 'agent-worker.jsonl'),
      `${JSON.stringify({ ...assistant({ request: 'subagent' }), isSidechain: true })}\n`);
    const result = recoverClaudeTerminalResult({ directory: root, snapshot, reader: localReader(root),
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5' });
    assert(result, 'the session transcript must recover a result');
    assert.equal(result.num_turns, 2);
    assert.deepEqual(result.usage, { input_tokens: 20, output_tokens: 40,
      cache_creation_input_tokens: 60, cache_read_input_tokens: 80 });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('terminal recovery rejects billable usage without a stable request ID', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const snapshot = localReader(root).snapshot();
    const record = assistant() as Record<string, unknown>;
    delete record.requestId;
    delete (record.message as Record<string, unknown>).id;
    writeFileSync(join(root, `${sessionId}.jsonl`), `${JSON.stringify(record)}\n`);
    assert.throws(() => recoverClaudeTerminalResult({ directory: root, snapshot, reader: localReader(root),
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5' }), /stable request ID/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('terminal recovery rejects conflicting usage for one request ID', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const snapshot = localReader(root).snapshot();
    const changed = assistant();
    (changed.message.usage as Record<string, unknown>).output_tokens = 21;
    writeFileSync(join(root, `${sessionId}.jsonl`), [
      JSON.stringify(assistant({ stop: 'tool_use' })), JSON.stringify(changed),
    ].join('\n') + '\n');
    assert.throws(() => recoverClaudeTerminalResult({ directory: root, snapshot, reader: localReader(root),
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5' }), /usage changed/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a hung process becomes a successful transcript recovery after the exit grace', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const transcript = join(root, `${sessionId}.jsonl`);
    const snapshot = localReader(root).snapshot();
    const script = `const fs=require('node:fs');`
      + `fs.writeFileSync(${JSON.stringify(transcript)}, JSON.stringify(${JSON.stringify(assistant())})+'\\n');`
      + `setInterval(()=>{},1000);`;
    const result = await runTranscriptAwareProcess({ command: process.execPath,
      args: ['-e', script], input: '', env: process.env, timeoutMs: 5_000,
      transcriptDirectory: root, transcriptSnapshot: snapshot, transcriptReader: localReader(root),
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5',
      exitGraceMs: 20, pollMs: 10,
      terminate: child => child.kill('SIGTERM') });
    assert.equal(result.status, 0);
    assert.equal(jsonRecord(result.stdout).session_id, sessionId);
    assert(result.terminalRecovery, 'transcript recovery evidence is required');
    assert.equal(result.terminalRecovery.kind, 'terminal-transcript');
    assert.equal(result.error, null);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a normal CLI exit keeps its authoritative result instead of using recovery', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const transcript = join(root, `${sessionId}.jsonl`);
    const snapshot = localReader(root).snapshot();
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
      transcriptDirectory: root, transcriptSnapshot: snapshot, transcriptReader: localReader(root),
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
    const snapshot = localReader(root).snapshot();
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
      transcriptDirectory: root, transcriptSnapshot: snapshot, transcriptReader: localReader(root),
      marker: 'FIX_COMPLETE', model: 'claude-sonnet-5', exitGraceMs: 20, pollMs: 10,
      terminate: child => child.kill('SIGTERM') });
    const returned = jsonRecord(result.stdout);
    assert.equal(result.status, 0);
    assert.equal(returned.total_cost_usd, cliResult.total_cost_usd);
    assert.deepEqual(returned.usage, cliResult.usage);
    assert.equal(returned.num_turns, cliResult.num_turns);
    assert.equal(returned.result, cliResult.result);
    const terminalRecovery = jsonRecordValue(returned.terminal_recovery, 'terminal recovery');
    assert.equal(terminalRecovery.kind, 'terminal-process');
    assert.equal(terminalRecovery.resultSource, 'cli-json');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a terminal result with unknown pricing fails closed after the grace period', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-terminal-'));
  try {
    const transcript = join(root, `${sessionId}.jsonl`);
    const snapshot = localReader(root).snapshot();
    const script = `const fs=require('node:fs');`
      + `fs.writeFileSync(${JSON.stringify(transcript)}, JSON.stringify(${JSON.stringify(
        assistant({ model: 'claude-unknown' }))})+'\\n');`
      + `setInterval(()=>{},1000);`;
    const result = await runTranscriptAwareProcess({ command: process.execPath,
      args: ['-e', script], input: '', env: process.env, timeoutMs: 5_000,
      transcriptDirectory: root, transcriptSnapshot: snapshot, transcriptReader: localReader(root),
      marker: 'FIX_COMPLETE', model: 'claude-unknown',
      exitGraceMs: 20, pollMs: 10,
      terminate: child => child.kill('SIGTERM') });
    assert.notEqual(result.status, 0);
    assert.equal(errorCode(result.error), 'CLAUDE_TERMINAL_RECOVERY_UNAVAILABLE');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

function jsonRecord(raw: string): Record<string, unknown> {
  const value: unknown = JSON.parse(raw);
  return jsonRecordValue(value, 'process result');
}

function errorCode(error: unknown): string | undefined {
  if (!isRecord(error)) return undefined;
  return typeof error.code === 'string' ? error.code : undefined;
}

function jsonRecordValue(value: unknown, label: string): Record<string, unknown> {
  if (!isRecord(value)) throw new Error(`${label} must be a JSON object`);
  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
