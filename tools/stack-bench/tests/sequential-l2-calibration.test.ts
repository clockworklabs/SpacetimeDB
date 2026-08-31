import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { qualificationReadiness } from '../commands/qualification-cli.js';
import { compileCalibrationFile } from '../src/composition/calibration-compiler.js';
import { compilePromotionFile } from '../src/composition/composition-compiler.js';
import { buildRecipeRelease } from '../src/composition/recipe-release.js';
import { loadReferenceRegistry, validateReferenceRegistry } from '../src/references/reference-fixtures.js';

const BENCH = STACK_BENCH_ROOT;
const TRACK_ROOT = join(BENCH, 'tracks', 'ecommerce');
const RECIPE = join(TRACK_ROOT, 'composition', 'recipes', 'sequential-l2-1.6.0.json');
const release = buildRecipeRelease(RECIPE, { trackRoot: TRACK_ROOT });
const calibration = compileCalibrationFile('composition/calibrations/sequential-l2-1.6.0.json', {
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
  assert.equal(release.state, 'draft');
  assert.equal(calibration.state, 'draft');
  assert.equal(calibration.promotion.status, 'candidate');
  assert.deepEqual(calibration.qualification.evidence, []);
  assert.deepEqual(calibration.qualification.stacks, [
    { id: 'mongodb', status: 'candidate' },
    { id: 'postgres', status: 'candidate' },
    { id: 'spacetime', status: 'candidate' },
  ]);
  assert(calibration.references.entries.every(reference => reference.status === 'candidate'));
  assert(calibration.mutations.every(mutation => mutation.status === 'candidate'));

  assert.equal(release.version, '1.6.0');
  assert.equal(release.task.mode, 'upgrade');
  const baseRecipe = release.task.baseRecipe;
  if (!baseRecipe) throw new Error('L2 1.6 release has no base recipe');
  assert.deepEqual([baseRecipe.id, baseRecipe.version],
    ['ecommerce.sequential-l1', '2.5.0']);
});

test('L2 qualification uses the exact current L1 base', () => {
  assert.equal(qualificationReadiness('ecommerce', 2,
    'ecommerce.sequential-l2@1.6.0').scope.recipe.version, '1.6.0');
});

test('the catalogs and registry retain L2 1.6 as an exact candidate', () => {
  const catalog = compilePromotionFile(join(TRACK_ROOT, 'composition', 'candidates.json'), {
    trackRoot: TRACK_ROOT,
  });
  assert.deepEqual(catalog.entries.filter(entry => entry.alias === 'L2'
    && entry.recipe.id === 'ecommerce.sequential-l2'), []);
  const promotions = compilePromotionFile(join(TRACK_ROOT, 'composition', 'promotions.json'), {
    trackRoot: TRACK_ROOT,
  });
  assert.deepEqual(promotions.entries.filter(entry => entry.alias === 'L2'
    && entry.status === 'candidate'), [{
    alias: 'L2',
    status: 'candidate',
    recipe: {
      path: 'recipes/sequential-l2-1.6.0.json',
      id: 'ecommerce.sequential-l2',
      version: '1.6.0',
    },
  }]);

  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry).issues, []);
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const fixture = registry.fixtures.find(entry =>
      entry.id === `ecommerce-reference-${backend}`);
    assert(fixture);
    assert.equal(fixture.status, 'candidate');
    assert(fixture.recipes);
    assert(fixture.recipes.includes('ecommerce.sequential-l2@1.6.0'));
    const calibrated = calibration.mutations.find(mutation => mutation.backend === backend);
    assert(calibrated);
    assert.deepEqual(fixture.mutationManifests, [calibrated.path]);
  }
});
