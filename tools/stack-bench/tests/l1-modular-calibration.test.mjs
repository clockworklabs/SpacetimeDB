import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileCalibrationFile } from '../src/composition/calibration-compiler.mjs';
import { loadReferenceRegistry, validateReferenceRegistry } from '../src/references/reference-fixtures.mjs';
import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';

const BENCH = join(import.meta.dirname, '..');
const TRACK = join(BENCH, 'tracks', 'ecommerce');
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'l1-modular-2.4.0.json'));
const calibration = compileCalibrationFile('composition/calibrations/l1-modular-2.4.0.json', {
  trackRoot: TRACK,
  stackBenchRoot: BENCH,
  release,
});

test('the L1 modular calibration is qualified by the complete live evidence set', () => {
  assert.equal(release.state, 'qualified');
  assert.equal(calibration.state, 'qualified');
  assert.equal(calibration.promotion.status, 'promoted');
  assert.equal(calibration.qualification.evidence.length, 7);
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

test('the active registry owns one exact L1 2.4 mutation set per backend', () => {
  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry).issues, []);
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const fixture = registry.fixtures.find(candidate =>
      candidate.id === `ecommerce-l1-action-inputs-2.4-${backend}`);
    assert(fixture.recipes.includes('ecommerce.l1-modular@2.4.0'));
    assert.deepEqual(fixture.mutationManifests,
      [`grader/mutations/${backend}-ecom-l1-modular-2.4.0.json`]);
  }
});

test('the comprehensive release has scored defects and only two supporting controls', () => {
  const scoredKeys = release.checkCatalog.filter(check => check.points > 0)
    .map(check => check.stableKey).sort();
  assert.equal(release.checkCatalog.length, 48);
  assert.equal(release.scoring.points, 58);
  assert.equal(scoredKeys.length, 46);
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const mutation = calibration.mutations.find(entry => entry.backend === backend);
    assert.deepEqual([...new Set(mutation.targets.flatMap(target => target.stableKeys))]
      .filter(key => scoredKeys.includes(key)).sort(), scoredKeys);
  }
  assert.deepEqual(calibration.controls.map(control => [control.stableKey, control.role]), [
    ['ecommerce.spec.concurrency-safety.restock-race.202-control', 'precondition'],
    ['ecommerce.spec.external-data-sync.external-stock.901b', 'precondition'],
  ]);
});
