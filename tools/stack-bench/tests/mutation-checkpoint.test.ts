import assert from 'node:assert/strict';
import test from 'node:test';

import { reusableMutationEvidence } from '../src/evidence/mutation-checkpoint.js';
import type { MutationCheckpointEvidence, MutationCheckpointIdentity }
  from '../src/evidence/mutation-checkpoint.js';

function identity(): MutationCheckpointIdentity {
  return {
    schemaVersion: 1,
    engineSha256: 'a'.repeat(64),
    recipeSha256: 'b'.repeat(64),
    fixtureSha256: 'c'.repeat(64),
    calibrationSha256: '8'.repeat(64),
    imageId: 'sha256:image',
    backend: 'mongodb',
    track: 'ecommerce',
    level: 3,
    trackSha256: '9'.repeat(64),
    shard: { index: 0, count: 1, mutationIds: ['m1', 'm2'] },
    groups: [
      { scenario: 'first.json', identitySha256: 'd'.repeat(64), mutationIds: ['m1'] },
      { scenario: 'second.json', identitySha256: 'e'.repeat(64), mutationIds: ['m2'] },
    ],
  };
}

function evidence(checkpoint: MutationCheckpointIdentity = identity()): MutationCheckpointEvidence {
  return { checkpoint, results: [
    { id: 'm1', scenario: 'first.json', status: 'CAUGHT' },
    { id: 'm2', scenario: 'second.json', status: 'CAUGHT' },
  ], baseline: { scenarios: [
    { scenario: 'first.json', identitySha256: 'd'.repeat(64), total: 1, max: 1 },
    { scenario: 'second.json', identitySha256: 'e'.repeat(64), total: 1, max: 1 },
  ] } };
}

test('mutation checkpoints reuse only unchanged scenario groups', () => {
  const current = identity();
  current.groups[1]!.identitySha256 = 'f'.repeat(64);
  const reusable = reusableMutationEvidence(evidence(), current);
  assert.deepEqual(reusable.results.map(result => result.id), ['m1']);
  assert.deepEqual(reusable.baselines.map(result => result.scenario), ['first.json']);
});

test('mutation checkpoints fail closed when a global identity changes', () => {
  const fields = ['engineSha256', 'recipeSha256', 'fixtureSha256',
    'calibrationSha256', 'imageId', 'backend', 'track', 'level', 'trackSha256'] as const;
  for (const field of fields) {
    const current = identity();
    current[field] = `changed-${field}`;
    assert.throws(() => reusableMutationEvidence(evidence(), current),
      new RegExp(`checkpoint ${field} does not match`));
  }
  const current = identity();
  current.shard = { index: 0, count: 2, mutationIds: ['m1'] };
  assert.throws(() => reusableMutationEvidence(evidence(), current),
    /checkpoint shard does not match/);
});
