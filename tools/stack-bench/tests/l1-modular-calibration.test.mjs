import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileCalibrationFile } from '../src/composition/calibration-compiler.mjs';
import { loadReferenceRegistry, validateReferenceRegistry } from '../src/references/reference-fixtures.mjs';
import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';

const BENCH = join(import.meta.dirname, '..');
const TRACK = join(BENCH, 'tracks', 'ecommerce');
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'l1-modular-2.3.0.json'));
const calibration = compileCalibrationFile('composition/calibrations/l1-modular-2.3.0.json', {
  trackRoot: TRACK,
  stackBenchRoot: BENCH,
  release,
});

test('the L1 modular calibration is qualified by the complete live evidence set', () => {
  assert.equal(release.state, 'qualified');
  assert.equal(calibration.state, 'qualified');
  assert.equal(calibration.promotion.status, 'promoted');
  assert.equal(calibration.qualification.evidence.length, 13);
  assert.equal(new Set(calibration.qualification.evidence.map(entry => entry.path)).size, 7);
  assert.deepEqual(calibration.qualification.stacks, [
    { id: 'mongodb', status: 'qualified' },
    { id: 'postgres', status: 'qualified' },
    { id: 'spacetime', status: 'qualified' },
  ]);
  assert.equal(calibration.references.entries.every(reference =>
    reference.status === 'active' && reference.targetPath === undefined), true);
  assert.equal(calibration.mutations.every(mutation => mutation.status === 'active'), true);
});

test('the active registry owns one exact L1 2.3 mutation set per backend', () => {
  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry).issues, []);
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const fixture = registry.fixtures.find(candidate =>
      candidate.id === `ecommerce-l1-direct-actions-${backend}`);
    assert(fixture.recipes.includes('ecommerce.l1-modular@2.3.0'));
    assert.deepEqual(fixture.mutationManifests,
      [`grader/mutations/${backend}-ecom-l1-modular-2.3.0.json`]);
  }
});

test('the comprehensive release has scored defects and only two supporting controls', () => {
  const mutations = new Map(calibration.mutations.map(entry => [entry.backend,
    new Map(entry.targets.map(target => [target.id, target.stableKeys]))]));
  const quantityKey = 'ecommerce.spec.concurrency-safety.duplicate-checkout.203a';
  assert.equal(release.checkCatalog.find(check => check.stableKey === quantityKey).points, 1);
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
  assert.deepEqual(mutations.get('mongodb').get('external-stock-polling-disabled'),
    ['ecommerce.spec.external-data-sync.external-stock.901a']);
  assert.deepEqual(mutations.get('postgres').get('reconnect-does-not-send-current-catalog'),
    ['ecommerce.spec.external-data-sync.external-stock.901d']);
  assert.deepEqual(mutations.get('mongodb').get('reconnect-generation-ignores-current-catalog'),
    ['ecommerce.spec.external-data-sync.external-stock.901d']);
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    assert.equal([...mutations.get(backend).values()].filter(keys =>
      keys.includes('ecommerce.spec.live-state.open-list.902a')).length, 2);
  }
  assert.deepEqual(calibration.controls.map(control => [control.stableKey, control.role]), [
    ['ecommerce.spec.concurrency-safety.restock-race.202-control', 'precondition'],
    ['ecommerce.spec.external-data-sync.external-stock.901b', 'precondition'],
  ]);
});
