import assert from 'node:assert/strict';
import test from 'node:test';
import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setImmediate as nextTurn } from 'node:timers/promises';
import { attemptTranscriptFiles, readAttemptTranscript, transcriptMessages } from '../../dashboard/dashboard-transcript.js';
import { emptyArtifactIdentities, writeArtifact } from '../../src/evidence/artifacts.js';

test('transcript normalizes Claude and Codex text/tools, redacts credentials, and ignores metadata', () => {
  const rows = [
    { type: 'assistant', message: { role: 'assistant', content: [{ type: 'text', text: '<script>hello</script>' },
      { type: 'tool_use', name: 'Bash', input: { command: 'echo hello' } }] } },
    { type: 'user', message: { role: 'user', content: [{ type: 'tool_result', content: 'Authorization: Bearer secret-example-token' }] } },
    { type: 'response_item', payload: { role: 'assistant', content: [{ type: 'output_text', text: 'Codex reply' }] } },
    { type: 'response_item', payload: { type: 'function_call', name: 'exec_command', arguments: '{"cmd":"ls"}' } },
    { type: 'item.completed', item: { type: 'command_execution', command: 'ls', aggregated_output: 'file.ts' } },
    { type: 'system', accessToken: 'never-show-metadata' },
  ];
  const result = transcriptMessages(rows.map(row => JSON.stringify(row)).join('\n') + '\n{broken');
  assert.equal(result.messages.length, 6);
  assert.equal(result.messages.filter(message => message.tool).length, 4);
  assert.equal(result.skipped, 1);
  assert.doesNotMatch(JSON.stringify(result), /secret-example-token|never-show-metadata/);
  assert.equal(result.messages[0]?.text, '<script>hello</script>'); // Escaped by the view, never interpreted as markup.
});

test('live transcript reads yield, share concurrent reads, and retain lease and range guards', async t => {
  const directory = mkdtempSync(join(tmpdir(), 'dashboard-transcript-'));
  const executions = [{ directory, label: 'Execution 1' }];
  writeArtifact(join(directory, 'backend-lease.json'), { kind: 'backend_lease_evidence', id: 'lease',
    identities: emptyArtifactIdentities(), payload: { track: 'ecommerce', backend: 'spacetime',
      runIndex: 1, runId: 'transcript-test', state: 'active',
      resources: { buildContainer: { owned: true, name: 'owned-build', id: 'exact-id' } } } });
  const text = JSON.stringify({ type: 'assistant', message: { role: 'assistant', content: 'hello' } }) + '\n';
  let actualId = 'exact-id', calls = 0;
  const pending: Array<() => void> = [];
  t.mock.method(childProcess, 'execFile', (...args: unknown[]) => {
    calls++;
    assert.equal(args[0], 'docker');
    assert.deepEqual(args[2], { timeout: 5000, maxBuffer: 2 * 1024 * 1024, encoding: 'buffer' });
    const command = args[1] as string[];
    const reply = command[0] === 'inspect' ? actualId
      : command.at(-3) === '' ? JSON.stringify([['session.jsonl', Buffer.byteLength(text), 1000]]) : text;
    const done = args[3] as (error: null, stdout: Buffer) => void;
    pending.push(() => done(null, Buffer.from(reply)));
  });
  syncBuiltinESMExports();
  try {
    const first = readAttemptTranscript(executions, 'claude-code');
    const second = readAttemptTranscript(executions, 'claude-code');
    await nextTurn(); // Other HTTP work can proceed while Docker has not answered.
    assert.equal(calls, 1);
    for (let phase = 0; phase < 3; phase++) {
      assert.equal(pending.length, 1);
      pending.shift()!();
      await nextTurn();
    }
    assert.deepEqual(await first, await second);
    assert.equal((await first).messages[0]?.text, 'hello');
    assert.equal(calls, 3); // One inspect, listing, and page read for both viewers.
    const files = attemptTranscriptFiles(executions, 'claude-code');
    for (let phase = 0; phase < 2; phase++) {
      pending.shift()!();
      await nextTurn();
    }
    await assert.rejects((await files)[0]!.read(0, 256 * 1024 + 1), /Invalid transcript range/);
    actualId = 'replacement-id';
    const rejected = assert.rejects(readAttemptTranscript(executions, 'claude-code'), /changed after lease creation/);
    pending.shift()!();
    await rejected;
    assert.equal(pending.length, 0); // A changed container never gets a transcript command.
  } finally {
    t.mock.restoreAll();
    syncBuiltinESMExports();
    rmSync(directory, { recursive: true, force: true });
  }
});
