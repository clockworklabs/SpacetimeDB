import assert from 'node:assert/strict';
import { mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { createBackendLease, publicBackendLease, runnerCapacity, claimBackendResources,
  claimBackendResourcesWhenAvailable, releaseResourceLocks } from '../src/runtime/backend-lease.js';
import { hostResourceWaitReason, resourceLockDescriptors, resourceLockTransaction } from '../src/runtime/resource-lock-worker.js';
import { attemptContainerLimitTotals } from '../src/composition/product-config.js';

test('dynamic startup reservation includes optional provider caps once per worker across backends', () => {
  const root = mkdtempSync(join(tmpdir(), 'host-auth-capacity-'));
  const first = createBackendLease({ runId: 'first', backend: 'stub', track: 'loop', runIndex: 0 });
  const next = createBackendLease({ runId: 'next', backend: 'stub', track: 'loop', runIndex: 1 });
  const start = Date.now();
  const keys = ['slot:loop:postgres:run0', 'slot:loop:mongodb:run0', 'slot:loop:spacetime:run0'];
  const withProvider = attemptContainerLimitTotals('keycloak').memoryBytes;
  const withoutProvider = attemptContainerLimitTotals().memoryBytes;
  try {
    const request = { root, lease: first, keys, operation: 'acquire' as const, capacity: null,
      authenticationProvider: 'keycloak' as const };
    const locks = resourceLockTransaction(request, (count, memory) => {
      assert.equal(count, 1); assert.equal(memory, withProvider); return null;
    }, start);
    assert.equal(JSON.parse(readFileSync(locks[0]!.path, 'utf8')).authenticationProvider, 'keycloak');
    assert.equal(JSON.parse(readFileSync(locks[0]!.path, 'utf8')).startupMemoryBytes, withProvider);
    assert.throws(() => resourceLockTransaction({ ...request, authenticationProvider: undefined }),
      /different authentication provider/);
    resourceLockTransaction({ root, lease: next, keys: ['slot:loop:postgres:run1', 'port:5001'],
      operation: 'acquire', capacity: null }, (count, memory) => {
        assert.equal(count, 2); assert.equal(memory, withProvider + withoutProvider); return null;
      }, start + 1);
    const later = createBackendLease({ runId: 'later', backend: 'stub', track: 'loop', runIndex: 2 });
    resourceLockTransaction({ root, lease: later, keys: ['slot:loop:postgres:run2'],
      operation: 'acquire', capacity: null }, (count, memory) => {
        assert.equal(count, 1); assert.equal(memory, withoutProvider); return null;
      }, start + 60_002);
    // The same host can admit the old envelope and defer the provider envelope.
    assert.equal(hostResourceWaitReason(32 * 1024 ** 3, 13 * 1024 ** 3, 8, 0), null);
    assert.match(hostResourceWaitReason(32 * 1024 ** 3, 13 * 1024 ** 3, 8, 0, 1, withProvider)!,
      /GiB required/);
    resourceLockTransaction({ ...request, operation: 'release', authenticationProvider: undefined });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('host admission counts a campaign reservation once per index across backends', () => {
  const root = mkdtempSync(join(tmpdir(), 'host-capacity-'));
  const first = createBackendLease({ runId: 'first', backend: 'stub', track: 'loop', runIndex: 0 });
  const next = createBackendLease({ runId: 'next', backend: 'stub', track: 'loop', runIndex: 1 });
  const keys = ['slot:loop:postgres:run0', 'slot:loop:mongodb:run0', 'slot:loop:spacetime:run0'];
  try {
    resourceLockTransaction({ root, lease: first, keys, operation: 'acquire', capacity: 1 });
    resourceLockTransaction({ root, lease: first, keys, operation: 'acquire', capacity: 1 });
    assert.throws(() => resourceLockTransaction({ root, lease: next,
      keys: ['slot:loop:postgres:run1', 'port:5999'], operation: 'acquire', capacity: 1 }), /host capacity unavailable/);
    assert.equal(readdirSync(root).length, 3, 'no partial claim when host capacity is exhausted');
    resourceLockTransaction({ root, lease: first, keys, operation: 'release' });
    resourceLockTransaction({ root, lease: next, keys: ['slot:loop:postgres:run1'], operation: 'acquire', capacity: 1 });
    assert.equal(runnerCapacity({}), null);
    assert.equal(runnerCapacity({ STACK_BENCH_RUNNER_CAPACITY: '9' }), 9);
    assert.equal(runnerCapacity({ STACK_BENCH_RUNNER_CAPACITY: 'dynamic' }), null);
    assert.throws(() => runnerCapacity({ STACK_BENCH_RUNNER_CAPACITY: '0' }), /positive safe integer/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('dynamic admission uses host pressure and retains exclusive resource ownership without a count quota', () => {
  const root = mkdtempSync(join(tmpdir(), 'host-unlimited-'));
  try {
    const capacity = runnerCapacity({ STACK_BENCH_RUNNER_CAPACITY: 'dynamic' });
    assert.equal(hostResourceWaitReason(46 * 1024 ** 3, 29 * 1024 ** 3, 32, 7), null);
    assert.match(hostResourceWaitReason(46 * 1024 ** 3, 3 * 1024 ** 3, 32, 7)!, /GiB available/);
    assert.match(hostResourceWaitReason(46 * 1024 ** 3, 29 * 1024 ** 3, 32, 33)!, /CPU load/);
    assert.match(hostResourceWaitReason(46 * 1024 ** 3, 29 * 1024 ** 3, 32, 7, 4)!, /GiB required/);
    assert.throws(() => hostResourceWaitReason(NaN, 0, 32, 0), /valid host resource/);
    const start = Date.now();
    for (let index = 0; index < 12; index += 1) {
      const lease = createBackendLease({ runId: `run-${index}`, backend: 'stub', track: 'loop', runIndex: index });
      resourceLockTransaction({ root, lease, capacity, operation: 'acquire',
        keys: [`slot:loop:stub:run${index}`, `port:${5000 + index}`] }, count => {
          assert.equal(count, 1, 'old live claims do not impose a fixed count limit');
          return null;
        }, start + index * 60_001);
    }
    const extra = createBackendLease({ runId: 'extra', backend: 'stub', track: 'loop', runIndex: 12 });
    assert.throws(() => resourceLockTransaction({ root, lease: extra, capacity,
      operation: 'acquire', keys: ['port:5000'] }), /already leased/);
    assert.equal(readdirSync(root).length, 24);
    const request = { root, lease: extra, capacity, operation: 'acquire' as const,
      keys: ['slot:loop:stub:run12', 'port:5012'] };
    assert.throws(() => resourceLockTransaction(request, () => 'host capacity unavailable: memory pressure'),
      /memory pressure/);
    assert.equal(readdirSync(root).length, 24, 'pressure cannot leave partial claims');
    resourceLockTransaction(request, count => { assert.equal(count, 2); return null; }, start + 11 * 60_001);
    resourceLockTransaction(request, () => { throw new Error('existing claim must not reacquire capacity'); });
    resourceLockTransaction({ ...request, operation: 'release' }, () => { throw new Error('cleanup must stay available'); });
    assert.equal(readdirSync(root).length, 24);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('port and workspace claims remain exclusive across acquisition and intent recovery', () => {
  const root = mkdtempSync(join(tmpdir(), 'lock-resources-'));
  const campaign = createBackendLease({ runId: 'campaign', backend: 'stub', track: 'loop', runIndex: 0 });
  const standalone = createBackendLease({ runId: 'standalone', backend: 'stub', track: 'loop', runIndex: 1 });
  const workspaceKey = `workspace:${join(root, 'app')}`;
  const campaignKeys = ['port:4000', workspaceKey];
  const keys = ['port:4001'];
  try {
    const reserved = resourceLockTransaction({ root, lease: campaign, keys: campaignKeys, operation: 'acquire' });
    const before = reserved.map(lock => readFileSync(lock.path, 'utf8'));
    for (const conflict of ['port:4000', workspaceKey]) {
      assert.throws(() => resourceLockTransaction({ root, lease: standalone,
        keys: [...keys, conflict], operation: 'acquire' }), /already leased/);
      assert.equal(readdirSync(root).length, 2, 'conflict must not acquire the free port');
    }
    const acquired = resourceLockTransaction({ root, lease: standalone, keys, operation: 'acquire' });
    assert.deepEqual(acquired.map(lock => lock.key), keys);
    assert.deepEqual(resourceLockTransaction({ root, lease: standalone, keys, operation: 'acquire' }), acquired);
    const intent = resourceLockDescriptors(root, [...keys, ...campaignKeys]);
    resourceLockTransaction({ root, lease: standalone, keys: intent.map(lock => lock.key), operation: 'release-intent' });
    assert.deepEqual(reserved.map(lock => readFileSync(lock.path, 'utf8')), before);
    resourceLockTransaction({ root, lease: campaign, keys, operation: 'acquire' });
    resourceLockTransaction({ root, lease: campaign, keys: [...keys, ...campaignKeys], operation: 'release' });
    assert.equal(readdirSync(root).length, 0);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a stale release cannot remove a replacement claim, even temporarily', () => {
  const root = mkdtempSync(join(tmpdir(), 'lock-transaction-'));
  const first = createBackendLease({ runId: 'first', backend: 'stub', track: 'loop', runIndex: 0 });
  const next = createBackendLease({ runId: 'next', backend: 'stub', track: 'loop', runIndex: 0 });
  const transaction = { root, keys: ['slot'], lease: first };
  try {
    resourceLockTransaction({ ...transaction, operation: 'acquire' });
    resourceLockTransaction({ ...transaction, operation: 'release' });
    const [lock] = resourceLockTransaction({ ...transaction, lease: next, operation: 'acquire' });
    assert(lock);
    const before = readFileSync(lock.path, 'utf8');
    assert.throws(() => resourceLockTransaction({ ...transaction, operation: 'release' }),
      /no longer belongs/);
    assert.equal(readFileSync(lock.path, 'utf8'), before);
    assert.throws(() => resourceLockTransaction({ ...transaction, operation: 'acquire' }),
      /already leased by next/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('partial private intent releases only matching claims and leaves foreign owners intact', () => {
  const root = mkdtempSync(join(tmpdir(), 'lock-intent-'));
  const first = createBackendLease({ runId: 'first', backend: 'stub', track: 'loop', runIndex: 0 });
  const other = createBackendLease({ runId: 'other', backend: 'stub', track: 'loop', runIndex: 0 });
  try {
    resourceLockTransaction({ root, lease: first, keys: ['a'], operation: 'acquire' });
    resourceLockTransaction({ root, lease: other, keys: ['b'], operation: 'acquire' });
    resourceLockTransaction({ root, lease: first, keys: ['a', 'b', 'c'], operation: 'release-intent' });
    resourceLockTransaction({ root, lease: other, keys: ['b'], operation: 'verify' });
    resourceLockTransaction({ root, lease: other, keys: ['a', 'c'], operation: 'acquire' });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a failed multi-key claim creates no partial exclusion', () => {
  const root = mkdtempSync(join(tmpdir(), 'lock-set-'));
  const first = createBackendLease({ runId: 'first', backend: 'stub', track: 'loop', runIndex: 0 });
  const other = createBackendLease({ runId: 'other', backend: 'stub', track: 'loop', runIndex: 0 });
  try {
    resourceLockTransaction({ root, lease: first, keys: ['b'], operation: 'acquire' });
    assert.throws(() => resourceLockTransaction({ root, lease: other, keys: ['a', 'b'], operation: 'acquire' }),
      /already leased/);
    resourceLockTransaction({ root, lease: first, keys: ['a'], operation: 'acquire' });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('public lease evidence does not disclose private campaign delegation', () => {
  const lease = createBackendLease({ runId: 'child', backend: 'stub', track: 'loop', runIndex: 0 });
  lease.campaignDelegation = { path: '/private/delegation.json', token: 'private-child-token' };
  assert.equal('campaignDelegation' in publicBackendLease(lease), false);
});

test('standalone admission waits for capacity but rejects ownership conflicts', { skip: process.platform !== 'linux' }, async () => {
  const root = mkdtempSync(join(tmpdir(), 'standalone-capacity-'));
  const first = createBackendLease({ runId: 'first', backend: 'stub', track: 'loop', runIndex: 0 });
  const next = createBackendLease({ runId: 'next', backend: 'stub', track: 'loop', runIndex: 1 });
  const firstPath = join(root, 'first.json'), nextPath = join(root, 'next.json');
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    claimBackendResources(firstPath, first, { root, keys: ['slot:loop:postgres:run0', 'port:5999'], capacity: 1 });
    await assert.rejects(claimBackendResourcesWhenAvailable(nextPath, next,
      { root, keys: ['port:5999'], capacity: 2 }), /already leased/);
    timer = setTimeout(() => releaseResourceLocks(first), 25);
    await claimBackendResourcesWhenAvailable(nextPath, next,
      { root, keys: ['slot:loop:postgres:run1'], capacity: 1 });
    assert.deepEqual(next.resources.locks.map(lock => lock.key), ['slot:loop:postgres:run1']);
  } finally {
    clearTimeout(timer);
    releaseResourceLocks(first);
    releaseResourceLocks(next);
    rmSync(root, { recursive: true, force: true });
  }
});
