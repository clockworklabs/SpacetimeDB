import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { calibrationQualificationRelease, compileCalibrationFile }
  from '../src/composition/calibration-compiler.js';
import { buildRecipeRelease, executionPlanForRelease, requireRecipeRelease }
  from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';
import { compileFeatureCatalogInput, compileProgressionDefinitionFile,
  selectFeatureCatalogLevels } from '../src/progression/progression-definition.js';
import { loadReferenceRegistry, selectReferenceFixture }
  from '../src/references/reference-fixtures.js';
import { qualificationReadiness } from '../commands/qualification-cli.js';

const ROOT = STACK_BENCH_ROOT;
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const RECIPE = join(TRACK, 'composition', 'recipes', 'progression-depth3-2.0.1.json');
const CALIBRATION = 'composition/calibrations/progression-depth3-2.0.1.json';
const PROGRESSION = join(TRACK, 'progression', 'ecommerce-2.0.1.json');

test('the qualified depth-3 recipe covers only the first three graph depths', () => {
  const release = buildRecipeRelease(RECIPE, { trackRoot: TRACK });
  const calibration = compileCalibrationFile(CALIBRATION, {
    trackRoot: TRACK,
    stackBenchRoot: ROOT,
    release,
  });
  const qualification = calibrationQualificationRelease(calibration, release,
    executionPlanForRelease(RECIPE, { trackRoot: TRACK, level: 3 }));
  const catalog = selectFeatureCatalogLevels(compileFeatureCatalogInput(
    compileProgressionDefinitionFile(PROGRESSION, { trackRoot: TRACK })), [1, 2, 3]);
  const catalogChecks = catalog.definition.nodes
    .flatMap(node => node.gradingChecks.map(check => check.id)).sort();
  const selectedPackIds = release.components.packs.map(pack => pack.id);

  assert.equal(catalog.definition.nodes.length, 27);
  assert.equal(release.components.packs.length, 40);
  assert.equal(release.checkCatalog.length, 99);
  assert.equal(release.scoring.points, 162);
  for (const excluded of [
    'ecommerce.l2.price-history-features',
    'ecommerce.l3.reservations-features',
    'ecommerce.progression.automatic-reorder',
  ]) assert.equal(selectedPackIds.includes(excluded), false, `${excluded} exceeds depth 3`);
  assert.equal(qualification.release.checkCatalog.length, 97);
  assert.equal(qualification.release.scoring.points, 162);
  assert.deepEqual(qualification.release.checkCatalog.map(check => check.stableKey).sort(),
    catalogChecks);
  assert.equal(calibration.qualification.evidence.length, 7);
  assert.deepEqual(calibration.qualification.stacks.map(stack => stack.status),
    ['qualified', 'qualified', 'qualified']);
});

test('the promotion catalog binds the same scoped recipe to L1 through L3', () => {
  const track = loadTrack('ecommerce');
  for (const level of [1, 2, 3]) {
    const binding = requireRecipeRelease(track, level,
      'ecommerce.progression-depth3@2.0.1');
    assert.equal(binding.release.id, 'ecommerce.progression-depth3');
    assert.equal(binding.status, 'promoted');
  }
});

test('the depth-3 recipe binds one maintained reference per stack', () => {
  const registry = loadReferenceRegistry();
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    const fixture = selectReferenceFixture(registry, { backend, track: 'ecommerce', level: 3,
      recipe: 'ecommerce.progression-depth3@2.0.1' });
    assert.equal(fixture.id, `ecommerce-reference-${backend}`);
  }
});

test('depth-3 qualification outputs are unique to the exact recipe content', () => {
  const status = qualificationReadiness('ecommerce', 3,
    'ecommerce.progression-depth3@2.0.1');
  const artifactHash = status.scope.recipe.contentSha256.slice(0, 12);
  assert(status.commands.every(command => command.includes(`ecommerce-l3-${artifactHash}`)));
  assert.equal(status.artifactPaths.null,
    `/var/lib/stack-bench/results/qualification/ecommerce-l3-${artifactHash}-null.json`);
});
