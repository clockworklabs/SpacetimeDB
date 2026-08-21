import assert from 'node:assert/strict';
import test from 'node:test';

import { mergeMutationShards, mutationShard, mutationWorkerSlots }
  from '../src/evidence/mutation-shards.mjs';

const mutations = ['a', 'b', 'c', 'd', 'e'].map(id => ({ id }));

test('mutation workers reserve one consecutive proven run slot each', () => {
  assert.deepEqual(mutationWorkerSlots({ workerCount: 4, runIndex: 8, maxRunIndex: 20 }),
    [8, 9, 10, 11]);
  assert.throws(() => mutationWorkerSlots({ workerCount: 2, runIndex: 20, maxRunIndex: 20 }),
    /exceed run-index cap/);
  for (const workerCount of [0, 1.5, NaN]) {
    assert.throws(() => mutationWorkerSlots({ workerCount, runIndex: 0, maxRunIndex: 20 }));
  }
});

test('mutation partitioning is stable and permits empty trailing shards', () => {
  assert.deepEqual(mutationShard(mutations, { index: 0, count: 3 }).mutationIds, ['a', 'd']);
  assert.deepEqual(mutationShard(mutations, { index: 1, count: 3 }).mutationIds, ['b', 'e']);
  assert.deepEqual(mutationShard(mutations, { index: 2, count: 3 }).mutationIds, ['c']);
  assert.deepEqual(mutationShard([{ id: 'a' }], { index: 2, count: 3 }).mutationIds, []);
});

test('mutation shard merging restores manifest order and rejects incomplete unions', () => {
  const shards = Array.from({ length: 3 }, (_, index) => {
    const plan = mutationShard(mutations, { index, count: 3 });
    return { ...plan, results: plan.mutationIds.map(id => ({ id, status: 'CAUGHT' })) };
  });
  assert.deepEqual(mergeMutationShards(mutations, [shards[2], shards[0], shards[1]])
    .map(result => result.id), ['a', 'b', 'c', 'd', 'e']);
  assert.throws(() => mergeMutationShards(mutations, shards.slice(1)), /declared shard count/);
  assert.throws(() => mergeMutationShards(mutations, [shards[0], shards[0], shards[2]]),
    /duplicated/);
  assert.throws(() => mergeMutationShards(mutations, [
    { ...shards[0], mutationIds: ['a'] }, shards[1], shards[2],
  ]), /exact assigned/);
});

test('mutation shard inputs reject duplicate and malformed mutation identities', () => {
  assert.throws(() => mutationShard([{ id: 'a' }, { id: 'a' }], { index: 0, count: 1 }),
    /unique/);
  assert.throws(() => mutationShard([{ nope: true }], { index: 0, count: 1 }), /has no id/);
  assert.throws(() => mutationShard(mutations, { index: 3, count: 3 }), /0 through 2/);
});
