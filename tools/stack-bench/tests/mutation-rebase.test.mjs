import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { rebaseMutationManifest } from '../src/evidence/mutation-rebase.mjs';
import { compileFeatureCatalogInput,
  compileProgressionDefinitionFile } from '../dist/src/progression/progression-definition.js';
import { resolveProgressionRecipeLevelSelection }
  from '../src/progression/progression-recipe-selection.mjs';
import { loadReferenceRegistry } from '../src/references/reference-fixtures.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const definition = compileProgressionDefinitionFile(
  join(TRACK, 'progression', 'ecommerce-1.0.0.json'), { trackRoot: TRACK });
const binding = resolveRecipeRelease(loadTrack('ecommerce'), 3,
  'ecommerce.progression-catalog@1.0.0');
const selection = resolveProgressionRecipeLevelSelection(binding,
  compileFeatureCatalogInput(definition), 3, { cumulative: true });
const fixtures = new Map(loadReferenceRegistry().fixtures
  .filter(fixture => fixture.id.startsWith('ecommerce-progression-'))
  .map(fixture => [fixture.backend, fixture]));
const expected = {
  mongodb: { mutations: 46, covered: 47, missing: 65,
    blocked: { 'unknown-target': 4, 'anchor-mismatch': 12 }, excluded: 9 },
  postgres: { mutations: 47, covered: 50, missing: 62,
    blocked: { 'unknown-target': 4, 'anchor-mismatch': 13 }, excluded: 9 },
  spacetime: { mutations: 58, covered: 60, missing: 52,
    blocked: { 'unknown-target': 4, 'anchor-mismatch': 2 }, excluded: 10 },
};

for (const backend of Object.keys(expected)) {
  test(`${backend} L2 defects rebase deterministically onto the cumulative L1-L3 app`, () => {
    const fixture = fixtures.get(backend);
    assert(fixture, `missing ${backend} progression fixture`);
    const source = readJson(join(ROOT, 'grader', 'mutations',
      `${backend}-ecom-l2-cumulative-1.5.0.json`));
    const options = {
      release: binding.release,
      selectedCheckKeys: selection.grader.checkKeys,
      app: join(ROOT, fixture.targetPath),
      fixtureSha256: fixture.imported.sourceSha256,
    };
    const first = rebaseMutationManifest(source, options);
    const second = rebaseMutationManifest(source, options);

    assert.deepEqual(first, second);
    assert.equal(first.manifest.mutations.length, expected[backend].mutations);
    assert.equal(first.coverage.covered.length, expected[backend].covered);
    assert.equal(first.coverage.missing.length, expected[backend].missing);
    assert.equal(first.coverage.selected.length, 112);
    assert.equal(first.excluded.length, expected[backend].excluded);
    assert(binding.release.checkCatalog.filter(check =>
      first.coverage.selected.includes(check.stableKey)).every(check => check.points > 0));
    assert.deepEqual(Object.fromEntries(Object.entries(Object.groupBy(first.blocked,
      item => item.reason)).map(([reason, items]) => [reason, items.length])),
    expected[backend].blocked);
    assert(first.manifest.mutations.every(mutation => mutation.targets.every(target =>
      binding.release.checkCatalog.find(check => check.stableKey === target)?.source
        === mutation.scenario.replace(`tracks/${binding.release.track}/`, ''))));
  });
}

test('cross-scenario defects split without changing their source edit', () => {
  const fixture = fixtures.get('postgres');
  const source = readJson(join(ROOT, 'grader', 'mutations',
    'postgres-ecom-l2-cumulative-1.5.0.json'));
  const result = rebaseMutationManifest(source, {
    release: binding.release,
    selectedCheckKeys: selection.grader.checkKeys,
    app: join(ROOT, fixture.targetPath),
    fixtureSha256: fixture.imported.sourceSha256,
  });
  const split = result.manifest.mutations.filter(mutation =>
    mutation.id.startsWith('purchase-stock-change-is-not-broadcast--'));

  assert.equal(split.length, 2);
  assert.equal(new Set(split.map(mutation => JSON.stringify(mutation.edits))).size, 1);
  assert.equal(new Set(split.map(mutation => mutation.scenario)).size, 2);
});

test('a stale anchor is reported and never copied', () => {
  const fixture = fixtures.get('mongodb');
  const source = readJson(join(ROOT, 'grader', 'mutations',
    'mongodb-ecom-l2-cumulative-1.5.0.json'));
  const stale = structuredClone(source);
  stale.mutations[0].edits[0].find = 'not present in the cumulative reference';
  const result = rebaseMutationManifest(stale, {
    release: binding.release,
    selectedCheckKeys: selection.grader.checkKeys,
    app: join(ROOT, fixture.targetPath),
    fixtureSha256: fixture.imported.sourceSha256,
  });

  assert(result.blocked.some(item => item.id === stale.mutations[0].id
    && item.reason === 'anchor-mismatch'));
  assert(!result.manifest.mutations.some(item => item.id === stale.mutations[0].id));
});
