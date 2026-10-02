import assert from 'node:assert/strict';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.js';
import { resolveFeatureCatalog } from '../src/progression/feature-catalog-selection.js';
import { compileFeatureCatalogInput } from '../src/progression/progression-definition.js';

test('feature catalogs resolve from a track-relative definition', () => {
  const catalog = resolveFeatureCatalog('progression/ecommerce.json', loadTrack('ecommerce'));
  assert.equal(catalog.definition.id, 'ecommerce.questlines');
  assert(catalog.definition.nodes.length > 0);
  assert.match(catalog.identity.contentSha256, /^[a-f0-9]{64}$/);
  assert.throws(() => resolveFeatureCatalog('../outside.json', loadTrack('ecommerce')),
    /escapes the track/);
});

test('feature catalog identity excludes its display title only', () => {
  const catalog = resolveFeatureCatalog('progression/ecommerce.json', loadTrack('ecommerce'));
  const governance = compileFeatureCatalogInput({
    ...catalog.definition,
    title: `${catalog.definition.title} renamed`,
  });
  assert.equal(governance.identity.contentSha256, catalog.identity.contentSha256);

  const nodeTitle = structuredClone(catalog.definition);
  nodeTitle.nodes[0]!.title += ' renamed';
  assert.notEqual(compileFeatureCatalogInput(nodeTitle).identity.contentSha256,
    catalog.identity.contentSha256);

  const questlineTitle = structuredClone(catalog.definition);
  questlineTitle.questlines[0]!.title += ' renamed';
  assert.notEqual(compileFeatureCatalogInput(questlineTitle).identity.contentSha256,
    catalog.identity.contentSha256);

  const behavior = structuredClone(catalog.definition);
  behavior.nodes[0]!.gradingChecks[0]!.points += 1;
  assert.notEqual(compileFeatureCatalogInput(behavior).identity.contentSha256,
    catalog.identity.contentSha256);
});
