import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { runAuditNetworkContext } from '../commands/bench.js';
import { createBackendLease } from '../src/runtime/backend-lease.js';
import { loadTrack } from '../src/composition/tracks.js';
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
      { message: { content: [{ type: 'tool_use', id: 'own-wildcard', name: 'Bash',
        input: { command: 'curl -s http://0.0.0.0:6173/api/items' } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: 'own-wildcard', is_error: false }] } },
      { message: { content: [{ type: 'tool_use', id: 'other-wildcard', name: 'Bash',
        input: { command: 'curl -s http://0.0.0.0:6174/api/items' } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: 'other-wildcard', is_error: false }] } },
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

    const result = auditTranscript(transcript, '/app', { ownEndpoints: ['127.0.0.1:6173', '127.0.0.1:6001', '127.0.0.1:3210'] });

    assert.deepEqual(result.hits.map(hit => ({ path: hit.path, kind: hit.kind })), [
      { path: '0.0.0.0:6174', kind: 'NETWORK / OTHER RUN' },
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


test('leased database endpoints permit the supplied namespace, not another host at the same port', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-owned-endpoints-'));
  try {
    for (const [backend, port] of [['mongodb', 27017], ['postgres', 5432], ['spacetime', 3210]] as const) {
      const lease = createBackendLease({ runId: 'endpoint-test', backend, track: 'ecommerce',
        runIndex: 1, ...(backend === 'spacetime'
          ? { serverUri: 'http://127.0.0.1:3210', module: 'app-ecom-run1', dataDir: join(root, 'spacetime') }
          : { database: 'app_ecom_run1' }) });
      lease.resources.network = { name: 'owned', id: 'network', namespaceContainerId: 'namespace',
        hostAddresses: [], services: [{ address: '172.20.0.2', port: 4873 }],
        ownAddresses: ['172.20.0.3'],
        cacheContainerId: 'cache', firewallSha256: null, firewallInstalledAt: null };
      const context = () => runAuditNetworkContext(loadTrack('ecommerce'), { backend, runIndex: 1 }, lease);
      const { ownEndpoints } = context();
      assert.equal(context().isolatedLoopback, false);
      lease.resources.container = { name: 'backend', id: 'namespace', owned: true };
      lease.resources.buildContainer = { name: 'build', id: 'build', owned: true, networkMode: 'container:namespace' };
      lease.resources.network.firewallSha256 = 'a'.repeat(64);
      lease.resources.network.firewallInstalledAt = new Date().toISOString();
      assert.equal(context().isolatedLoopback, true);
      for (const mode of ['host', 'bridge', 'container:foreign']) {
        lease.resources.buildContainer.networkMode = mode;
        assert.equal(context().isolatedLoopback, false, mode);
      }
      assert(ownEndpoints.includes(`127.0.0.1:${port}`));
      // The attempt's own bridge address owns the same ports; a probe there is not another run.
      assert(ownEndpoints.includes(`172.20.0.3:${port}`));
      assert(!ownEndpoints.includes(`172.20.0.99:${port}`));
      assert(!ownEndpoints.some(value => value.includes('@') || value.includes(lease.ownershipToken)));
      const transcript = join(root, `${backend}.jsonl`);
      const events = [
        { cwd: '/app', message: { content: [{ type: 'tool_use', id: 'own', name: 'Bash',
          input: { command: `which mongosh mongo; ls /usr/bin | grep -i mongo; nc -zv 127.0.0.1 ${port} 2>&1; npm config get registry` } }] } },
        { message: { content: [{ type: 'tool_result', tool_use_id: 'own', is_error: false,
          content: '/bin/bash: line 1: nc: command not found' }] } },
        { message: { content: [{ type: 'tool_use', id: 'foreign', name: 'Bash',
          input: { command: `nc -zv 172.20.0.99 ${port}` } }] } },
        { message: { content: [{ type: 'tool_result', tool_use_id: 'foreign', is_error: false }] } },
      ];
      writeFileSync(transcript, events.map(event => JSON.stringify(event)).join('\n'));
      const audit = auditTranscript(transcript, '/app', { ownEndpoints });
      assert.deepEqual(audit.hits.map(hit => ({ path: hit.path, via: hit.via, kind: hit.kind })), [
        { path: `172.20.0.99:${port}`, via: 'Bash network attempt', kind: 'NETWORK / OTHER RUN' },
      ]);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('verified isolated loopback allows temporary test ports but keeps private hosts restricted', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-isolated-network-'));
  try {
    const transcript = join(root, 'session.jsonl');
    const loopback = ['localhost:6574', '127.0.0.1:6579', '127.0.0.2:7331', '[::1]:6574', '0.0.0.0:6574'];
    const privateHosts = ['host.docker.internal:6574', '10.0.0.1:6574', '172.20.0.99:6574', '192.168.0.1:6574'];
    writeFileSync(transcript, [...loopback, ...privateHosts].flatMap((endpoint, i) => [
      { cwd: '/app', message: { content: [{ type: 'tool_use', id: String(i), name: 'Bash',
        input: { command: `curl http://${endpoint}/` } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: String(i), is_error: false }] } },
    ]).map(event => JSON.stringify(event)).join('\n'));
    // /app in a transcript is not proof of isolation; the default remains strict.
    assert.equal(auditTranscript(transcript, '/app').hits.length, loopback.length + privateHosts.length);
    const isolated = auditTranscript(transcript, '/app', { isolatedLoopback: true });
    assert.deepEqual(isolated.hits.map(hit => hit.path), privateHosts);
    assert(isolated.hits.every(hit => hit.kind === 'NETWORK / OTHER RUN'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});
