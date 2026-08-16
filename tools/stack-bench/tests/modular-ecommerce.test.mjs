import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { compileRecipeFile } from '../composition-compiler.mjs';
import { buildRecipeRelease } from '../recipe-release.mjs';
import { createBoundRecipeTaskRequest, createModularRecipeTaskRequest,
  resolveBoundRecipeTaskRequest, resolveModularRecipeSelection }
  from '../recipe-selection.mjs';

const recipePath = resolve('tools/stack-bench/tracks/ecommerce/composition/recipes/l1-modular-2.0.0.json');
const plan = compileRecipeFile(recipePath);
const release = buildRecipeRelease(recipePath);
const binding = { plan, release };
const specifications = [
  'ecommerce.spec.access-control@1.0.0',
  'ecommerce.spec.state-durability@1.0.0',
  'ecommerce.spec.live-state@1.0.0',
  'ecommerce.spec.concurrency-safety@1.0.0',
  'ecommerce.spec.transactional-integrity@1.0.0',
  'ecommerce.spec.external-data-sync@1.0.0',
];

test('the modular L1 catalog owns every existing criterion exactly once', () => {
  assert.equal(plan.packs.length, 12);
  assert.equal(plan.checks.length, 48);
  assert.equal(plan.scoring.points, 51);
  assert.equal(plan.packs.every(pack => ['feature', 'specification'].includes(pack.moduleType)), true);
  assert.deepEqual(release.task.requirements
    .find(fragment => fragment.id === 'ecommerce.spec.state-durability.account-data')
    .requiresFeatures, ['ecommerce.feature.accounts', 'ecommerce.feature.cart-checkout']);

  const fullyDisclosed = resolveModularRecipeSelection(release, {
    disclosedSpecifications: specifications,
  });
  assert.equal(fullyDisclosed.requestedChecks.length, 48);
  assert.equal(fullyDisclosed.requestedPoints, 51);
  assert.equal(fullyDisclosed.probeChecks.length, 0);
});

test('the generic task boundary dispatches and replays the modular schema', () => {
  const options = {
    featureIds: ['ecommerce.feature.accounts'],
    probedSpecifications: ['ecommerce.spec.state-durability@1.0.0'],
  };
  const task = createBoundRecipeTaskRequest(binding, options);
  assert.equal(task.request.schemaVersion, 2);
  assert.deepEqual(resolveBoundRecipeTaskRequest(binding, task.request).request, task.request);
  assert.throws(() => resolveBoundRecipeTaskRequest(binding,
    { ...task.request, schemaVersion: 1 }), /requires a schema-2/);
});

test('feature dependencies compose a smaller product without unrelated modules', () => {
  const reviews = createModularRecipeTaskRequest(binding, {
    featureIds: ['ecommerce.feature.reviews'],
  });
  assert.deepEqual(reviews.selection.features, [
    'ecommerce.feature.accounts',
    'ecommerce.feature.catalog',
    'ecommerce.feature.purchasing',
    'ecommerce.feature.reviews',
  ]);
  assert.equal(reviews.selection.requestedPoints, 10);
  assert.match(reviews.task.requirementText, /## Reviews/);
  assert.doesNotMatch(reviews.task.requirementText, /## Cart and checkout/);
  assert.doesNotMatch(reviews.task.requirementText, /## Warehouse administration/);
});

test('unmentioned specifications change probe scope without changing prompt bytes', () => {
  const probes = specifications.slice(0, -1);
  const defaultsProbe = createModularRecipeTaskRequest(binding, {
    probedSpecifications: probes,
  });
  const featureOnly = createModularRecipeTaskRequest(binding);

  assert.equal(defaultsProbe.selection.requestedPoints, 14);
  assert.equal(defaultsProbe.task.requirementText, featureOnly.task.requirementText);
  assert.equal(defaultsProbe.task.contractText, featureOnly.task.contractText);
  assert.equal(defaultsProbe.task.sha256, featureOnly.task.sha256);
  assert.notEqual(defaultsProbe.selection.sha256, featureOnly.selection.sha256);
  assert.equal(defaultsProbe.selection.probeChecks.length > 0, true);
  for (const heading of ['Access control', 'State durability', 'Live state',
    'Concurrency safety', 'Transactional integrity']) {
    assert.equal(defaultsProbe.task.requirementText.includes(`## ${heading}`), false);
  }
  assert.doesNotMatch(defaultsProbe.task.contractText,
    /server-enforced|without a reload|survives|negative|source of truth/i);
  assert.doesNotMatch(defaultsProbe.task.requirementText, /\|\n##/);
});

test('a disclosed specification applies only to the selected feature surface', () => {
  const accounts = createModularRecipeTaskRequest(binding, {
    featureIds: ['ecommerce.feature.accounts'],
    disclosedSpecifications: ['ecommerce.spec.state-durability@1.0.0'],
  });
  assert.deepEqual(accounts.selection.features, ['ecommerce.feature.accounts']);
  assert.equal(accounts.selection.requestedPoints, 5);
  assert.deepEqual(accounts.selection.requestedChecks
    .filter(check => check.packId === 'ecommerce.spec.state-durability')
    .map(check => check.criterionId), ['1e']);
  assert.match(accounts.task.requirementText, /## State durability/);
  assert.doesNotMatch(accounts.task.requirementText, /## Catalog/);
  assert.doesNotMatch(accounts.task.requirementText, /cart|orders|reviews|administrator/i);
});

test('a probe that needs a disclosed interoperability schema fails closed', () => {
  assert.throws(() => resolveModularRecipeSelection(release, {
    probedSpecifications: ['ecommerce.spec.external-data-sync@1.0.0'],
  }), /has no probe observation/);
});

test('criteria with ordered state dependencies are not exposed as isolated probes', () => {
  const byKey = new Map(release.checkCatalog.map(check => [check.stableKey, check]));
  for (const key of [
    'ecommerce.spec.live-state.shared-cart.4c',
    'ecommerce.spec.transactional-integrity.unique-review.6b',
    'ecommerce.spec.live-state.rating.6c',
    'ecommerce.spec.live-state.warehouse-stock.7c',
  ]) assert.deepEqual(byKey.get(key).observations, ['requested']);

  assert.deepEqual(release.checkCatalog
    .filter(check => check.checkGroupId === 'account-state-recovery')
    .map(check => check.criterionId), ['105a', '105b']);
  assert.deepEqual(release.checkCatalog
    .filter(check => check.checkGroupId === 'cart-boundary')
    .map(check => check.criterionId), ['109a', '109b']);
});
