import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { buildRecipeRelease, requireRecipeRelease }
  from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';
import { compileFeatureCatalogInput, compileProgressionDefinitionFile,
  selectFeatureCatalogLevels } from '../src/progression/progression-definition.js';
import { loadReferenceRegistry, selectReferenceFixture }
  from '../src/references/reference-fixtures.js';

const ROOT = STACK_BENCH_ROOT;
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const RECIPE = join(TRACK, 'composition', 'recipes', 'progression-depth3-2.0.1.json');
const PROGRESSION = join(TRACK, 'progression', 'ecommerce-2.0.1.json');

test('the depth-3 recipe covers only the first three graph depths', () => {
  const release = buildRecipeRelease(RECIPE, { trackRoot: TRACK });
  const catalog = selectFeatureCatalogLevels(compileFeatureCatalogInput(
    compileProgressionDefinitionFile(PROGRESSION, { trackRoot: TRACK })), [1, 2, 3]);
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
});

test('the candidate catalog binds the same scoped recipe to L1 through L3', () => {
  const track = loadTrack('ecommerce');
  for (const level of [1, 2, 3]) {
    const binding = requireRecipeRelease(track, level,
      'ecommerce.progression-depth3@2.0.1');
    assert.equal(binding.release.id, 'ecommerce.progression-depth3');
    assert.equal(binding.status, 'candidate');
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
