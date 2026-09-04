import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { auditTranscript, networkTargetsFromBash, pathsFromBash } from '../commands/leak-audit.js';

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

test('private grading reads inside the app contaminate the transcript', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-private-leak-audit-'));
  const transcript = join(root, 'session.jsonl');
  try {
    const events = [
      { cwd: '/app', message: { content: [{ type: 'tool_use', id: 'read-1', name: 'Bash',
        input: { command: 'cat stack-bench/bundle.json' } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: 'read-1', is_error: false }] } },
    ];
    writeFileSync(transcript, `${events.map(event => JSON.stringify(event)).join('\n')}\n`);

    const result = auditTranscript(transcript, '/app');

    assert.deepEqual(result.hits.map(hit => ({ path: hit.path, kind: hit.kind })), [{
      path: '/app/stack-bench/bundle.json',
      kind: 'GRADER / TEST SPECS',
    }]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('network targets come from URLs and raw sockets in a shell command', () => {
  assert.deepEqual(networkTargetsFromBash(
    'curl -s http://127.0.0.1:7331/api/overview && wget https://registry.npmjs.org/react; '
    + 'nc -z host.docker.internal 6532; node -e "fetch(\'http://localhost:6173/\')"'), [
    { host: '127.0.0.1', port: 7331 },
    { host: 'registry.npmjs.org', port: null },
    { host: 'localhost', port: 6173 },
    { host: 'host.docker.internal', port: 6532 },
  ]);
  assert.deepEqual(networkTargetsFromBash('npm install && npm run build'), []);
});

test('a local network read outside the run\'s own ports contaminates the transcript', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-network-leak-audit-'));
  const transcript = join(root, 'session.jsonl');
  try {
    const events = [
      { cwd: '/app', message: { content: [{ type: 'tool_use', id: 'own', name: 'Bash',
        input: { command: 'curl -s http://localhost:6173/api/items' } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: 'own', is_error: false }] } },
      { message: { content: [{ type: 'tool_use', id: 'other', name: 'Bash',
        input: { command: 'curl -s http://127.0.0.1:3211/v1/database/app-ecom-run1/schema' } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: 'other', is_error: false }] } },
      { message: { content: [{ type: 'tool_use', id: 'blocked', name: 'Bash',
        input: { command: 'curl -s http://127.0.0.1:7331/api/overview' } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: 'blocked', is_error: true }] } },
      { message: { content: [{ type: 'tool_use', id: 'registry', name: 'Bash',
        input: { command: 'curl -sI https://registry.npmjs.org/express' } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: 'registry', is_error: false }] } },
    ];
    writeFileSync(transcript, `${events.map(event => JSON.stringify(event)).join('\n')}\n`);

    const result = auditTranscript(transcript, '/app', { ownPorts: [6173, 6001, 3210] });

    assert.deepEqual(result.hits.map(hit => ({ path: hit.path, kind: hit.kind })), [
      { path: '127.0.0.1:3211', kind: 'NETWORK / OTHER RUN' },
      { path: 'registry.npmjs.org', kind: 'network (internet)' },
    ]);
    assert.deepEqual(result.refused.map(hit => ({ path: hit.path, kind: hit.kind })), [
      { path: '127.0.0.1:7331', kind: 'NETWORK / OTHER RUN' },
    ]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
