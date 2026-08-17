import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileCalibrationFile } from '../calibration-compiler.mjs';
import { loadReferenceRegistry, validateReferenceRegistry } from '../reference-fixtures.mjs';
import { buildRecipeRelease } from '../recipe-release.mjs';

const BENCH = join(import.meta.dirname, '..');
const TRACK = join(BENCH, 'tracks', 'ecommerce');
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'l1-modular-2.2.0.json'));
const calibration = compileCalibrationFile('composition/calibrations/l1-modular-2.2.0.json', {
  trackRoot: TRACK,
  stackBenchRoot: BENCH,
  release,
});

test('the L1 modular calibration is a candidate with no invented live evidence', () => {
  assert.equal(release.state, 'draft');
  assert.equal(calibration.state, 'draft');
  assert.equal(calibration.promotion.status, 'candidate');
  assert.deepEqual(calibration.qualification.evidence, []);
  assert.deepEqual(calibration.qualification.stacks, [
    { id: 'mongodb', status: 'candidate' },
    { id: 'postgres', status: 'candidate' },
    { id: 'spacetime', status: 'candidate' },
  ]);
  assert.equal(calibration.references.entries.every(reference =>
    reference.status === 'candidate' && reference.targetPath === undefined), true);
  assert.equal(calibration.mutations.every(mutation => mutation.status === 'candidate'), true);
});

test('the candidate registry owns one exact L1 2.2 mutation set per backend', () => {
  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry).issues, []);
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const fixture = registry.fixtures.find(candidate =>
      candidate.id === `ecommerce-l1-direct-actions-${backend}`);
    assert(fixture.recipes.includes('ecommerce.l1-modular@2.2.0'));
    assert.deepEqual(fixture.mutationManifests,
      [`grader/mutations/candidates/${backend}-ecom-l1-modular-2.2.0.json`]);
  }
});

test('the candidate carries proven defect shapes forward and adds exact 203a targets', () => {
  const mutations = new Map(calibration.mutations.map(entry => [entry.backend,
    new Map(entry.targets.map(target => [target.id, target.stableKeys]))]));
  const quantityKey = 'ecommerce.spec.concurrency-safety.duplicate-checkout.203a';
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    assert.deepEqual(mutations.get(backend).get('existing-cart-line-does-not-increment'),
      [quantityKey]);
  }
  assert.deepEqual(mutations.get('mongodb').get('oversell-unguarded-decrement'), [
    'ecommerce.spec.concurrency-safety.last-unit.201a',
    'ecommerce.spec.concurrency-safety.last-unit.201b',
    'ecommerce.spec.concurrency-safety.last-unit.201c',
  ]);
  assert.deepEqual(mutations.get('postgres').get('checkout-does-not-empty-cart'),
    ['ecommerce.spec.concurrency-safety.duplicate-checkout.203b']);
  assert.deepEqual(mutations.get('spacetime').get('purchase-does-not-reserve-stock-restock-race'),
    ['ecommerce.spec.concurrency-safety.restock-race.202a']);

  const quantityControl = calibration.controls.find(control =>
    control.stableKey === quantityKey);
  assert.deepEqual(quantityControl.mutationTargets, [
    'mongodb:existing-cart-line-does-not-increment',
    'postgres:existing-cart-line-does-not-increment',
    'spacetime:existing-cart-line-does-not-increment',
  ]);
  const revenueControl = calibration.controls.find(control =>
    control.stableKey === 'ecommerce.spec.concurrency-safety.last-unit.201c');
  assert.deepEqual(revenueControl.mutationTargets, [
    'mongodb:oversell-unguarded-decrement',
    'postgres:oversell-no-row-lock',
    'spacetime:purchase-does-not-reserve-stock-last-unit',
  ]);
});
