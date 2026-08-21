import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileRecipeFile } from '../src/composition/composition-compiler.mjs';
import { buildRecipeRelease, executionPlanForRelease } from '../src/composition/recipe-release.mjs';
import { resolveModularRecipeSelection } from '../src/composition/recipe-selection.mjs';

const TRACK = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const recipe = name => join(TRACK, 'composition', 'recipes', name);
const L1 = recipe('l1-modular-2.4.0.json');
const PREVIOUS = recipe('l2-standard-1.4.0.json');
const CURRENT = recipe('l2-standard-1.5.0.json');
const l2Prefixes = [
  'ecommerce.operations-access.',
  'ecommerce.inventory-operations.',
  'ecommerce.returns-pricing.',
];
const isL2 = check => l2Prefixes.some(prefix => check.stableKey.startsWith(prefix));
const identity = check => [check.stableKey, check.points];

test('L2 1.5 inherits exact L1 2.4 and preserves the qualified L2 contract', () => {
  const base = compileRecipeFile(L1, { trackRoot: TRACK });
  const previous = compileRecipeFile(PREVIOUS, { trackRoot: TRACK });
  const current = compileRecipeFile(CURRENT, { trackRoot: TRACK });

  assert.deepEqual(current.recipe.task.baseRecipe, {
    id: 'ecommerce.l1-modular', version: '2.4.0', path: 'recipes/l1-modular-2.4.0.json',
  });
  assert.deepEqual(current.checks.slice(0, base.checks.length), base.checks);

  const previousL2 = previous.checks.filter(isL2).map(identity).sort();
  const currentL2 = current.checks.filter(isL2).map(identity).sort();
  assert.deepEqual(currentL2, previousL2);
  assert.deepEqual({
    checks: current.checks.length,
    points: current.scoring.points,
    l1Checks: base.checks.length,
    l1Points: base.scoring.points,
    l2Checks: currentL2.length,
    l2Points: current.checks.filter(isL2).reduce((sum, check) => sum + check.points, 0),
  }, {
    checks: 76, points: 117, l1Checks: 48, l1Points: 58, l2Checks: 28, l2Points: 59,
  });

  assert.deepEqual(current.checks.filter(check => check.points === 0)
    .map(check => check.stableKey).sort(), [
    'ecommerce.spec.concurrency-safety.restock-race.202-control',
    'ecommerce.spec.external-data-sync.external-stock.901b',
  ]);
});

test('L2 1.5 carries the exact L1 2.4 pack and scenario identities', () => {
  const basePlan = compileRecipeFile(L1, { trackRoot: TRACK });
  const currentPlan = compileRecipeFile(CURRENT, { trackRoot: TRACK });
  const baseRelease = buildRecipeRelease(L1, { trackRoot: TRACK });
  const currentRelease = buildRecipeRelease(CURRENT, { trackRoot: TRACK });

  const basePacks = new Map(baseRelease.components.packs.map(pack => [pack.id, pack]));
  for (const [id, basePack] of basePacks) {
    assert.deepEqual(currentRelease.components.packs.find(pack => pack.id === id), basePack,
      `${id} must retain its exact L1 2.4 version and content hash`);
  }

  assert.deepEqual(currentPlan.execution.slice(0, basePlan.execution.length)
    .map(execution => ({
      id: execution.id.replace(/@L1$/, ''),
      source: execution.source,
    })), basePlan.execution.map(execution => ({ id: execution.id, source: execution.source })));
  assert.deepEqual(currentRelease.task.baseRecipe, {
    id: baseRelease.id,
    version: baseRelease.version,
    track: baseRelease.track,
    state: baseRelease.state,
    recipeReleaseSchemaVersion: baseRelease.recipeReleaseSchemaVersion,
    contentSha256: baseRelease.contentSha256,
    executionSha256: baseRelease.executionSha256,
    meaningSha256: baseRelease.meaningSha256,
    sourceManifestSha256: baseRelease.sourceManifestSha256,
  });
});

test('typed ownership marks exact L1 executions inherited and all L2 executions current', () => {
  const plan = compileRecipeFile(CURRENT, { trackRoot: TRACK });
  const ownership = executionPlanForRelease(CURRENT, { trackRoot: TRACK, level: 2 });
  const byId = new Map(ownership.map(execution => [execution.id, execution]));
  assert.equal(ownership.length, plan.execution.length);

  for (const execution of plan.execution) {
    const owned = byId.get(execution.id);
    assert(owned, execution.id);
    if (execution.source.startsWith('scenarios/01-')) {
      assert.deepEqual(owned.ownership, { kind: 'inherited', fromLevel: 1 }, execution.id);
    } else {
      assert.deepEqual(owned.ownership, { kind: 'current', level: 2 }, execution.id);
    }
  }
  assert.equal(ownership.filter(item => item.ownership.kind === 'inherited').length, 31);
  assert.equal(ownership.filter(item => item.ownership.kind === 'current').length, 10);
});

test('L2 1.5 remains modular and never selects checks without their feature modules', () => {
  const release = buildRecipeRelease(CURRENT, { trackRoot: TRACK });
  const operationsOnly = resolveModularRecipeSelection(release, {
    featureIds: ['ecommerce.operations-access-features'],
    expectedSpecifications: ['ecommerce.operations-access-specifications@1.0.0'],
  });
  const operationKeys = operationsOnly.scoredChecks.map(check => check.stableKey);
  assert(operationKeys.includes('ecommerce.operations-access.operator-authorization.201c'));
  assert(operationKeys.includes('ecommerce.operations-access.order-owner.204a'));
  assert(!operationKeys.includes('ecommerce.operations-access.operator-authorization.201a'));
  assert(!operationKeys.includes('ecommerce.operations-access.operator-authorization.201b'));

  const inheritedCart = resolveModularRecipeSelection(release, {
    featureIds: ['ecommerce.feature.cart-checkout'],
    expectedSpecifications: [
      'ecommerce.spec.access-control@1.2.0',
      'ecommerce.spec.state-durability@1.1.0',
      'ecommerce.spec.live-state@1.2.0',
    ],
  });
  const cartKeys = inheritedCart.scoredChecks.map(check => check.stableKey);
  for (const stableKey of [
    'ecommerce.feature.cart-checkout.cart.4a',
    'ecommerce.feature.cart-checkout.cart.4d',
    'ecommerce.spec.state-durability.cart-reload.4b',
    'ecommerce.spec.live-state.shared-cart.4c',
    'ecommerce.spec.access-control.cart-boundary.109a',
    'ecommerce.spec.access-control.cart-boundary.109b',
  ]) assert(cartKeys.includes(stableKey), stableKey);
  assert(!cartKeys.some(stableKey => l2Prefixes.some(prefix => stableKey.startsWith(prefix))),
    'an inherited L1-only selection must not pull in L2 checks');
});
