import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { gradeDatabaseLease, parseGradeArgs } from '../grader/grade.js';
import { parseMutationArgs, remainingMutationBatchMs } from '../grader/mutation-test.js';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { createDatabaseWriteCapability } from '../src/actions/runtime-action-executors.js';
import { attemptDatabaseIdentity } from '../src/stacks/hosted-database-identity.js';

test('the grader preserves private MongoDB authority through its stock-write caller', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-grade-lease-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const lease = createBackendLease({ runId: 'grade-mongo', backend: 'mongodb',
    track: 'ecommerce', runIndex: 0, database: 'bench',
    container: { name: 'leased-mongodb', id: 'a'.repeat(64) } });
  lease.state = 'active';
  lease.resources.network = { name: 'leased-network', id: 'b'.repeat(64),
    namespaceContainerId: lease.resources.container!.id, hostAddresses: [], services: [],
    firewallSha256: null, firewallInstalledAt: null };
  const path = join(root, 'lease.json');
  writeBackendLease(path, lease);
  const env = { STACK_BENCH_LEASE: path, STACK_BENCH_LEASE_TOKEN: lease.ownershipToken };
  const databaseLease = gradeDatabaseLease('mongodb', env);
  assert.deepEqual(databaseLease?.resources.network, lease.resources.network);
  const calls: Array<readonly string[]> = [];
  const writer = createDatabaseWriteCapability({ backend: 'mongodb', databaseLease,
    expand: value => value, exec: (_command, args) => {
      calls.push(args);
      return args[0] === 'inspect' ? lease.resources.container!.id : 'OK\n';
    } });
  writer.setStock({ item: 'Desk Lamp', warehouse: 'East', quantity: 5, settleMs: 0 });
  const argv = calls.find(args => args.includes('mongosh'))!;
  const identity = attemptDatabaseIdentity(lease.ownershipToken);
  assert.deepEqual(argv.slice(0, 10), ['exec', lease.resources.container!.id,
    'mongosh', 'bench', '--username', identity.user, '--password', identity.password,
    '--authenticationDatabase', 'bench']);
  assert.throws(() => gradeDatabaseLease('mongodb', { ...env, STACK_BENCH_LEASE_TOKEN: 'wrong' }),
    /token/);
  assert.throws(() => gradeDatabaseLease('postgres', env), /backend/);
  assert.equal(gradeDatabaseLease('mongodb', {}), null);
});

test('grader arguments require an explicit scenario and valid numeric selectors', () => {
  assert.throws(() => parseGradeArgs(['node', 'grade', '--url', 'http://localhost:1']),
    /--spec/);
  assert.throws(() => parseGradeArgs(['node', 'grade', '--url', 'file:///tmp/app',
    '--spec', 'scenario.json']), /HTTP or HTTPS/);
  assert.throws(() => parseGradeArgs(['node', 'grade', '--url', 'http://localhost:1',
    '--spec', 'scenario.json', '--level', '0']), /positive integer/);
  assert.equal(parseGradeArgs(['node', 'grade', '--url', 'http://localhost:1',
    '--spec', 'scenario.json', '--level', '2']).level, 2);
  assert.equal(parseGradeArgs(['node', 'grade', '--url', 'http://localhost:1',
    '--spec', 'scenario.json', '--browser-ws-endpoint', 'ws://127.0.0.1:9000/session'])
    .browserWsEndpoint, 'ws://127.0.0.1:9000/session');
  assert.throws(() => parseGradeArgs(['node', 'grade', '--url', 'http://localhost:1',
    '--spec', 'scenario.json', '--browser-ws-endpoint', 'http://127.0.0.1:9000']),
  /must use ws or wss/);
});

test('mutation arguments fail before execution when the batch bounds are invalid', () => {
  const base = ['node', 'mutation-test', '--app', 'app', '--url', 'http://localhost:1',
    '--mutations', 'mutations.json', '--level', '3', '--recipe', 'recipe@1.0.0'];
  assert.throws(() => parseMutationArgs([...base, '--mutation-shard-index', '0']),
    /must be supplied together/);
  assert.throws(() => parseMutationArgs([...base, '--max-runtime-minutes', '0']),
    /from 1 through 120/);
  assert.equal(parseMutationArgs([...base, '--max-runtime-minutes', '30']).maxRuntimeMinutes, 30);
});

test('mutation operations use only the remaining batch time', () => {
  assert.equal(remainingMutationBatchMs(10_000, 2_500), 7_500);
  assert.throws(() => remainingMutationBatchMs(10_000, 10_000),
    /deadline reached/);
});
