import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileRecipeFile } from '../src/composition/composition-compiler.js';
import { buildRecipeRelease, type RecipeBinding }
  from '../src/composition/recipe-release.js';
import { compileFeatureCatalogInput, compileProgressionDefinitionFile }
  from '../src/progression/progression-definition.js';
import { validateProgressionRecipeBindings }
  from '../src/progression/progression-recipe-selection.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const definitionPath = join(trackRoot, 'progression', 'ecommerce-2.0.1.json');
const recipePath = join(trackRoot, 'composition', 'recipes', 'progression-catalog-2.0.1.json');

test('the 2.0 ecommerce catalog has clear questlines and product-only edges', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const byId = new Map(definition.nodes.map(node => [node.id, node]));

  assert.equal(definition.nodes.length, 43);
  assert.equal(definition.questlines.length, 12);
  assert.deepEqual(definition.questlines.map(questline => questline.id).sort(), [
    'business-operations',
    'catalog-discovery',
    'customer-support',
    'identity-profiles',
    'inventory',
    'notifications',
    'orders-fulfillment',
    'promotions',
    'recommendations',
    'reviews',
    'shopping-checkout',
    'staff-access',
  ]);
  assert.equal(new Set(definition.nodes.flatMap(node => node.gradingChecks.map(check => check.id))).size,
    146, 'every scored check has one owner');

  assert.deepEqual(requiredNode(byId, 'cart').dependencies, ['accounts', 'catalog']);
  assert.deepEqual(requiredNode(byId, 'catalog-discovery').dependencies, ['catalog']);
  assert.deepEqual(requiredNode(byId, 'purchasing').dependencies, ['accounts', 'catalog']);
  assert.deepEqual(requiredNode(byId, 'faceted-search').dependencies, ['catalog-discovery']);
  assert.deepEqual(requiredNode(byId, 'catalog-management').dependencies,
    ['catalog-discovery', 'staff-roles']);
  assert.deepEqual(byId.get('catalog')!.gradingChecks.map(check => check.id),
    ['ecommerce.feature.catalog.catalog-values.2a']);
  assert.deepEqual(byId.get('catalog-discovery')!.gradingChecks.map(check => check.id), [
    'ecommerce.feature.catalog.catalog-ranking.2b',
    'ecommerce.feature.catalog.catalog-search.2d',
  ]);
  assert.deepEqual(requiredNode(byId, 'checkout').dependencies, ['cart']);
  assert.deepEqual(requiredNode(byId, 'stock-transfers').dependencies, ['warehouse-admin']);
  assert.deepEqual(requiredNode(byId, 'price-history').dependencies, ['catalog-management']);
  assert.deepEqual(requiredNode(byId, 'inventory-dashboard').dependencies, ['warehouse-admin']);
  assert.deepEqual(requiredNode(byId, 'sales-dashboard').dependencies, ['purchasing']);
  assert.deepEqual(requiredNode(byId, 'recommendations').dependencies, ['cart', 'purchasing']);
  assert.equal(byId.has('operational-views'), false);
});

test('every 2.0 feature and grading check binds to the 2.0 recipe', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const plan = compileRecipeFile(recipePath, { trackRoot });
  const release = buildRecipeRelease(recipePath, { trackRoot });
  const binding: RecipeBinding = {
    alias: 'ecommerce.progression-catalog@2.0.1',
    status: 'draft',
    catalog: {
      id: plan.recipe.id,
      version: plan.recipe.version,
      state: 'draft',
      title: plan.recipe.title,
      path: recipePath,
      sha256: release.contentSha256,
    },
    recipePath,
    plan,
    release,
    execution: [],
  };
  const levels = [...new Set(definition.nodes.map(node => node.level))].sort((a, b) => a - b);

  validateProgressionRecipeBindings(compileFeatureCatalogInput(definition),
    levels.map(level => ({ level, binding })), { levels });

  const owned = new Set(definition.nodes.flatMap(node =>
    node.gradingChecks.map(check => check.id)));
  const unowned = plan.checks.filter(check => !owned.has(check.stableKey));
  assert.deepEqual(unowned.map(check => [check.stableKey, check.points]), [
    ['ecommerce.spec.external-data-sync.external-stock.901b', 0],
    ['ecommerce.spec.concurrency-safety.restock-race.202-control', 0],
  ], 'only zero-point controls may sit outside feature scoring');
});

test('every 2.0 feature pack has a bounded and testable product contract', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const plan = compileRecipeFile(recipePath, { trackRoot });
  const featurePacks = plan.packs.filter(pack => pack.moduleType === 'feature');

  assert.equal(featurePacks.length, definition.nodes.length,
    'each graph node must own one feature pack');
  for (const pack of featurePacks) {
    assert.equal(pack.task.requirementIds.length, 1,
      `${pack.id}@${pack.version} must state one product request`);
    assert.equal(pack.task.contractIds.length, 1,
      `${pack.id}@${pack.version} must state one testing contract`);
    assert(pack.capabilities.length > 0,
      `${pack.id}@${pack.version} must declare its required capabilities`);
    assert(pack.evidence.length > 0,
      `${pack.id}@${pack.version} must declare its required evidence`);
    assert.equal(pack.budget.status, 'bounded',
      `${pack.id}@${pack.version} must have a bounded runtime`);
    assert(pack.budget.maxRuntimeMs > 0,
      `${pack.id}@${pack.version} must have a positive runtime limit`);
  }
});

test('cross-feature grading requirements stay separate from product dependencies', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  const ownerByFeature = new Map(definition.nodes.flatMap(node => node.featureRefs
    .map(ref => [ref.replace(/@.*$/, ''), node.id] as const)));
  const ancestors = (nodeId: string, found = new Set([nodeId])): Set<string> => {
    for (const dependency of nodes.get(nodeId)?.dependencies ?? []) {
      if (!found.has(dependency)) {
        found.add(dependency);
        ancestors(dependency, found);
      }
    }
    return found;
  };
  const deferred = definition.nodes.flatMap(node => {
    const productAncestors = ancestors(node.id);
    return node.gradingChecks.flatMap(check => (check.requiresFeatures ?? []).flatMap(feature => {
      const owner = ownerByFeature.get(feature);
      return owner && !productAncestors.has(owner)
        ? [`${node.id}:${check.id}:${owner}`]
        : [];
    }));
  }).sort();

  assert.deepEqual(deferred, [
    'automatic-reorder:ecommerce.progression.automatic-reorder.automatic-reorder.502a:purchasing',
    'automatic-reorder:ecommerce.progression.automatic-reorder.automatic-reorder.502b:purchasing',
    'inventory-dashboard:ecommerce.inventory-operations.operational-views.5a:purchasing',
    'price-history:ecommerce.returns-pricing.price-history.4a:purchasing',
    'price-history:ecommerce.returns-pricing.price-history.4c:checkout',
    'sales-dashboard:ecommerce.spec.transactional-integrity.books-balance.107a:warehouse-admin',
    'sales-dashboard:ecommerce.spec.transactional-integrity.books-balance.107b:warehouse-admin',
    'stock-transfers:ecommerce.inventory-operations.stock-conservation.202d:purchasing',
  ]);
});

function requiredNode(
  nodes: ReadonlyMap<string, { dependencies: string[] }>,
  id: string,
): { dependencies: string[] } {
  const node = nodes.get(id);
  assert(node, `${id} must exist`);
  return node;
}
