import assert from 'node:assert/strict';
import test from 'node:test';

import { resolveModularRecipeSelection } from '../recipe-selection.mjs';

const module = (id, moduleType, requiresPacks = []) => ({
  id, version: '1.0.0', moduleType, requiresPacks,
});
const check = (packId, suffix, observations) => ({
  stableKey: `${packId}.${suffix}`, packId, points: 1,
  ...(observations ? { observations } : {}),
});
const release = {
  contentSha256: 'a'.repeat(64),
  components: { packs: [
    module('example.accounts', 'feature'),
    module('example.cart', 'feature', ['example.accounts@1.0.0']),
    module('example.durability', 'specification', ['example.cart@1.0.0']),
    module('example.concurrency', 'specification', ['example.cart@1.0.0']),
  ] },
  checkCatalog: [
    check('example.accounts', 'works'),
    check('example.cart', 'works'),
    check('example.durability', 'survives', ['probe', 'requested']),
    check('example.concurrency', 'safe', ['probe', 'requested']),
  ],
};

test('feature, disclosed specification, and hidden probe selections stay independent', () => {
  const selection = resolveModularRecipeSelection(release, {
    featureIds: ['example.cart'],
    disclosedSpecifications: ['example.durability@1.0.0'],
    probedSpecifications: ['example.concurrency@1.0.0'],
  });
  assert.deepEqual(selection.features, ['example.accounts', 'example.cart']);
  assert.deepEqual(selection.specifications, {
    disclosed: ['example.durability@1.0.0'],
    probed: ['example.concurrency@1.0.0'],
  });
  assert.deepEqual(selection.taskPacks,
    ['example.accounts', 'example.cart', 'example.durability']);
  assert.deepEqual(selection.requestedChecks.map(item => item.stableKey), [
    'example.accounts.works', 'example.cart.works', 'example.durability.survives',
  ]);
  assert.deepEqual(selection.probeChecks.map(item => item.stableKey),
    ['example.concurrency.safe']);
  assert.match(selection.sha256, /^[a-f0-9]{64}$/);
});

test('modular selection rejects overlap, wrong module kinds, and probe-ineligible checks', () => {
  assert.throws(() => resolveModularRecipeSelection(release, {
    disclosedSpecifications: ['example.durability@1.0.0'],
    probedSpecifications: ['example.durability@1.0.0'],
  }), /both disclosed and probed/);
  assert.throws(() => resolveModularRecipeSelection(release, {
    disclosedSpecifications: ['example.cart@1.0.0'],
  }), /no disclosed specification/);
  const noProbe = structuredClone(release);
  noProbe.checkCatalog.find(item => item.packId === 'example.concurrency').observations = ['requested'];
  assert.throws(() => resolveModularRecipeSelection(noProbe, {
    probedSpecifications: ['example.concurrency@1.0.0'],
  }), /has no probe observation/);
  const noRequested = structuredClone(release);
  noRequested.checkCatalog.find(item => item.packId === 'example.durability').observations = ['probe'];
  assert.throws(() => resolveModularRecipeSelection(noRequested, {
    disclosedSpecifications: ['example.durability@1.0.0'],
  }), /has no requested observation/);
});

test('requested check filters cannot reach hidden probe scope', () => {
  assert.throws(() => resolveModularRecipeSelection(release, {
    probedSpecifications: ['example.concurrency@1.0.0'],
    checkKeys: ['example.concurrency.safe'],
  }), /outside the disclosed/);
});
