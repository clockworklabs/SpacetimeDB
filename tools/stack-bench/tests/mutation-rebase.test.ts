import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { requireRecipeRelease } from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';
import { readMutationManifest } from '../src/evidence/mutation-analysis.js';
import { rebaseMutationManifest } from '../src/evidence/mutation-rebase.js';
import {
  compileFeatureCatalogInput,
  compileProgressionDefinitionFile,
} from '../src/progression/progression-definition.js';
import { resolveProgressionRecipeLevelSelection }
  from '../src/progression/progression-recipe-selection.js';
import { loadReferenceRegistry, selectImportedReferenceFixture }
  from '../src/references/reference-fixtures.js';

const ROOT = STACK_BENCH_ROOT;
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const recipe = 'ecommerce.progression-catalog@2.0.1';
const binding = requireRecipeRelease(loadTrack('ecommerce'), 3, recipe);
const definition = compileProgressionDefinitionFile(
  join(TRACK, 'progression', 'ecommerce-2.0.1.json'), { trackRoot: TRACK });
const selection = resolveProgressionRecipeLevelSelection(
  binding, compileFeatureCatalogInput(definition), 3, { cumulative: true });

test('a stale mutation anchor is reported and never copied', () => {
  const fixture = selectImportedReferenceFixture(loadReferenceRegistry(), {
    backend: 'mongodb', track: 'ecommerce', level: 3, recipe,
  });
  const manifest = structuredClone(readMutationManifest(join(
    ROOT,
    'grader',
    'mutations',
    'mongodb-ecommerce-2.0.1.json',
  )));
  const mutation = manifest.mutations[0];
  assert.ok(mutation);
  const edit = mutation.edits[0];
  assert.ok(edit);
  edit.find = 'not present in the cumulative reference';

  const result = rebaseMutationManifest(manifest, {
    release: binding.release,
    selectedCheckKeys: selection.grader.checkKeys,
    app: join(ROOT, fixture.targetPath),
    fixtureSha256: fixture.imported.sourceSha256,
  });

  assert(result.blocked.some(item => item.id === mutation.id
    && item.reason === 'anchor-mismatch'));
  assert(!result.manifest.mutations.some(item => item.id === mutation.id));
});

test('global checks separate valid unselected targets from unknown targets', () => {
  const fixture = selectImportedReferenceFixture(loadReferenceRegistry(), {
    backend: 'mongodb', track: 'ecommerce', level: 3, recipe,
  });
  const manifest = structuredClone(readMutationManifest(join(
    ROOT, 'grader', 'mutations', 'mongodb-ecommerce-2.0.1.json')));
  const outside = manifest.mutations.find(mutation =>
    mutation.targets?.includes('ecommerce.feature.catalog.catalog.2a'));
  assert(outside);
  const unknown = structuredClone(outside);
  unknown.id = 'unknown-check';
  unknown.targets = ['ecommerce.unknown.check'];
  manifest.mutations = [outside, unknown];

  const result = rebaseMutationManifest(manifest, {
    release: binding.release,
    selectedCheckKeys: selection.grader.checkKeys,
    knownCheckKeys: [
      ...binding.release.checkCatalog.map(check => check.stableKey),
      'ecommerce.feature.catalog.catalog.2a',
    ],
    app: join(ROOT, fixture.targetPath),
    fixtureSha256: fixture.imported.sourceSha256,
  });

  assert.deepEqual(result.excluded, [{
    id: outside.id,
    targets: ['ecommerce.feature.catalog.catalog.2a'],
  }]);
  assert.deepEqual(result.blocked, [{
    id: 'unknown-check',
    reason: 'unknown-target',
    targets: ['ecommerce.unknown.check'],
  }]);
});
