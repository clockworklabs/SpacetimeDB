import assert from 'node:assert/strict';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.js';
import { resolveFeatureCatalog } from '../src/progression/feature-catalog-selection.js';

test('feature catalogs resolve by exact identity', () => {
  const catalog = resolveFeatureCatalog('ecommerce.questlines@2.0.1', loadTrack('ecommerce'));
  assert.equal(catalog.definition.id, 'ecommerce.questlines');
  assert.equal(catalog.definition.version, '2.0.1');
  assert.equal(catalog.definition.nodes.length, 43);
  assert.throws(() => resolveFeatureCatalog('ecommerce.questlines', loadTrack('ecommerce')),
    /exact id@version/);
  assert.throws(() => resolveFeatureCatalog('ecommerce.missing@1.0.0', loadTrack('ecommerce')),
    /resolve exactly one/);
});
