import assert from 'node:assert/strict';
import test from 'node:test';

import { createModularRecipeTaskRequest, resolveModularRecipeSelection,
  resolveModularRecipeTaskRequest } from '../recipe-selection.mjs';

const module = (id, moduleType, requiresPacks = []) => ({
  id, version: '1.0.0', moduleType, requiresPacks,
});
const check = (packId, suffix, observations, requiresFeatures) => ({
  stableKey: `${packId}.${suffix}`, packId, points: 1,
  ...(observations ? { observations } : {}),
  ...(requiresFeatures ? { requiresFeatures } : {}),
});
const release = {
  id: 'example.modular', version: '1.0.0',
  contentSha256: 'a'.repeat(64),
  components: { packs: [
    module('example.accounts', 'feature'),
    module('example.cart', 'feature', ['example.accounts@1.0.0']),
    module('example.durability', 'specification'),
    module('example.concurrency', 'specification'),
  ] },
  checkCatalog: [
    check('example.accounts', 'works'),
    check('example.cart', 'works'),
    check('example.durability', 'survives', ['probe', 'requested'], ['example.cart']),
    check('example.concurrency', 'safe', ['probe', 'requested'], ['example.cart']),
  ],
};

const plan = { recipe: { task: {
  requirements: [
    { id: 'frame', owners: ['recipe'], text: 'Build this product.\n' },
    { id: 'accounts', owners: ['example.accounts'], text: 'Support accounts.\n' },
    { id: 'cart', owners: ['example.cart'], text: 'Support a cart.\n' },
    { id: 'durability', owners: ['example.durability'], text: 'Survive restarts.\n' },
    { id: 'concurrency', owners: ['example.concurrency'], text: 'Handle races.\n' },
  ],
  contracts: [
    { id: 'cart-hook', owners: ['example.cart'], text: 'Expose a cart button.\n' },
    { id: 'probe-hook', owners: ['example.concurrency'], text: 'DO NOT LEAK.\n' },
  ],
} } };

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
  assert.throws(() => resolveModularRecipeSelection(release, {
    featureIds: ['example.accounts'],
    disclosedSpecifications: ['example.durability@1.0.0'],
  }), /has no requested observation/);
  const featureAddingSpec = structuredClone(release);
  featureAddingSpec.components.packs.find(item => item.id === 'example.durability')
    .requiresPacks = ['example.cart@1.0.0'];
  assert.throws(() => resolveModularRecipeSelection(featureAddingSpec, {
    featureIds: ['example.accounts'],
    disclosedSpecifications: ['example.durability@1.0.0'],
  }), /cannot add feature example.cart@1.0.0/);
});

test('requested check filters cannot reach hidden probe scope', () => {
  assert.throws(() => resolveModularRecipeSelection(release, {
    probedSpecifications: ['example.concurrency@1.0.0'],
    checkKeys: ['example.concurrency.safe'],
  }), /outside the disclosed/);
});

test('modular task composition includes disclosed specs and excludes hidden probes', () => {
  const binding = { release, plan };
  const compiled = createModularRecipeTaskRequest(binding, {
    featureIds: ['example.cart'],
    disclosedSpecifications: ['example.durability@1.0.0'],
    probedSpecifications: ['example.concurrency@1.0.0'],
  });
  assert.equal(compiled.task.requirementText,
    'Build this product.\nSupport accounts.\nSupport a cart.\nSurvive restarts.\n');
  assert.equal(compiled.task.contractText, 'Expose a cart button.\n');
  assert.equal(compiled.task.requirementText.includes('Handle races'), false);
  assert.equal(compiled.task.contractText.includes('DO NOT LEAK'), false);
  assert.deepEqual(compiled.request.selection.probeChecks, ['example.concurrency.safe']);
  assert.deepEqual(resolveModularRecipeTaskRequest(binding, compiled.request).request,
    compiled.request);

  const withoutProbe = createModularRecipeTaskRequest(binding, {
    featureIds: ['example.cart'],
    disclosedSpecifications: ['example.durability@1.0.0'],
  });
  assert.equal(withoutProbe.task.requirementSha256, compiled.task.requirementSha256);
  assert.equal(withoutProbe.task.contractSha256, compiled.task.contractSha256);
  assert.equal(withoutProbe.task.sha256, compiled.task.sha256);
  assert.notEqual(withoutProbe.selection.sha256, compiled.selection.sha256);
});
