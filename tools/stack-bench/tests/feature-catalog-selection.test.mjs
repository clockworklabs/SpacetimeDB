import assert from 'node:assert/strict';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.mjs';
import { resolveFeatureCatalog } from '../src/progression/feature-catalog-selection.mjs';

test('feature catalogs resolve by exact identity', () => {
  const catalog = resolveFeatureCatalog('ecommerce.questlines@1.0.0', loadTrack('ecommerce'));
  assert.equal(catalog.definition.id, 'ecommerce.questlines');
  assert.equal(catalog.definition.version, '1.0.0');
  assert.equal(catalog.definition.nodes.length, 39);
  assert.throws(() => resolveFeatureCatalog('ecommerce.questlines', loadTrack('ecommerce')),
    /exact id@version/);
  assert.throws(() => resolveFeatureCatalog('ecommerce.missing@1.0.0', loadTrack('ecommerce')),
    /resolve exactly one/);
});
