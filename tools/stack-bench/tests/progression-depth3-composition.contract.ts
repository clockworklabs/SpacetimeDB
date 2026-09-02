import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('catalog fixtures, warehouse counts, and pagination agree', () => {
  const fixture = JSON.parse(readFileSync(join(TRACK,
    'composition', 'fixtures', 'storefront-1.0.1.json'), 'utf8')) as {
    items: Array<{ name: string }>; warehouses: unknown[];
  };
  const operations = JSON.parse(readFileSync(join(TRACK,
    'composition', 'fixtures', 'operations-1.0.1.json'), 'utf8')) as {
    items: Array<{ name: string }>; warehouses: unknown[];
  };
  assert.deepEqual(operations.items.map(item => item.name), fixture.items.map(item => item.name));
  assert.equal(operations.warehouses.length, fixture.warehouses.length);
  for (const file of ['01-warehouse-admin-staff-1.0.0.json',
    '01-warehouse-admin-2.4.0.json', '01-features.json']) {
    const scenario = JSON.parse(readFileSync(join(TRACK, 'scenarios', file), 'utf8')) as {
      features: Array<{ criteria: Array<{ id: string; steps: Array<{
        testid?: string; count?: number;
      }> }> }>;
    };
    const steps = scenario.features.flatMap(feature => feature.criteria)
      .find(criterion => criterion.id === '7b')?.steps ?? [];
    assert.equal(steps.find(step => step.testid === 'admin-item-row')?.count,
      fixture.items.length, file);
    const locations = steps.find(step => step.testid === 'admin-location-row');
    if (locations) assert.equal(locations.count,
      fixture.items.length * fixture.warehouses.length, file);
  }
  const pagination = JSON.parse(readFileSync(join(TRACK,
    'scenarios', 'progression-faceted-pagination-1.0.0.json'), 'utf8')) as {
    features: Array<{ criteria: Array<{ id: string; steps: Array<{ equals?: string[] }> }> }>;
  };
  const pages = pagination.features.flatMap(feature => feature.criteria)
    .find(criterion => criterion.id === '402a')?.steps
    .flatMap(step => step.equals ? [step.equals] : []) ?? [];
  const names = fixture.items.map(item => item.name).sort();
  assert.deepEqual(pages, [names.slice(0, 10), names.slice(10), names.slice(0, 10)]);
});

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
