import assert from 'node:assert/strict';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.js';
import { resolveFeatureCatalog } from '../src/progression/feature-catalog-selection.js';
import { compileFeatureCatalogInput } from '../src/progression/progression-definition.js';

test('feature catalogs resolve by exact identity', () => {
  const catalog = resolveFeatureCatalog('ecommerce.questlines@2.0.2', loadTrack('ecommerce'));
  assert.equal(catalog.definition.id, 'ecommerce.questlines');
  assert.equal(catalog.definition.version, '2.0.2');
  assert.equal(catalog.definition.nodes.length, 43);
  assert.throws(() => resolveFeatureCatalog('ecommerce.questlines', loadTrack('ecommerce')),
    /exact id@version/);
  assert.throws(() => resolveFeatureCatalog('ecommerce.missing@1.0.0', loadTrack('ecommerce')),
    /resolve exactly one/);
});

test('feature catalog identity excludes root governance text only', () => {
  const catalog = resolveFeatureCatalog('ecommerce.questlines@2.0.2', loadTrack('ecommerce'));
  const governance = compileFeatureCatalogInput({
    ...catalog.definition,
    state: catalog.definition.state === 'draft' ? 'qualified' : 'draft',
    title: `${catalog.definition.title} renamed`,
  });
  assert.equal(governance.identity.sha256, catalog.identity.sha256);

  const nodeTitle = structuredClone(catalog.definition);
  nodeTitle.nodes[0]!.title += ' renamed';
  assert.notEqual(compileFeatureCatalogInput(nodeTitle).identity.sha256, catalog.identity.sha256);

  const questlineTitle = structuredClone(catalog.definition);
  questlineTitle.questlines[0]!.title += ' renamed';
  assert.notEqual(compileFeatureCatalogInput(questlineTitle).identity.sha256, catalog.identity.sha256);

  const behavior = structuredClone(catalog.definition);
  behavior.nodes[0]!.gradingChecks[0]!.points += 1;
  assert.notEqual(compileFeatureCatalogInput(behavior).identity.sha256, catalog.identity.sha256);
});
