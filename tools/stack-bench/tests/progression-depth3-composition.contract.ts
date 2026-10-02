import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
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
