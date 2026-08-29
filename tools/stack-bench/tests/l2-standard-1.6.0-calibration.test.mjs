import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { qualificationReadiness } from '../commands/qualification-cli.mjs';
import { compileCalibrationFile } from '../dist/src/composition/calibration-compiler.js';
import { compilePromotionFile } from '../src/composition/composition-compiler.js';
import { buildRecipeRelease } from '../dist/src/composition/recipe-release.js';
import { loadReferenceRegistry, validateReferenceRegistry } from '../src/references/reference-fixtures.mjs';

const BENCH = join(import.meta.dirname, '..');
const TRACK_ROOT = join(BENCH, 'tracks', 'ecommerce');
const RECIPE = join(TRACK_ROOT, 'composition', 'recipes', 'l2-standard-1.6.0.json');
const release = buildRecipeRelease(RECIPE, { trackRoot: TRACK_ROOT });
const calibration = compileCalibrationFile('composition/calibrations/l2-standard-1.6.0.json', {
  trackRoot: TRACK_ROOT,
  stackBenchRoot: BENCH,
  release,
});

test('L2 1.6 calibration binds every scored check to an exact defect per backend', () => {
  const scoredKeys = release.checkCatalog.filter(check => check.points > 0)
    .map(check => check.stableKey).sort();
  assert.equal(scoredKeys.length, 74);

  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const mutation = calibration.mutations.find(entry => entry.backend === backend);
    assert(mutation, `${backend} must have an exact mutation manifest`);
    const covered = [...new Set(mutation.targets.flatMap(target => target.stableKeys))]
      .filter(stableKey => scoredKeys.includes(stableKey)).sort();
    assert.deepEqual(covered, scoredKeys, `${backend} must cover all 74 scored stable keys`);
  }

  assert.deepEqual(calibration.controls.map(control => [control.stableKey, control.role]), [
    ['ecommerce.spec.concurrency-safety.restock-race.202-control', 'precondition'],
    ['ecommerce.spec.external-data-sync.external-stock.901b', 'precondition'],
  ]);
});

test('L2 1.6 remains available while current qualification is pending', () => {
  assert.equal(release.state, 'qualified');
  assert.equal(calibration.state, 'draft');
  assert.equal(calibration.promotion.status, 'candidate');
  assert.deepEqual(calibration.qualification.evidence, []);
  assert.deepEqual(calibration.qualification.stacks, [
    { id: 'mongodb', status: 'candidate' },
    { id: 'postgres', status: 'candidate' },
    { id: 'spacetime', status: 'candidate' },
  ]);
  assert(calibration.references.entries.every(reference => reference.status === 'active'));
  assert(calibration.mutations.every(mutation => mutation.status === 'active'));

  assert.equal(release.version, '1.6.0');
  assert.equal(release.task.mode, 'upgrade');
  assert.deepEqual([release.task.baseRecipe.id, release.task.baseRecipe.version],
    ['ecommerce.l1-modular', '2.5.0']);
});

test('L2 qualification waits for a promoted L1 baseline', () => {
  assert.throws(() => qualificationReadiness('ecommerce', 2,
    'ecommerce.l2-standard@1.6.0'), /requires exactly one promoted L1 base; found 0/);
});

test('the catalogs and registry retain L2 1.6 as an exact candidate', () => {
  const catalog = compilePromotionFile(join(TRACK_ROOT, 'composition', 'candidates.json'), {
    trackRoot: TRACK_ROOT,
  });
  assert.deepEqual(catalog.entries.filter(entry => entry.alias === 'L2'
    && entry.recipe.id === 'ecommerce.l2-standard'), []);
  const promotions = compilePromotionFile(join(TRACK_ROOT, 'composition', 'promotions.json'), {
    trackRoot: TRACK_ROOT,
  });
  assert.deepEqual(promotions.entries.filter(entry => entry.alias === 'L2'
    && entry.status === 'candidate'), [{
    alias: 'L2',
    status: 'candidate',
    recipe: {
      path: 'recipes/l2-standard-1.6.0.json',
      id: 'ecommerce.l2-standard',
      version: '1.6.0',
    },
  }]);

  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry).issues, []);
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const fixture = registry.fixtures.find(entry =>
      entry.id === `ecommerce-l2-cumulative-1.5-${backend}`);
    assert(fixture);
    assert.equal(fixture.status, 'active');
    assert.deepEqual(fixture.recipes,
      ['ecommerce.l2-standard@1.5.0', 'ecommerce.l2-standard@1.6.0']);
    assert.deepEqual(fixture.mutationManifests,
      [`grader/mutations/${backend}-ecom-l2-cumulative-1.5.0.json`]);
  }
});
