import assert from 'node:assert/strict';
import { mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { createBackendLease, publicBackendLease } from '../src/runtime/backend-lease.js';
import { resourceLockDescriptors, resourceLockTransaction } from '../src/runtime/resource-lock-worker.js';
import { MAX_RUNNER_CAPACITY } from '../src/composition/product-config.js';

test('pooled standalone admission shares campaign capacity and preserves fixed port claims', () => {
  const root = mkdtempSync(join(tmpdir(), 'lock-capacity-'));
  const campaign = createBackendLease({ runId: 'campaign', backend: 'stub', track: 'loop', runIndex: 0 });
  const standalone = createBackendLease({ runId: 'standalone', backend: 'stub', track: 'loop', runIndex: 1 });
  const third = createBackendLease({ runId: 'third', backend: 'stub', track: 'loop', runIndex: 2 });
  const workspaceKey = `workspace:${join(root, 'app')}`;
  const campaignKeys = ['capacity:runner:0', 'port:4000', workspaceKey];
  const keys = ['capacity:runner:0', 'port:4001'];
  try {
    const reserved = resourceLockTransaction({ root, lease: campaign, keys: campaignKeys, operation: 'acquire' });
    const before = reserved.map(lock => readFileSync(lock.path, 'utf8'));
    assert.throws(() => resourceLockTransaction({ root, lease: standalone,
      keys: campaignKeys, capacity: 2, operation: 'acquire' }), /port:4000.*already leased/);
    assert.equal(readdirSync(root).length, 3, 'port conflict must not acquire the free capacity slot');
    assert.throws(() => resourceLockTransaction({ root, lease: standalone,
      keys: [...keys, workspaceKey], capacity: 2, operation: 'acquire' }), /workspace:.*already leased/);
    assert.equal(readdirSync(root).length, 3, 'workspace conflict must not acquire capacity or ports');

    const acquired = resourceLockTransaction({ root, lease: standalone, keys, capacity: 2, operation: 'acquire' });
    assert.deepEqual(acquired.map(lock => lock.key), ['capacity:runner:1', 'port:4001']);
    assert.deepEqual(resourceLockTransaction({ root, lease: standalone, keys, capacity: 2,
      operation: 'acquire' }), acquired, 'the same owner reuses its selected capacity');
    assert.throws(() => resourceLockTransaction({ root, lease: third,
      keys: ['capacity:runner:0', 'port:4002'], capacity: 2, operation: 'acquire' }), /all 2.*slots are leased/);
    assert.equal(readdirSync(root).length, 5);

    // Crash after acquisition but before final lease write: intent includes all
    // candidates, and recovery must leave the campaign's claims untouched.
    const intent = resourceLockDescriptors(root, keys, 2);
    assert.deepEqual(intent.map(lock => lock.key), ['capacity:runner:0', 'capacity:runner:1', 'port:4001']);
    resourceLockTransaction({ root, lease: standalone, keys: intent.map(lock => lock.key), operation: 'release-intent' });
    assert.deepEqual(reserved.map(lock => readFileSync(lock.path, 'utf8')), before);
    const reused = resourceLockTransaction({ root, lease: third, keys, capacity: 2, operation: 'acquire' });
    assert.deepEqual(reused.map(lock => lock.key), acquired.map(lock => lock.key));
    resourceLockTransaction({ root, lease: third, keys: reused.map(lock => lock.key), operation: 'release' });
    resourceLockTransaction({ root, lease: campaign, keys: campaignKeys, operation: 'release' });
    assert.equal(readdirSync(root).length, 0);
    for (const capacity of [0, 1.5, MAX_RUNNER_CAPACITY + 1]) {
      assert.throws(() => resourceLockDescriptors(root, keys, capacity), /outside the declared runner pool/);
    }
    assert.throws(() => resourceLockDescriptors(root, ['port:4000'], 2), /requires one capacity key/);
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
