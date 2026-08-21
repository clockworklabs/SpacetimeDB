import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { qualificationReadiness } from '../commands/qualification-cli.mjs';
import { compileCalibrationFile } from '../src/composition/calibration-compiler.mjs';
import { compilePromotionFile } from '../src/composition/composition-compiler.mjs';
import { buildRecipeRelease, resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';
import { loadReferenceRegistry, validateReferenceRegistry } from '../src/references/reference-fixtures.mjs';

const BENCH = join(import.meta.dirname, '..');
const TRACK_ROOT = join(BENCH, 'tracks', 'ecommerce');
const RECIPE = join(TRACK_ROOT, 'composition', 'recipes', 'l2-standard-1.5.0.json');
const release = buildRecipeRelease(RECIPE, { trackRoot: TRACK_ROOT });
const calibration = compileCalibrationFile('composition/calibrations/l2-standard-1.5.0.json', {
  trackRoot: TRACK_ROOT,
  stackBenchRoot: BENCH,
  release,
});

test('L2 1.5 calibration binds every scored check to an exact defect per backend', () => {
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

test('L2 1.5 is qualified and promoted by its exact evidence set', () => {
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
  assert(calibration.references.entries.every(reference => reference.status === 'active'));
  assert(calibration.mutations.every(mutation => mutation.status === 'active'));

  const track = loadTrack('ecommerce');
  const promoted = resolveRecipeRelease(track, 2);
  const exact = resolveRecipeRelease(track, 2, 'ecommerce.l2-standard@1.5.0');
  assert.deepEqual([promoted.release.version, promoted.status], ['1.5.0', 'promoted']);
  assert.deepEqual([exact.release.version, exact.status], ['1.5.0', 'promoted']);
});

test('L2 1.5 is promotion ready with complete defect and qualification evidence', () => {
  const readiness = qualificationReadiness('ecommerce', 2, 'ecommerce.l2-standard@1.5.0');
  assert.equal(readiness.launch.ok, true);
  assert.deepEqual(readiness.defectChecks.stacks.map(stack => ({
    stack: stack.stack,
    coveredChecks: stack.coveredChecks,
    missingChecks: stack.missingChecks,
  })), [
    { stack: 'mongodb', coveredChecks: 74, missingChecks: [] },
    { stack: 'postgres', coveredChecks: 74, missingChecks: [] },
    { stack: 'spacetime', coveredChecks: 74, missingChecks: [] },
  ]);
  assert.equal(readiness.promotion.ready, true);
  assert.deepEqual(readiness.promotion.blockers, []);
});

test('the promotion catalog and registry own exactly the L2 1.5 qualified inputs', () => {
  const catalog = compilePromotionFile(join(TRACK_ROOT, 'composition', 'candidates.json'), {
    trackRoot: TRACK_ROOT,
  });
  assert.deepEqual(catalog.entries.filter(entry => entry.alias === 'L2'), []);
  const promotions = compilePromotionFile(join(TRACK_ROOT, 'composition', 'promotions.json'), {
    trackRoot: TRACK_ROOT,
  });
  assert.deepEqual(promotions.entries.filter(entry => entry.alias === 'L2'
    && entry.status === 'promoted'), [{
    alias: 'L2',
    status: 'promoted',
    recipe: {
      path: 'recipes/l2-standard-1.5.0.json',
      id: 'ecommerce.l2-standard',
      version: '1.5.0',
    },
  }]);

  const registry = loadReferenceRegistry();
  assert.deepEqual(validateReferenceRegistry(registry).issues, []);
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const fixture = registry.fixtures.find(entry =>
      entry.id === `ecommerce-l2-cumulative-1.5-${backend}`);
    assert(fixture);
    assert.equal(fixture.status, 'active');
    assert.deepEqual(fixture.recipes, ['ecommerce.l2-standard@1.5.0']);
    assert.deepEqual(fixture.mutationManifests,
      [`grader/mutations/${backend}-ecom-l2-cumulative-1.5.0.json`]);
  }
});
