import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { qualificationReadiness } from '../commands/qualification-cli.js';
import { compileCalibrationFile } from '../src/composition/calibration-compiler.js';
import { buildRecipeRelease } from '../src/composition/recipe-release.js';
import { loadReferenceRegistry, validateReferenceRegistry } from '../src/references/reference-fixtures.js';

const BENCH = STACK_BENCH_ROOT;
const TRACK_ROOT = join(BENCH, 'tracks', 'ecommerce');
const RECIPE = join(TRACK_ROOT, 'composition', 'recipes', 'sequential-l2.json');
const release = buildRecipeRelease(RECIPE, { trackRoot: TRACK_ROOT });
const calibration = compileCalibrationFile('composition/calibrations/sequential-l2.json', {
  trackRoot: TRACK_ROOT,
  stackBenchRoot: BENCH,
  release,
});

test('L2 calibration binds every scored check to an exact defect per backend', () => {
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

test('L2 calibration binds the current recipe and all stacks', () => {
  assert.equal(release.id, 'ecommerce.sequential-l2');
  assert.equal(calibration.recipe.id, release.id);
  assert.equal(calibration.recipe.contentSha256, release.contentSha256);
  assert.deepEqual(calibration.qualification.evidence, []);
  assert.deepEqual(calibration.qualification.stacks, ['mongodb', 'postgres', 'spacetime']);
  assert(calibration.references.entries.every(reference =>
    reference.id === `ecommerce-reference-${reference.backend}`));

  assert.equal(release.task.mode, 'upgrade');
  const baseRecipe = release.task.baseRecipe;
  if (!baseRecipe) throw new Error('L2 release has no base recipe');
  assert.equal(baseRecipe.id, 'ecommerce.sequential-l1');
});

test('L2 qualification uses the exact current L1 base', () => {
  assert.equal(qualificationReadiness('ecommerce', 2,
    'ecommerce.sequential-l2').scope.recipe.id, 'ecommerce.sequential-l2');
});

test('the registry owns one current L2 mutation set per backend', () => {
  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry).issues, []);
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const fixture = registry.fixtures.find(entry =>
      entry.id === `ecommerce-reference-${backend}`);
    assert(fixture);
    assert(fixture.recipes);
    assert(fixture.recipes.includes('ecommerce.sequential-l2'));
    const calibrated = calibration.mutations.find(mutation => mutation.backend === backend);
    assert(calibrated);
    assert.deepEqual(fixture.mutationManifests, [calibrated.path]);
  }
});
