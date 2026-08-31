import assert from 'node:assert/strict';
import test from 'node:test';

import { createModularRecipeTaskRequest, resolveModularRecipeSelection,
  resolveModularRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import type { ModularRecipePack, ModularRecipeRelease, ModularRecipeTaskBinding,
  RecipeTaskPlan } from '../src/composition/recipe-selection.js';
import type { RecipeCheck } from '../src/composition/recipe-release.js';

const module = (id: string, moduleType: 'feature' | 'specification', requiresPacks: string[] = []):
  ModularRecipePack => ({
  id, version: '1.0.0', moduleType, requiresPacks,
});
const check = (packId: string, suffix: string, observations?: string[],
  requiresFeatures?: string[]): RecipeCheck => ({
  stableKey: `${packId}.${suffix}`, packId, executionId: packId, points: 1,
  ...(observations ? { observations } : {}),
  ...(requiresFeatures ? { requiresFeatures } : {}),
});
const release: ModularRecipeRelease = {
  id: 'example.modular', version: '1.0.0',
  contentSha256: 'a'.repeat(64),
  components: { packs: [
    module('example.accounts', 'feature'),
    module('example.cart', 'feature', ['example.accounts@1.0.0']),
    module('example.durability', 'specification'),
    module('example.concurrency', 'specification'),
    module('example.reconnect', 'specification'),
  ] },
  checkCatalog: [
    check('example.accounts', 'works'),
    check('example.cart', 'works'),
    check('example.durability', 'survives', ['requested', 'unmentioned'], ['example.cart']),
    check('example.concurrency', 'safe', ['requested', 'unmentioned'], ['example.cart']),
    check('example.reconnect', 'catches-up', ['requested', 'unmentioned'], ['example.cart']),
  ],
};

const plan: RecipeTaskPlan = { packs: release.components.packs, recipe: { task: {
  mode: 'fresh',
  requirements: [
    { id: 'frame', owners: ['recipe'], modes: ['fresh'], text: 'Build this product.\n' },
    { id: 'accounts', owners: ['example.accounts'], modes: ['fresh'], text: 'Support accounts.\n' },
    { id: 'cart', owners: ['example.cart'], modes: ['fresh'], text: 'Support a cart.\n' },
    { id: 'durability', owners: ['example.durability'], modes: ['fresh'], text: 'Survive restarts.\n' },
    { id: 'concurrency', owners: ['example.concurrency'], modes: ['fresh'], text: 'Handle races.\n' },
    { id: 'reconnect', owners: ['example.reconnect'], modes: ['fresh'], text: 'Catch up after reconnect.\n' },
  ],
  contracts: [
    { id: 'cart-hook', owners: ['example.cart'], modes: ['fresh'], text: 'Expose a cart button.\n' },
    { id: 'probe-hook', owners: ['example.concurrency'], modes: ['fresh'], text: 'DO NOT LEAK.\n' },
  ],
} } };
const binding: ModularRecipeTaskBinding = { release, plan };

test('requested, expected, and observed specification treatments stay independent', () => {
  const selection = resolveModularRecipeSelection(release, {
    featureIds: ['example.cart'],
    requestedSpecifications: ['example.durability@1.0.0'],
    expectedSpecifications: ['example.concurrency@1.0.0'],
    observedSpecifications: ['example.reconnect@1.0.0'],
  });
  assert.deepEqual(selection.features, ['example.accounts', 'example.cart']);
  assert.deepEqual(selection.specifications, {
    requested: ['example.durability@1.0.0'],
    expected: ['example.concurrency@1.0.0'],
    observed: ['example.reconnect@1.0.0'],
  });
  assert.deepEqual(selection.promptPacks,
    ['example.accounts', 'example.cart', 'example.durability']);
  assert.deepEqual(selection.scoredChecks.map(item => item.stableKey), [
    'example.accounts.works', 'example.cart.works', 'example.durability.survives',
    'example.concurrency.safe',
  ]);
  const expectedCheck = selection.scoredChecks.find(item =>
    item.stableKey === 'example.concurrency.safe');
  assert(expectedCheck);
  assert.equal(expectedCheck.treatment, 'expected');
  assert.deepEqual(selection.observedChecks.map(item => item.stableKey),
    ['example.reconnect.catches-up']);
  assert.match(selection.sha256, /^[a-f0-9]{64}$/);
});

test('modular selection rejects overlap, wrong module kinds, and unobservable checks', () => {
  assert.throws(() => resolveModularRecipeSelection(release, {
    requestedSpecifications: ['example.durability@1.0.0'],
    expectedSpecifications: ['example.durability@1.0.0'],
  }), /both requested and expected/);
  assert.throws(() => resolveModularRecipeSelection(release, {
    requestedSpecifications: ['example.cart@1.0.0'],
  }), /no requested specification/);
  const noProbe = structuredClone(release);
  const noProbeCheck = noProbe.checkCatalog.find(item => item.packId === 'example.concurrency');
  assert(noProbeCheck);
  noProbeCheck.observations = ['requested'];
  assert.throws(() => resolveModularRecipeSelection(noProbe, {
    expectedSpecifications: ['example.concurrency@1.0.0'],
  }), /has no evaluation without prompting/);
  const noRequested = structuredClone(release);
  const noRequestedCheck = noRequested.checkCatalog.find(item => item.packId === 'example.durability');
  assert(noRequestedCheck);
  noRequestedCheck.observations = ['unmentioned'];
  assert.throws(() => resolveModularRecipeSelection(noRequested, {
    requestedSpecifications: ['example.durability@1.0.0'],
  }), /has no prompted evaluation/);
  assert.throws(() => resolveModularRecipeSelection(release, {
    featureIds: ['example.accounts'],
    requestedSpecifications: ['example.durability@1.0.0'],
  }), /has no prompted evaluation/);
  const featureAddingSpec = structuredClone(release);
  const durabilityPack = featureAddingSpec.components.packs.find(item => item.id === 'example.durability');
  assert(durabilityPack);
  durabilityPack.requiresPacks = ['example.cart@1.0.0'];
  assert.throws(() => resolveModularRecipeSelection(featureAddingSpec, {
    featureIds: ['example.accounts'],
    requestedSpecifications: ['example.durability@1.0.0'],
  }), /cannot add feature example.cart@1.0.0/);
});

test('scored check filters can select expected checks but cannot reach observed-only scope', () => {
  const expected = resolveModularRecipeSelection(release, {
    expectedSpecifications: ['example.concurrency@1.0.0'],
    checkKeys: ['example.concurrency.safe'],
  });
  assert.deepEqual(expected.scoredChecks.map(check => check.stableKey),
    ['example.concurrency.safe']);
  assert.throws(() => resolveModularRecipeSelection(release, {
    observedSpecifications: ['example.reconnect@1.0.0'],
    checkKeys: ['example.reconnect.catches-up'],
  }), /outside the requested\/expected/);
});

test('modular task composition includes requested specs and withholds expected and observed specs', () => {
  const compiled = createModularRecipeTaskRequest(binding, {
    featureIds: ['example.cart'],
    requestedSpecifications: ['example.durability@1.0.0'],
    expectedSpecifications: ['example.concurrency@1.0.0'],
    observedSpecifications: ['example.reconnect@1.0.0'],
  });
  assert.equal(compiled.task.requirementText,
    'Build this product.\n\nSupport accounts.\n\nSupport a cart.\n\nSurvive restarts.\n');
  assert.equal(compiled.task.contractText, 'Expose a cart button.\n');
  assert.equal(compiled.task.requirementText.includes('Handle races'), false);
  assert.equal(compiled.task.requirementText.includes('Catch up after reconnect'), false);
  assert.equal(compiled.task.contractText.includes('DO NOT LEAK'), false);
  assert.deepEqual(compiled.request.selection.scoredChecks, [
    'example.accounts.works', 'example.cart.works', 'example.durability.survives',
    'example.concurrency.safe',
  ]);
  assert.deepEqual(compiled.request.selection.observedChecks, ['example.reconnect.catches-up']);
  assert.deepEqual(resolveModularRecipeTaskRequest(binding, compiled.request).request,
    compiled.request);

  const requestedOnly = createModularRecipeTaskRequest(binding, {
    featureIds: ['example.cart'],
    requestedSpecifications: ['example.durability@1.0.0'],
  });
  assert.equal(requestedOnly.task.requirementSha256, compiled.task.requirementSha256);
  assert.equal(requestedOnly.task.contractSha256, compiled.task.contractSha256);
  assert.equal(requestedOnly.task.sha256, compiled.task.sha256);
  assert.notEqual(requestedOnly.selection.sha256, compiled.selection.sha256);
});
