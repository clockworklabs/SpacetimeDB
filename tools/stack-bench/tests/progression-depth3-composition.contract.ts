import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { buildRecipeRelease, resolveRecipeRelease }
  from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';
import { compileFeatureCatalogInput, compileProgressionDefinitionFile,
  selectFeatureCatalogLevels } from '../src/progression/progression-definition.js';

const TRACK = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');

test('the first three progression depths are a valid prefix of the catalog', () => {
  const full = compileFeatureCatalogInput(compileProgressionDefinitionFile(
    join(TRACK, 'progression', 'ecommerce.json'), { trackRoot: TRACK }));
  const selected = selectFeatureCatalogLevels(full, [1, 2, 3]);

  assert(selected.definition.nodes.length > 0);
  assert(selected.definition.nodes.every(node => node.level <= 3));
  assert(selected.definition.nodes.length < full.definition.nodes.length);
  assert(selected.definition.questlines.every(questline => questline.nodes.length > 0));
});

test('the progression recipe is one stable selection for every configured depth', () => {
  const track = loadTrack('ecommerce');
  const releases = [1, 2, 3].map(level => resolveRecipeRelease(track, level,
    'ecommerce.progression-catalog'));

  assert(releases.every((binding): binding is NonNullable<typeof binding> => binding !== null));
  assert(releases.every(binding => binding.release.id === 'ecommerce.progression-catalog'));
  assert.deepEqual(new Set(releases.map(binding => binding.release.contentSha256)).size, 1);
});

test('the progression recipe uses the maintained operations fixture', () => {
  const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes', 'progression-catalog.json'),
    { trackRoot: TRACK });
  const fixture = JSON.parse(
    readFileSync(join(TRACK, 'composition', 'fixtures', 'operations.json'), 'utf8'),
  ) as { id: string };

  assert.equal(release.components.fixture.id, fixture.id);
  assert(release.checkCatalog.length > 0);
});
