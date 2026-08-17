import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compileRecipeFile } from '../composition-compiler.mjs';
import { buildRecipeRelease } from '../recipe-release.mjs';
import { createAgentVisibleTaskRequest, createBoundRecipeTaskRequest, createModularRecipeTaskRequest,
  resolveBoundRecipeTaskRequest, resolveModularRecipeSelection, selectScenarioChecks }
  from '../recipe-selection.mjs';

const recipePath = join(import.meta.dirname, '..', 'tracks', 'ecommerce', 'composition',
  'recipes', 'l1-modular-2.0.0.json');
const plan = compileRecipeFile(recipePath);
const release = buildRecipeRelease(recipePath);
const binding = { plan, release };
const hardenedRecipePath = join(import.meta.dirname, '..', 'tracks', 'ecommerce', 'composition',
  'recipes', 'l1-modular-2.1.0.json');
const hardenedPlan = compileRecipeFile(hardenedRecipePath);
const hardenedRelease = buildRecipeRelease(hardenedRecipePath);
const hardenedBinding = { plan: hardenedPlan, release: hardenedRelease };
const contentionPath = join(import.meta.dirname, '..', 'tracks', 'ecommerce', 'scenarios',
  '01-duplicate-checkout-2.0.0.json');
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

  const fullyRequested = resolveModularRecipeSelection(release, {
    requestedSpecifications: specifications,
  });
  assert.equal(fullyRequested.scoredChecks.length, 39);
  assert.equal(fullyRequested.scoredChecks.every(check => check.points > 0), true);
  assert.equal(fullyRequested.scoredPoints, 51);
  assert.equal(fullyRequested.observedChecks.length, 0);
});

test('the generic task boundary dispatches and replays the modular schema', () => {
  const options = {
    featureIds: ['ecommerce.feature.accounts'],
    observedSpecifications: ['ecommerce.spec.state-durability@1.0.0'],
  };
  const task = createBoundRecipeTaskRequest(binding, options);
  assert.equal(task.request.schemaVersion, 3);
  assert.deepEqual(resolveBoundRecipeTaskRequest(binding, task.request).request, task.request);
  assert.throws(() => resolveBoundRecipeTaskRequest(binding,
    { ...task.request, schemaVersion: 1 }), /requires a schema-3/);
});

test('the coding process receives no controller-owned expected or observed selection', () => {
  const selected = createBoundRecipeTaskRequest(binding, {
    expectedSpecifications: ['ecommerce.spec.access-control@1.0.0'],
    observedSpecifications: ['ecommerce.spec.state-durability@1.0.0'],
  });
  const visible = createAgentVisibleTaskRequest(binding, selected);
  assert.deepEqual(visible.selection.requested.specifications,
    { requested: [], expected: [], observed: [] });
  assert.deepEqual(visible.selection.observedChecks, []);
  assert.equal(visible.task.sha256, selected.request.task.sha256);
  assert.doesNotMatch(JSON.stringify(visible), /state-durability|access-control/);
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
  assert.equal(reviews.selection.scoredPoints, 10);
  assert.match(reviews.task.requirementText, /## Reviews/);
  assert.doesNotMatch(reviews.task.requirementText, /## Cart and checkout/);
  assert.doesNotMatch(reviews.task.requirementText, /## Warehouse administration/);
});

test('expected specifications change the score without changing prompt bytes', () => {
  const expected = specifications.slice(0, -1);
  const productionExpectation = createModularRecipeTaskRequest(binding, {
    expectedSpecifications: expected,
  });
  const featureOnly = createModularRecipeTaskRequest(binding);

  assert.equal(productionExpectation.selection.scoredPoints > featureOnly.selection.scoredPoints, true);
  assert.equal(productionExpectation.task.requirementText, featureOnly.task.requirementText);
  assert.equal(productionExpectation.task.contractText, featureOnly.task.contractText);
  assert.equal(productionExpectation.task.sha256, featureOnly.task.sha256);
  assert.notEqual(productionExpectation.selection.sha256, featureOnly.selection.sha256);
  assert.equal(productionExpectation.selection.scoredChecks
    .some(check => check.treatment === 'expected'), true);
  for (const heading of ['Access control', 'State durability', 'Live state',
    'Concurrency safety', 'Transactional integrity']) {
    assert.equal(productionExpectation.task.requirementText.includes(`## ${heading}`), false);
  }
  assert.doesNotMatch(productionExpectation.task.contractText,
    /server-enforced|without a reload|survives|negative|source of truth/i);
  assert.doesNotMatch(productionExpectation.task.requirementText, /\|\n##/);
});

test('a requested specification applies only to the selected feature surface', () => {
  const accounts = createModularRecipeTaskRequest(binding, {
    featureIds: ['ecommerce.feature.accounts'],
    requestedSpecifications: ['ecommerce.spec.state-durability@1.0.0'],
  });
  assert.deepEqual(accounts.selection.features, ['ecommerce.feature.accounts']);
  assert.equal(accounts.selection.scoredPoints, 5);
  assert.deepEqual(accounts.selection.scoredChecks
    .filter(check => check.packId === 'ecommerce.spec.state-durability')
    .map(check => check.criterionId), ['1e']);
  assert.match(accounts.task.requirementText, /## State durability/);
  assert.doesNotMatch(accounts.task.requirementText, /## Catalog/);
  assert.doesNotMatch(accounts.task.requirementText, /cart|orders|reviews|administrator/i);
});

test('an unmentioned expectation that needs a prescribed schema fails closed', () => {
  assert.throws(() => resolveModularRecipeSelection(release, {
    expectedSpecifications: ['ecommerce.spec.external-data-sync@1.0.0'],
  }), /has no unmentioned observation/);
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

test('the scored duplicate-checkout check owns all state needed when selected alone', () => {
  const key = 'ecommerce.spec.concurrency-safety.duplicate-checkout.203b';
  const selected = selectScenarioChecks(JSON.parse(readFileSync(contentionPath, 'utf8')),
    { checks: release.checkCatalog }, [key]);

  assert.deepEqual(selected.features[0].criteria.map(criterion => criterion.id), ['203b']);
  assert.deepEqual(selected.features[0].setup.slice(-2).map(step => step.do),
    ['clickConcurrently', 'click']);
  assert.equal(selected.features[0].setup.at(-1).testid, 'cart-toggle');
});

test('the hardened modular recipe gives selected features direct-call inputs and focused checks', () => {
  const selected = createModularRecipeTaskRequest(hardenedBinding, {
    featureIds: ['ecommerce.feature.purchasing', 'ecommerce.feature.warehouse-admin'],
    expectedSpecifications: [
      'ecommerce.spec.access-control@1.1.0',
      'ecommerce.spec.transactional-integrity@1.1.0',
    ],
  });
  assert.match(selected.task.contractText, /data-buy-input/);
  assert.match(selected.task.contractText, /data-restock-input/);
  assert.doesNotMatch(selected.task.requirementText, /server-enforced authority/);
  assert.equal(selected.selection.scoredChecks
    .filter(check => ['101a', '102a', '103a', '104a'].includes(String(check.criterionId)))
    .every(check => check.executionId === 'server-actions'), true);
  assert.equal(hardenedRelease.checkCatalog.length, release.checkCatalog.length);
  assert.equal(hardenedPlan.scoring.points, plan.scoring.points);
});
