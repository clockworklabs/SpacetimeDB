import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { runAuditNetworkContext } from '../commands/bench.js';
import { createBackendLease } from '../src/runtime/backend-lease.js';
import { loadTrack } from '../src/composition/tracks.js';
import { auditTranscript, networkTargetsFromBash, STACKBENCH_SOURCE, stackBenchSourceReferences }
  from '../commands/leak-audit.js';
import { archiveTranscripts, transcriptDirectories } from '../src/agents/transcript-archive.js';
import { codexTranscriptDirectory } from '../src/agents/codex-protocol.js';

test('Codex tool events use the shared restricted access and refusal audit', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-codex-audit-'));
  const transcript = join(root, 'session.events.jsonl');
  try {
    const events = [
      { type: 'stack_bench_context', cwd: '/app' },
      { type: 'item.completed', item: { id: 'read', type: 'command_execution',
        command: "/bin/bash -lc 'cat /tools/stack-bench/grader/grade.ts'",
        status: 'completed', exit_code: 0, aggregated_output: 'private contents' } },
      { type: 'item.completed', item: { id: 'net', type: 'command_execution',
        command: 'curl http://host.docker.internal:7331/api/overview',
        status: 'completed', exit_code: 0, aggregated_output: '{}' } },
      { type: 'item.completed', item: { id: 'edit', type: 'file_change',
        status: 'completed', changes: [{ path: '/app/src/../stack-bench/bundle.json', kind: 'update' }] } },
      { type: 'item.completed', item: { id: 'refused', type: 'file_change',
        status: 'failed', changes: [{ path: '/tools/stack-bench/grader/grade.ts', kind: 'update' }] } },
      { type: 'item.started', item: { id: 'cut', type: 'command_execution',
        command: 'cat /tools/stack-bench/scenarios/test.json', status: 'in_progress' } },
    ];
    writeFileSync(transcript, events.map(event => JSON.stringify(event)).join('\n'));
    const audit = auditTranscript(transcript, 'C:/host/app');
    assert.equal(audit.cwd, '/app');
    assert.deepEqual(audit.hits.map(hit => hit.path), [
      '/tools/stack-bench/grader/grade.ts', 'host.docker.internal:7331',
      '/app/stack-bench/bundle.json', '/tools/stack-bench/scenarios/test.json',
    ]);
    assert.equal(audit.hits.at(-1)?.unresolved, true);
    assert.equal(audit.refused.length, 1);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('failed Codex commands retain partial reads but refuse empty output', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-codex-refusal-'));
  const transcript = join(root, 'session.events.jsonl');
  try {
    writeFileSync(transcript, ['partial', 'empty'].map(id => JSON.stringify({
      cwd: '/app', type: 'item.completed', item: { id, type: 'command_execution',
        command: 'cat /tools/stack-bench/grader/grade.ts; false', status: 'failed',
        exit_code: 1, aggregated_output: id === 'partial' ? 'private contents' : '' },
    })).join('\n'));
    const audit = auditTranscript(transcript, '/app');
    assert.equal(audit.hits.length, 1);
    assert.equal(audit.refused.length, 1);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('Codex archive copies controller events and excludes native session files', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-codex-archive-'));
  const store = codexTranscriptDirectory(root);
  assert(!existsSync(store));
  try {
    mkdirSync(join(store, 'sessions'), { recursive: true });
    writeFileSync(join(store, 'run.events.jsonl'), '{}\n');
    writeFileSync(join(store, 'sessions', 'agent.events.jsonl'), '{}\n');
    const output = join(root, 'archive');
    assert.deepEqual(transcriptDirectories(root, join(root, 'no-claude')), [store]);
    const result = archiveTranscripts(root, 'attempt', output, join(root, 'no-claude'));
    assert.equal(result.copied, 1);
    assert(existsSync(join(output, 'attempt', 'run.events.jsonl')));
    assert(!existsSync(join(output, 'attempt', 'sessions__agent.events.jsonl')));
  } finally {
    rmSync(store, { recursive: true, force: true });
    rmSync(root, { recursive: true, force: true });
  }
});

test('a completed external Bash read contaminates the transcript', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-leak-audit-'));
  const transcript = join(root, 'session.jsonl');
  try {
    const events = [
      { cwd: '/app', message: { content: [{ type: 'tool_use', id: 'read-1', name: 'Bash',
        input: { command: 'cat /tools/stack-bench/grader/grade.ts; rg secret /outside/notes.md' } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: 'read-1', is_error: false }] } },
      { message: { content: [{ type: 'tool_use', id: 'read-2', name: 'Bash',
        input: { command: 'cat stack-bench/bundle.json' } }] } },
      { message: { content: [{ type: 'tool_result', tool_use_id: 'read-2', is_error: false }] } },
    ];
    writeFileSync(transcript, `${events.map(event => JSON.stringify(event)).join('\n')}\n`);

    const result = auditTranscript(transcript, '/app');

    assert.equal(result.cwd, '/app');
    assert.equal(result.refused.length, 0);
    assert.deepEqual(result.hits.map(hit => ({ path: hit.path, kind: hit.kind })), [
      { path: '/tools/stack-bench/grader/grade.ts', kind: 'GRADER / TEST SPECS' },
      { path: '/outside/notes.md', kind: 'other' },
      { path: '/app/stack-bench/bundle.json', kind: 'GRADER / TEST SPECS' },
    ]);
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

test('a Grok Build stream is audited through its own tool names', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-grok-audit-'));
  const transcript = join(root, '01a112c6-39bd-7d52-a3a9-3158e1b76555.events.jsonl');
  try {
    // The shapes grok 1.0.46 prints with --output-format streaming-messages-json.
    const events = [
      { type: 'system', subtype: 'init', session_id: '01a112c6-39bd-7d52-a3a9-3158e1b76555', cwd: '/app' },
      { type: 'assistant', message: { content: [{ type: 'tool_use', id: 'call_1', name: 'run_terminal_command',
        input: { command: 'cat /tools/stack-bench/grader/grade.ts', description: 'read' } }] } },
      { type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'call_1', content: 'x', is_error: false }] } },
      { type: 'assistant', message: { content: [{ type: 'tool_use', id: 'call_2', name: 'read_file',
        input: { target_file: '/outside/notes.md' } }] } },
      { type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'call_2', content: 'x', is_error: false }] } },
      { type: 'assistant', message: { content: [{ type: 'tool_use', id: 'call_3', name: 'search_replace',
        input: { file_path: 'src/app.ts', old_string: 'a', new_string: 'b' } }] } },
      { type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'call_3', content: 'ok', is_error: false }] } },
      { type: 'assistant', message: { content: [{ type: 'tool_use', id: 'call_4', name: 'list_dir',
        input: { target_directory: '/tools/stack-bench/tracks' } }] } },
      { type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'call_4', content: 'denied', is_error: true }] } },
      { type: 'result', subtype: 'success', is_error: false, session_id: '01a112c6-39bd-7d52-a3a9-3158e1b76555' },
    ];
    writeFileSync(transcript, `${events.map(event => JSON.stringify(event)).join('\n')}\n`);
    const audit = auditTranscript(transcript, 'C:/host/app');
    assert.equal(audit.cwd, '/app');
    assert.deepEqual(audit.hits.map(hit => [hit.path, hit.via]), [
      ['/tools/stack-bench/grader/grade.ts', 'Bash'], ['/outside/notes.md', 'Read']]);
    assert.deepEqual(audit.refused.map(hit => [hit.path, hit.via]), [['/tools/stack-bench/tracks', 'Glob']]);
    assert.equal(audit.fileTool, 3, 'the in-app edit is a file tool use, not a hit');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('an Antigravity stream is audited through its tool steps', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-antigravity-audit-'));
  const id = '86eb1dda-1452-4d14-ab41-9afdc260bdd2';
  const transcript = join(root, `${id}.events.jsonl`);
  // The shapes agy 1.3.2 prints with --output-format stream-json: each tool starts, then ends.
  const step = (index: number, state: string, name: string, parameters: Record<string, string>, output?: string) =>
    ({ event: 'step_update', step_update: { conversation_id: id, step_index: index, state, step_type: 'tool',
      tool_name: name, tool_info: { name, parameters, ...(output === undefined ? {} : { output }) } } });
  try {
    const events = [
      { event: 'init', conversation_id: id, init: { model: 'gemini-3.8-flash', cwd: '/app' } },
      step(2, 'ACTIVE', 'run_command', { CommandLine: 'cat /tools/stack-bench/grader/grade.ts' }),
      step(2, 'DONE', 'run_command', { CommandLine: 'cat /tools/stack-bench/grader/grade.ts' }, 'x'),
      step(3, 'ACTIVE', 'view_file', { AbsolutePath: '/outside/notes.md' }),
      step(3, 'DONE', 'view_file', { AbsolutePath: '/outside/notes.md' }, 'x'),
      step(4, 'ACTIVE', 'write_to_file', { TargetFile: '/app/src/app.ts' }),
      step(4, 'DONE', 'write_to_file', { TargetFile: '/app/src/app.ts' }),
      step(5, 'ACTIVE', 'view_file', { AbsolutePath: '/tools/stack-bench/tracks/spec.md' }),
      step(5, 'ERROR', 'view_file', { AbsolutePath: '/tools/stack-bench/tracks/spec.md' }),
      { event: 'result', result: { conversation_id: id, status: 'SUCCESS' } },
    ];
    writeFileSync(transcript, `${events.map(event => JSON.stringify(event)).join('\n')}\n`);
    const audit = auditTranscript(transcript, 'C:/host/app');
    assert.equal(audit.cwd, '/app');
    assert.deepEqual(audit.hits.map(hit => [hit.path, hit.via]), [
      ['/tools/stack-bench/grader/grade.ts', 'Bash'], ['/outside/notes.md', 'Read']]);
    assert.deepEqual(audit.refused.map(hit => [hit.path, hit.via]), [['/tools/stack-bench/tracks/spec.md', 'Read']]);
    assert.equal(audit.fileTool, 3, 'the in-app write is a file tool use, not a hit');
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('StackBench source online is matched on any branch, by URL, gh and git, and nothing else of SpacetimeDB is', () => {
  const flagged = [
    'curl -sL https://raw.githubusercontent.com/clockworklabs/SpacetimeDB/master/tools/stack-bench/grader/grade.ts',
    'curl https://raw.githubusercontent.com/clockworklabs/SpacetimeDB/refs/heads/bradley/stackbench-test/tools/stack-bench/README.md',
    'wget https://github.com/clockworklabs/SpacetimeDB/blob/bradley/stackbench-test/tools/stack-bench/tracks/x.json',
    'curl "https://api.github.com/repos/clockworklabs/SpacetimeDB/contents/tools%2Fstack-bench?ref=bradley/stackbench-test"',
    'curl https://api.github.com/repos/clockworklabs/spacetimedb/contents/tools/stack-bench/grader?ref=master',
    'curl -L https://github.com/clockworklabs/SpacetimeDB/pull/5766.diff',
    'curl https://github.com/clockworklabs/SpacetimeDB/pull/5766/files',
    'curl https://api.github.com/repos/clockworklabs/SpacetimeDB/pulls/5766/files',
    'curl https://patch-diff.githubusercontent.com/raw/clockworklabs/SpacetimeDB/pull/5766.patch',
    'gh pr diff 5766 --repo clockworklabs/SpacetimeDB',
    'gh api repos/clockworklabs/SpacetimeDB/contents/tools/stack-bench',
    'git fetch https://github.com/clockworklabs/SpacetimeDB pull/5766/head:pr',
    '{"query":"clockworklabs/SpacetimeDB tools/stack-bench grader"}',
    // Mirrors serve the same files from other hosts, so the harness path counts on any host.
    'curl https://example.com/clockworklabs/SpacetimeDB/tools/stack-bench',
    'curl https://cdn.jsdelivr.net/gh/clockworklabs/SpacetimeDB@bradley/stackbench-test/tools/stack-bench/grader/grade.ts',
    'curl https://raw.githack.com/clockworklabs/SpacetimeDB/master/tools/stack-bench/README.md',
    'curl "https://sourcegraph.com/github.com/clockworklabs/SpacetimeDB@master/-/blob/tools/stack-bench/grader/grade.ts"',
    // A search for the pull request.
    '{"query":"clockworklabs SpacetimeDB pull 5766"}',
    '{"query":"SpacetimeDB PR #5766 stack bench"}',
    '{"query":"clockworklabs/SpacetimeDB pull/5766 files"}',
  ];
  for (const command of flagged) assert.equal(stackBenchSourceReferences(command).length > 0, true, command);
  const allowed = [
    'curl -sL https://raw.githubusercontent.com/clockworklabs/SpacetimeDB/master/crates/schema/src/auto_migrate.rs',
    'curl https://github.com/clockworklabs/SpacetimeDB/blob/master/crates/bindings-typescript/README.md',
    'git clone --depth 1 https://github.com/clockworklabs/SpacetimeDB',
    'curl https://github.com/clockworklabs/SpacetimeDB/pull/5767',
    'curl https://github.com/clockworklabs/SpacetimeDB/pull/57660',
    'curl https://cdn.jsdelivr.net/gh/clockworklabs/SpacetimeDB@master/crates/schema/src/auto_migrate.rs',
    '{"query":"clockworklabs SpacetimeDB pull 57660"}',
    'curl https://github.com/clockworklabs/spacetimedb-typescript-sdk/tree/main/tools/stack-bench',
    'curl https://github.com/someone/benchmarks/tree/main/tools/stack-bench',
    'gh pr view 5766 --repo someone/other',
    '{"query":"spacetimedb auto migration add column"}',
  ];
  for (const command of allowed) assert.deepEqual(stackBenchSourceReferences(command), [], command);
});

test('fetching StackBench source contaminates through each agent\'s web tools and shell', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-source-audit-'));
  const raw = 'https://raw.githubusercontent.com/clockworklabs/SpacetimeDB/bradley/stackbench-test/tools/stack-bench/grader/grade.ts';
  const allowed = 'https://raw.githubusercontent.com/clockworklabs/SpacetimeDB/master/crates/schema/src/auto_migrate.rs';
  const use = (id: string, name: string, input: Record<string, unknown>) =>
    ({ cwd: '/app', message: { content: [{ type: 'tool_use', id, name, input }] } });
  const result = (id: string, is_error = false) =>
    ({ message: { content: [{ type: 'tool_result', tool_use_id: id, is_error }] } });
  const write = (name: string, events: unknown[]): string => {
    const file = join(root, name);
    writeFileSync(file, `${events.map(event => JSON.stringify(event)).join('\n')}\n`);
    return file;
  };
  const sources = (file: string) => {
    const audit = auditTranscript(file, '/app');
    const pick = (hits: typeof audit.hits) => hits.filter(hit => hit.kind === STACKBENCH_SOURCE)
      .map(hit => [hit.path, hit.via]);
    return { hits: pick(audit.hits), refused: pick(audit.refused), all: audit.hits };
  };
  try {
    // Claude Code and Grok Build record tool calls as message content.
    const claude = sources(write('claude.jsonl', [
      use('f1', 'WebFetch', { url: raw, prompt: 'read it' }), result('f1'),
      use('f2', 'WebFetch', { url: allowed, prompt: 'read it' }), result('f2'),
      use('s1', 'WebSearch', { query: 'spacetimedb auto migrate' }), result('s1'),
      use('b1', 'Bash', { command: 'curl -sL https://github.com/clockworklabs/SpacetimeDB/pull/5766.diff | head' }), result('b1'),
      use('b2', 'Bash', { command: `curl -sL ${raw}` }), result('b2', true),
      use('b3', 'Bash', { command: 'cat vendor/SpacetimeDB/tools/stack-bench/grader/grade.ts' }), result('b3'),
    ]));
    assert.deepEqual(claude.hits, [[raw, 'WebFetch network attempt'],
      ['https://github.com/clockworklabs/SpacetimeDB/pull/5766.diff', 'Bash network attempt']]);
    assert.deepEqual(claude.refused, [[raw, 'Bash network attempt']]);
    // A local copy of the harness is never the app's own file.
    assert.deepEqual(claude.all.filter(hit => hit.kind === 'GRADER / TEST SPECS').map(hit => hit.path),
      ['/app/vendor/spacetimedb/tools/stack-bench/grader/grade.ts']);

    const grok = sources(write('grok.events.jsonl', [
      use('g1', 'web_fetch', { url: raw }), result('g1'),
      use('g2', 'web_search', { query: 'spacetimedb typescript procedures' }), result('g2'),
    ]));
    assert.deepEqual(grok.hits, [[raw, 'web_fetch network attempt']]);

    // Codex records its hosted search, and each page it opens, as an item.
    const codex = sources(write('codex.events.jsonl', [
      { type: 'stack_bench_context', cwd: '/app' },
      { type: 'item.started', item: { id: 'w1', type: 'web_search', query: '' } },
      { type: 'item.completed', item: { id: 'w1', type: 'web_search', query: raw,
        action: { type: 'open_page', url: raw } } },
      { type: 'item.completed', item: { id: 'w2', type: 'web_search', query: 'spacetimedb views',
        action: { type: 'search', query: 'spacetimedb views' } } },
    ]));
    assert.deepEqual(codex.hits, [[raw, 'WebSearch network attempt']]);

    // Antigravity reports each tool as a step that starts, then ends.
    const id = '86eb1dda-1452-4d14-ab41-9afdc260bdd2';
    const step = (index: number, state: string, name: string, parameters: Record<string, string>) =>
      ({ event: 'step_update', step_update: { conversation_id: id, step_index: index, state, step_type: 'tool',
        tool_name: name, tool_info: { name, parameters, ...(state === 'DONE' ? { output: 'x' } : {}) } } });
    const antigravity = sources(write(`${id}.events.jsonl`, [
      { event: 'init', conversation_id: id, init: { cwd: '/app' } },
      step(1, 'ACTIVE', 'read_url_content', { Url: raw }), step(1, 'DONE', 'read_url_content', { Url: raw }),
      step(2, 'ACTIVE', 'search_web', { query: 'spacetimedb auto migrate' }),
      step(2, 'DONE', 'search_web', { query: 'spacetimedb auto migrate' }),
    ]));
    assert.deepEqual(antigravity.hits, [[raw, 'WebFetch network attempt']]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
