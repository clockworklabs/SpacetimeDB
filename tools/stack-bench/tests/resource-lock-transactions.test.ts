import assert from 'node:assert/strict';
import { mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { createBackendLease, publicBackendLease } from '../src/runtime/backend-lease.js';
import { resourceLockDescriptors, resourceLockTransaction } from '../src/runtime/resource-lock-worker.js';

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
