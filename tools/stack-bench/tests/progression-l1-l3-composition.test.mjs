import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileCalibrationFile } from '../dist/src/composition/calibration-compiler.js';
import { buildRecipeRelease, resolveRecipeRelease }
  from '../dist/src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';
import { compileFeatureCatalogInput, compileProgressionDefinitionFile,
  selectFeatureCatalogLevels } from '../dist/src/progression/progression-definition.js';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const RECIPE = join(TRACK, 'composition', 'recipes', 'progression-l1-l3-1.0.0.json');
const CALIBRATION = join(TRACK, 'composition', 'calibrations',
  'progression-l1-l3-1.0.0.json');
const PROGRESSION = join(TRACK, 'progression', 'ecommerce-1.1.0.json');

test('the cumulative L1-L3 release contains only its catalog checks', () => {
  const release = buildRecipeRelease(RECIPE, { trackRoot: TRACK });
  const fullCatalog = compileFeatureCatalogInput(
    compileProgressionDefinitionFile(PROGRESSION, { trackRoot: TRACK }));
  const catalog = selectFeatureCatalogLevels(fullCatalog, [1, 2, 3]);
  const catalogChecks = catalog.definition.nodes
    .flatMap(node => node.gradingChecks.map(check => check.id)).sort();
  const releaseChecks = release.checkCatalog.filter(check => check.points > 0)
    .map(check => check.stableKey).sort();

  assert.equal(release.id, 'ecommerce.progression-l1-l3');
  assert.equal(release.components.packs.length, 39);
  assert.equal(release.scoring.points, 199);
  assert.deepEqual(releaseChecks, catalogChecks);
  assert.deepEqual(release.checkCatalog.filter(check => check.points === 0)
    .map(check => check.stableKey).sort(), [
    'ecommerce.spec.concurrency-safety.restock-race.202-control',
    'ecommerce.spec.external-data-sync.external-stock.901b',
  ]);

  const packIds = new Set(release.components.packs.map(pack => pack.id));
  for (const id of [
    'ecommerce.l2.price-history-features',
    'ecommerce.l3.cart-expiration-features',
    'ecommerce.l3.order-delivery-features',
    'ecommerce.l3.order-returns-features',
    'ecommerce.progression.automatic-reorder',
    'ecommerce.progression.cancellation-queue-specifications',
    'ecommerce.progression.cart-recovery',
    'ecommerce.progression.delivery-notifications',
    'ecommerce.progression.order-support',
    'ecommerce.progression.personalized-recommendations',
    'ecommerce.progression.price-accounting-specifications',
    'ecommerce.progression.promotion-reporting',
    'ecommerce.progression.recommendation-feedback',
    'ecommerce.progression.staff-activity',
    'ecommerce.progression.support-refunds',
  ]) assert.equal(packIds.has(id), false, id);
});

test('the qualified calibration binds the exact cumulative L1-L3 release', () => {
  const release = buildRecipeRelease(RECIPE, { trackRoot: TRACK });
  const calibration = compileCalibrationFile(CALIBRATION, {
    trackRoot: TRACK,
    stackBenchRoot: ROOT,
    release,
  });

  assert.equal(calibration.id, 'ecommerce.progression-l1-l3-calibration');
  assert.equal(calibration.state, 'qualified');
  assert.equal(calibration.recipe.contentSha256, release.contentSha256);
  assert.equal(calibration.qualification.checks.length, 112);
  assert.equal(calibration.qualification.featureCatalog.sha256,
    '6ee12eae91afcdb8a83293dd4218fcbaac24982b6b22648a4859881a0c9f9aee');
  assert.equal(calibration.qualification.evidence.length, 7);
  assert.deepEqual(calibration.qualification.stacks.map(stack => stack.status),
    ['qualified', 'qualified', 'qualified']);
  assert.equal(calibration.promotion.alias, 'L3');
  assert.equal(calibration.promotion.status, 'promoted');
});

test('L1-L3 share the scoped release while L4-L5 keep the full draft release', () => {
  const track = loadTrack('ecommerce');
  assert.deepEqual([1, 2, 3].map(level =>
    resolveRecipeRelease(track, level, 'ecommerce.progression-l1-l3@1.0.0').release.id), [
    'ecommerce.progression-l1-l3',
    'ecommerce.progression-l1-l3',
    'ecommerce.progression-l1-l3',
  ]);
  assert.deepEqual([4, 5].map(level =>
    resolveRecipeRelease(track, level, 'ecommerce.progression-catalog@1.0.0').release.id), [
    'ecommerce.progression-catalog',
    'ecommerce.progression-catalog',
  ]);
});
