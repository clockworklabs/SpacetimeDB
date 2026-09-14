import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileCalibrationFile } from '../src/composition/calibration-compiler.js';
import { loadReferenceRegistry, validateReferenceRegistry } from '../src/references/reference-fixtures.js';
import { buildRecipeRelease } from '../src/composition/recipe-release.js';

const BENCH = STACK_BENCH_ROOT;
const TRACK = join(BENCH, 'tracks', 'ecommerce');
const BACKENDS = ['mongodb', 'postgres', 'spacetime'];
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'sequential-l1.json'));
const calibration = compileCalibrationFile('composition/calibrations/sequential-l1.json', {
  trackRoot: TRACK,
  stackBenchRoot: BENCH,
  release,
});

test('the sequential L1 calibration binds the current recipe and all stacks', () => {
  assert.equal(release.id, 'ecommerce.sequential-l1');
  assert.equal(calibration.recipe.id, release.id);
  assert.equal(calibration.recipe.contentSha256, release.contentSha256);
  assert.deepEqual(calibration.qualification.evidence, []);
  assert.deepEqual(calibration.qualification.stacks, BACKENDS);
  assert.equal(calibration.references.entries.every(reference =>
    reference.id === `ecommerce-reference-${reference.backend}`), true);
});

test('the registry owns one current L1 mutation set per backend', () => {
  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry).issues, []);
  for (const backend of BACKENDS) {
    const fixture = registry.fixtures.find(candidate =>
      candidate.id === `ecommerce-reference-${backend}`);
    assert(fixture?.recipes);
    assert(fixture.mutationManifests);
    assert(fixture.recipes.includes('ecommerce.sequential-l1'));
    const calibrated = calibration.mutations.find(mutation => mutation.backend === backend);
    assert(calibrated);
    assert.deepEqual(fixture.mutationManifests, [calibrated.path]);
  }
});

test('the comprehensive release has scored defects and only two supporting controls', () => {
  const scoredKeys = release.checkCatalog.filter(check => check.points > 0)
    .map(check => check.stableKey).sort();
  assert.equal(release.checkCatalog.length, 48);
  assert.equal(release.scoring.points, 58);
  assert.equal(scoredKeys.length, 46);
  for (const backend of BACKENDS) {
    const mutation = calibration.mutations.find(entry => entry.backend === backend);
    assert(mutation);
    assert.deepEqual([...new Set(mutation.targets.flatMap(target => target.stableKeys))]
      .filter(key => scoredKeys.includes(key)).sort(), scoredKeys);
  }
  assert.deepEqual(calibration.controls.map(control => [control.stableKey, control.role]), [
    ['ecommerce.spec.concurrency-safety.restock-race.202-control', 'precondition'],
    ['ecommerce.spec.external-data-sync.external-stock.901b', 'precondition'],
  ]);
});
