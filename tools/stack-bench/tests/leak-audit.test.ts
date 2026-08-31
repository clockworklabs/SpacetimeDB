import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { auditTranscript, pathsFromBash } from '../commands/leak-audit.js';

test('Bash reader extraction keeps absolute file arguments', () => {
  assert.deepEqual(pathsFromBash('cat /app/src/main.ts; rg secret /outside/notes.md'),
    ['/app/src/main.ts', '/outside/notes.md']);
});

test('a completed external Bash read contaminates the transcript', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-leak-audit-'));
  const transcript = join(root, 'session.jsonl');
  try {
    const events = [
      { cwd: '/app', message: { content: [{ type: 'tool_use', id: 'read-1', name: 'Bash',
        input: { command: 'cat /tools/stack-bench/grader/grade.ts' } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: 'read-1', is_error: false }] } },
    ];
    writeFileSync(transcript, `${events.map(event => JSON.stringify(event)).join('\n')}\n`);

    const result = auditTranscript(transcript, '/app');

    assert.equal(result.cwd, '/app');
    assert.equal(result.refused.length, 0);
    assert.deepEqual(result.hits.map(hit => ({ path: hit.path, kind: hit.kind })), [{
      path: '/tools/stack-bench/grader/grade.ts',
      kind: 'GRADER / TEST SPECS',
    }]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
