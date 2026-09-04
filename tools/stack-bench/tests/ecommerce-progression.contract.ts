import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { compilePackDefinition, compileRecipeFile, type CompiledPackDefinition }
  from '../src/composition/composition-compiler.js';
import { loadTrack } from '../src/composition/tracks.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { buildRecipeRelease, type RecipeBinding } from '../src/composition/recipe-release.js';
import { compileProgressionDefinition, compileProgressionDefinitionFile,
  compileDependencyPolicyInput, compileFeatureCatalogInput,
  dependencyRuntimeDefinition, type CompiledProgressionNode }
  from '../src/progression/progression-definition.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import type { ProgressionWorkAction } from '../src/progression/progression-engine.js';
import { resolveProgressionRecipeAction, resolveProgressionRepairTarget,
  validateProgressionRecipeBindings } from '../src/progression/progression-recipe-selection.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const definitionPath = join(trackRoot, 'progression', 'ecommerce.json');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));

function packChecks(pack: CompiledPackDefinition): string[] {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    assert(feature, `${pack.id}.${check.id} must select a feature`);
    const criteria = check.criteria === undefined
      ? feature.criteria
      : check.criteria.map(id => {
        const criterion = feature.criteria.find(candidate => candidate.id === id);
        assert(criterion, `${pack.id}.${check.id} must select ${id}`);
        return criterion;
      });
    return criteria.map(criterion =>
      `${pack.stableId ?? pack.id}.${check.stableId ?? check.id}.${criterion.id}`);
  });
}

test('the ecommerce progression definition is complete and calculated from its dependencies', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  assert.equal(definition.nodes.length, 43);
  assert.deepEqual(Object.fromEntries([1, 2, 3, 4, 5, 6].map(level => [
    level,
    definition.nodes.filter(node => node.level === level).length,
  ])), { 1: 4, 2: 10, 3: 13, 4: 9, 5: 6, 6: 1 });
  assert.equal(definition.questlines.length, 12);
  assert.equal(new Set(definition.nodes.flatMap(node => node.gradingChecks.map(check => check.id))).size,
    157);
  assert.equal(definition.nodes.flatMap(node => node.gradingChecks)
    .reduce((total, check) => total + check.points, 0), 294);
  assert(definition.nodes.every(node => Object.keys(node.dependencyReasons).length
    === node.dependencies.length));
  assert(definition.questlines.every(questline =>
    definition.nodes.some(node => node.questline === questline.id)));

  const byId = new Map(definition.nodes.map(node => [node.id, node]));
  assert.deepEqual(requiredNode(byId, 'faceted-search').dependencies, ['catalog-discovery']);
  assert.deepEqual(requiredNode(byId, 'scheduled-restocks').dependencies, ['warehouse-admin']);
  assert.deepEqual(requiredNode(byId, 'price-history').dependencies,
    ['catalog-management']);
  assert.deepEqual(requiredNode(byId, 'warehouse-admin').dependencies, ['catalog', 'staff-access']);
  assert.deepEqual(requiredNode(byId, 'stock-transfers').dependencies, ['warehouse-admin']);
  assert.deepEqual(requiredNode(byId, 'catalog-management').dependencies,
    ['catalog-discovery', 'staff-roles']);
  assert.deepEqual(requiredNode(byId, 'warehouse-admin').gradingChecks.map(check => check.id), [
    'ecommerce.feature.warehouse-admin.admin-write.103a',
    'ecommerce.feature.warehouse-admin.warehouse-view.7b',
    'ecommerce.spec.access-control.warehouse-area-boundary.7a',
    'ecommerce.spec.access-control.warehouse-write-boundary.103b',
    'ecommerce.spec.external-data-sync.external-stock.901a',
    'ecommerce.spec.external-data-sync.external-stock.901c',
    'ecommerce.spec.external-data-sync.external-stock.901d',
    'ecommerce.spec.live-state.warehouse-stock.7c',
  ]);
  assert(requiredNode(byId, 'fulfilment-queue').gradingChecks.some(check =>
    check.id === 'ecommerce.spec.concurrency-safety.last-unit.201a'));
  assert(requiredNode(byId, 'fulfilment-queue').gradingChecks.some(check =>
    check.id === 'ecommerce.spec.concurrency-safety.restock-race.202a'));
  assert(requiredNode(byId, 'order-delivery').gradingChecks.some(check =>
    check.id === 'ecommerce.returns-pricing.cancellation-and-return.3d'));
  assert.deepEqual(requiredNode(byId, 'personalized-recommendations').dependencies,
    ['recommendations']);
  assert.deepEqual(requiredNode(byId, 'automatic-reorder').dependencies,
    ['purchasing', 'scheduled-restocks', 'staff-roles']);
  assert.deepEqual(requiredNode(byId, 'order-delivery').dependencies,
    ['fulfilment-queue', 'order-cancellation']);
  assert.deepEqual(requiredNode(byId, 'order-returns').dependencies,
    ['order-delivery']);
  assert.deepEqual(requiredNode(byId, 'support-refunds').dependencies,
    ['order-cancellation', 'order-support']);
});

test('every progression feature reference and scored check binds to repository data', () => {
  const packs: Array<[string, CompiledPackDefinition]> = readdirSync(packRoot)
    .filter(name => name.endsWith('.json')).map(name => {
    const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
      return [pack.id, pack];
  });
  const packByRef = new Map(packs);
  assert.equal(packByRef.size, packs.length, 'pack ids must be unique');

  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  for (const node of definition.nodes) {
    assert.deepEqual(node.promptModules, node.featureRefs);
    const owned = new Set(node.gradingChecks.map(check => check.id));
    for (const reference of node.featureRefs) {
      const pack = packByRef.get(reference);
      assert(pack, `${node.id} references missing ${reference}`);
      assert.equal(pack.moduleType, 'feature');
      assert(packChecks(pack).every(key => owned.has(key)),
        `${node.id} must own every check selected by ${reference}`);
    }
  }

  const recipe = compileRecipeFile(join(trackRoot, 'composition', 'recipes',
    'progression-catalog.json'), {
    trackRoot,
  });
  const actual = new Map(definition.nodes.flatMap(node => node.gradingChecks)
    .map(check => [check.id, check.points]));
  for (const check of recipe.checks.filter(item => item.points > 0)) {
    if (check.stableKey.startsWith('ecommerce.feature.catalog.catalog.')) continue;
    assert.equal(actual.get(check.stableKey), check.points,
      `progression must preserve ${check.stableKey} from the L3 candidate`);
  }
  assert.deepEqual([...actual.keys()].filter(key => key.startsWith('ecommerce.feature.catalog.')).sort(),
    ['ecommerce.feature.catalog.catalog-ranking.2b',
      'ecommerce.feature.catalog.catalog-search.2d',
      'ecommerce.feature.catalog.catalog-values.2a']);
});

test('signed-out purchase access does not depend on cart controls', () => {
  const pack = compilePackDefinition(
    readJson(join(packRoot, 'spec-access-control.json')),
    { source: 'spec-access-control.json' },
  );
  const purchase = pack.checks.find(check => check.id === 'signed-out-purchase');
  assert(purchase, 'access control must include signed-out-purchase');
  assert.deepEqual(purchase.requiresFeatures, ['ecommerce.feature.purchasing']);
  assert.equal(purchase.source, 'scenarios/progression-signed-out-purchase.json');

  const scenario = compileScenarioDefinition(readJson(join(trackRoot, purchase.source)), {
    source: purchase.source,
  });
  const criterion = scenario.features.find(feature => feature.id === purchase.feature)
    ?.criteria.find(candidate => candidate.id === '3a');
  assert(criterion, 'signed-out purchase must own criterion 3a');
  assert.equal(criterion.points, 1);
  assert.deepEqual(criterion.steps.map(step => step.testid), ['buy-now']);

  const cartBoundary = pack.checks.find(check => check.id === 'cart-boundary');
  assert(cartBoundary, 'access control must include cart-boundary');
  assert.deepEqual(cartBoundary.requiresFeatures, ['ecommerce.feature.cart']);
  assert.notEqual(cartBoundary.source, purchase.source);
});

test('cart isolation reads the action input from the acting customer', () => {
  const pack = compilePackDefinition(
    readJson(join(packRoot, 'spec-access-control.json')),
    { source: 'spec-access-control.json' },
  );
  const source = pack.checks.find(check => check.id === 'cart-boundary')?.source;
  assert(source, 'access control must include cart-boundary');
  const scenario = compileScenarioDefinition(readJson(join(trackRoot, source)), { source });
  const action = scenario.features.find(feature => feature.id === 109)
    ?.criteria.find(criterion => criterion.id === '109a')
    ?.steps.find(step => step.do === 'callAction');
  assert(action && action.do === 'callAction');
  assert.equal(action.actor, 'stranger');
  assert.equal(action.from, undefined);
});

test('every progression feature is a whole module and every direct graph edge is required', () => {
  const packByRef = new Map<string, CompiledPackDefinition>(readdirSync(packRoot)
    .filter(name => name.endsWith('.json'))
    .map(name => {
      const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
      return [pack.id, pack];
    }));
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const nodeById = new Map(definition.nodes.map(node => [node.id, node]));
  const ownerByRef = new Map(definition.nodes
    .flatMap(node => node.featureRefs.map(reference => [reference, node.id])));
  const ancestors = (nodeId: string): Set<string> => {
    const found = new Set<string>();
    const visit = (id: string): void => requiredNode(nodeById, id).dependencies.forEach(parent => {
      if (found.has(parent)) return;
      found.add(parent);
      visit(parent);
    });
    visit(nodeId);
    return found;
  };

  for (const node of definition.nodes) {
    const featurePacks = node.featureRefs.map(reference => packByRef.get(reference));
    const requiredPacks = featurePacks.map((pack, index) => {
      assert(pack, `${node.id} references missing ${node.featureRefs[index] ?? '<unknown>'}`);
      return pack;
    });
    for (const pack of requiredPacks) {
      assert.equal(pack.task.requirements.length, 1,
        `${node.id} must have one product prompt module`);
      assert.equal(pack.task.contracts.length, 1,
        `${node.id} must have one application interface module`);
      for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
        assert.equal(fragment.from, undefined, `${node.id} must not slice ${fragment.path}`);
        assert.equal(fragment.until, undefined, `${node.id} must not slice ${fragment.path}`);
      }
    }
    const requiredOwners = [...new Set(requiredPacks.flatMap(pack => pack.requiresPacks)
      .map(reference => ownerByRef.get(reference))
      .filter((owner): owner is string => typeof owner === 'string' && owner !== node.id))];
    const directRequiredOwners = requiredOwners.filter(owner => !requiredOwners.some(other =>
      other !== owner && ancestors(other).has(owner))).sort();
    assert.deepEqual(node.dependencies, directRequiredOwners,
      `${node.id} graph parents must be the minimal feature dependency set`);
  }
});

test('one progression catalog binds every node and selects only current work', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const input = compileFeatureCatalogInput(definition);
  const track = loadTrack('ecommerce');
  const bindings = [1, 2, 3, 4, 5, 6].map(level => ({
    level,
    binding: resolveRecipeRelease(track, level, 'ecommerce.progression-catalog'),
  }));
  validateProgressionRecipeBindings(input, bindings);
  assert.equal(new Set(bindings.map(item => item.binding.release.contentSha256)).size, 1);

  const allFeatureIds = new Set(definition.nodes.flatMap(node => node.featureRefs));
  const firstBinding = bindings[0];
  const secondBinding = bindings[1];
  assert(firstBinding && secondBinding, 'progression must bind levels 1 and 2');
  const release = firstBinding.binding.release;
  assert([...allFeatureIds].every(id => release.components.packs.some(pack => pack.id === id)));
  assert(definition.nodes.flatMap(node => node.gradingChecks)
    .every(check => release.checkCatalog.some(item => item.stableKey === check.id)));

  const policy = compileDependencyPolicyInput({ selection: 'feature',
    budget: { perFeature: 3 } }, input);
  let state = progressionEngine.initialize(dependencyRuntimeDefinition(input, policy));
  const first = resolveProgressionRecipeAction(firstBinding.binding, state);
  if (first.action.type === 'terminal') throw new Error('L1 must produce work');
  assert('agent' in first && 'grader' in first, 'L1 must have agent and grader selections');
  const firstFeatures = definition.nodes.filter(node => node.level === 1)
    .flatMap(node => node.featureRefs).sort();
  assert.equal(first.agent.request.task.mode, 'fresh');
  assert.deepEqual(first.agent.request.selection.requested.features, firstFeatures);
  assert.deepEqual([...first.grader.checkKeys].sort(),
    gradingCheckIds(first.action).sort());
  assert([...allFeatureIds].filter(id => !firstFeatures.includes(id))
    .every(id => !first.agent.request.selection.promptPacks.includes(id)));

  const grading = progressionEngine.gradingSelection(state);
  state = progressionEngine.recordResult(state, {
    attemptId: 'level-1-pass',
    outcome: 'conclusive',
    nodes: grading.nodeIds.map(nodeId => ({
      id: nodeId,
      checks: grading.checks.filter(check => check.nodeId === nodeId)
        .map(check => ({ id: check.id, outcome: 'pass' })),
    })),
  });
  const second = resolveProgressionRecipeAction(secondBinding.binding, state);
  if (second.action.type === 'terminal') throw new Error('L2 must produce work');
  assert('agent' in second && 'grader' in second, 'L2 must have agent and grader selections');
  const secondFeatures = definition.nodes.filter(node => node.level === 2)
    .flatMap(node => node.featureRefs).sort();
  assert.equal(second.action.level, 2);
  assert.equal(second.agent.request.task.mode, 'upgrade');
  assert.deepEqual(second.agent.request.selection.requested.features, secondFeatures);
  assert.deepEqual([...second.grader.checkKeys].sort(),
    gradingCheckIds(second.action).sort());
  const futureFeatures = definition.nodes.filter(node => node.level > 2)
    .flatMap(node => node.featureRefs).map(reference => reference.slice(0, reference.lastIndexOf('@')));
  assert(futureFeatures.every(id => !second.agent.request.selection.promptPacks.includes(id)));

  const secondGrading = progressionEngine.gradingSelection(state);
  state = progressionEngine.recordResult(state, {
    attemptId: 'level-2-account-regression',
    outcome: 'conclusive',
    nodes: secondGrading.nodeIds.map(nodeId => ({
      id: nodeId,
      checks: secondGrading.checks.filter(check => check.nodeId === nodeId)
        .map(check => ({ id: check.id, outcome: nodeId === 'accounts' ? 'fail' : 'pass' })),
    })),
  });
  const next = resolveProgressionRecipeAction(firstBinding.binding, state);
  if (next.action.type === 'terminal') throw new Error('the regression must produce repair work');
  assert('agent' in next, 'the repair must have an agent selection');
  assert.equal(next.action.type, 'repair');
  assert.equal(next.action.level, 1);
  assert.deepEqual(actionPromptNodeIds(next.action), ['accounts']);
  assert.equal(next.agent.request.task.mode, 'upgrade');
  assert.deepEqual(next.agent.request.selection.requested.features,
    definition.nodes.filter(node => actionPromptNodeIds(next.action).includes(node.id))
      .flatMap(node => node.featureRefs)
      .sort());
  const repairTarget = resolveProgressionRepairTarget(firstBinding.binding, state);
  assert.deepEqual(repairTarget.request.selection.requested.features,
    next.agent.request.selection.requested.features);
  assert(repairTarget.checkKeys.length > 0);
  assert(repairTarget.checkKeys.every(check => next.grader.checkKeys.includes(check)));
  assert(repairTarget.checkKeys.length < next.grader.checkKeys.length);
});

test('all-at-once composes every selected feature into one fresh request', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const catalog = compileFeatureCatalogInput(definition);
  const policy = compileDependencyPolicyInput({ selection: 'batch', budget: { total: 1 } },
    catalog, { workSelection: 'all-at-once' });
  const state = progressionEngine.initialize(dependencyRuntimeDefinition(catalog, policy));
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 6,
    'ecommerce.progression-catalog');
  const selected = resolveProgressionRecipeAction(binding, state);
  if (selected.action.type === 'terminal' || !('agent' in selected)) {
    throw new Error('all-at-once must produce one build request');
  }
  const featureIds = definition.nodes.flatMap(node => node.featureRefs)
    .sort();
  assert.equal(selected.action.level, 6);
  assert.equal(selected.agent.request.task.mode, 'fresh');
  assert.deepEqual(selected.agent.request.selection.requested.features, featureIds);
  assert(selected.agent.task.requirementIds.includes('ecommerce.progression.managed-support'));
});

test('the current campaign binds the full graph to one catalog across six levels', () => {
  const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'appliance',
    'campaign.ecommerce-progression-reference.json'));
  assert.deepEqual(plan.definition.levels, [1, 2, 3, 4, 5, 6]);
  assert(plan.featureCatalog, 'the campaign must compile its feature catalog');
  assert.equal(plan.featureCatalog.definition.nodes.length, 43);
  assert.equal(new Set(plan.bindings.map(binding => binding.recipe.contentSha256)).size, 1);
  const condition = plan.conditions[0];
  assert(condition, 'the campaign must have a condition');
  assert(condition.requested.levels.every(level =>
    typeof level.task.mode === 'string' && ['fresh', 'upgrade'].includes(level.task.mode)));
});

test('every feature and grading check binds to the progression recipe', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const recipePath = join(trackRoot, 'composition', 'recipes', 'progression-catalog.json');
  const plan = compileRecipeFile(recipePath, { trackRoot });
  const release = buildRecipeRelease(recipePath, { trackRoot });
  const binding: RecipeBinding = {
    alias: 'ecommerce.progression-catalog',
    selection: { path: 'composition/dependency.json', sha256: '0'.repeat(64) },
    recipePath, plan, release, execution: [],
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

test('every feature pack declares a bounded, testable product contract', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const plan = compileRecipeFile(join(trackRoot, 'composition', 'recipes',
    'progression-catalog.json'), { trackRoot });
  const featurePacks = plan.packs.filter(pack => pack.moduleType === 'feature');
  assert.equal(featurePacks.length, definition.nodes.length,
    'each graph node must own one feature pack');
  for (const pack of featurePacks) {
    assert(pack.capabilities.length > 0, `${pack.id} must declare its required capabilities`);
    assert(pack.evidence.length > 0, `${pack.id} must declare its required evidence`);
    assert.equal(pack.budget.status, 'bounded', `${pack.id} must have a bounded runtime`);
    assert(pack.budget.maxRuntimeMs > 0, `${pack.id} must have a positive runtime limit`);
  }
});

test('cross-feature grading requirements stay separate from product dependencies', () => {
  // A check may need a feature outside its node's ancestry; that is declared
  // grading scope, never a product edge, and the list is held explicit so it
  // cannot grow unnoticed.
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  const ownerByFeature = new Map(definition.nodes.flatMap(node => node.featureRefs
    .map(ref => [ref, node.id] as const)));
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
      return owner && !productAncestors.has(owner) ? [`${node.id}:${check.id}:${owner}`] : [];
    }));
  }).sort();
  assert.deepEqual(deferred, [
    'inventory-dashboard:ecommerce.spec.live-state.inventory-dashboard.5a:purchasing',
    'price-history:ecommerce.returns-pricing.price-history.4a:purchasing',
    'price-history:ecommerce.returns-pricing.price-history.4c:checkout',
    'sales-dashboard:ecommerce.spec.transactional-integrity.books-balance.107a:warehouse-admin',
    'sales-dashboard:ecommerce.spec.transactional-integrity.books-balance.107b:warehouse-admin',
    'stock-transfers:ecommerce.inventory-operations.stock-conservation.202d:purchasing',
  ]);
});

interface AuthoredNode {
  id: string;
  featureRefs: string[];
  gradingGroups: string[];
  dependencies: Array<{ id: string; reason: string }>;
}

const authoredDefinition = (): { nodes: AuthoredNode[] } =>
  JSON.parse(readFileSync(definitionPath, 'utf8')) as { nodes: AuthoredNode[] };

test('the authored graph rejects test-driven edges, missing packs, and stray ownership', async t => {
  const cases: Array<[string, (input: { nodes: AuthoredNode[] }) => void, RegExp]> = [
    ['a test-driven product edge', input => {
      const dashboard = input.nodes.find(node => node.id === 'inventory-dashboard');
      assert.ok(dashboard);
      dashboard.dependencies.push({ id: 'purchasing', reason: 'A grading scenario uses purchase data.' });
    }, /inventory-dashboard\.dependencies: must equal minimal product dependencies: warehouse-admin/],
    ['a missing product dependency', input => {
      const purchasing = input.nodes.find(node => node.id === 'purchasing');
      assert.ok(purchasing);
      purchasing.dependencies = purchasing.dependencies.filter(dependency => dependency.id !== 'accounts');
    }, /purchasing\.dependencies: must equal minimal product dependencies: accounts, catalog/],
    ['an unnecessary product dependency', input => {
      const reviews = input.nodes.find(node => node.id === 'reviews');
      assert.ok(reviews);
      reviews.dependencies.push({ id: 'accounts', reason: 'A grading scenario uses a customer.' });
    }, /reviews\.dependencies: must equal minimal product dependencies: purchasing/],
    ['a missing feature pack', input => {
      input.nodes[0]!.featureRefs = ['ecommerce.missing'];
    }, /missing pack/],
    ['a missing grading group', input => {
      input.nodes[0]!.gradingGroups[0] = 'ecommerce.feature.accounts#missing';
    }, /missing group/],
    ['feature checks omitted from their owner', input => {
      input.nodes[0]!.gradingGroups = input.nodes[0]!.gradingGroups
        .filter(reference => !reference.endsWith('#account-create'));
    }, /must own feature group/],
    ['a group owned twice', input => {
      input.nodes[1]!.gradingGroups.push(input.nodes[0]!.gradingGroups[0]!);
    }, /already owned/],
  ];
  for (const [name, mutate, expected] of cases) {
    await t.test(name, () => {
      const input = authoredDefinition();
      mutate(input);
      assert.throws(() => compileProgressionDefinition(input, { trackRoot }), expected);
    });
  }
});

function requiredNode(
  nodes: ReadonlyMap<string, CompiledProgressionNode>,
  nodeId: string,
): CompiledProgressionNode {
  const node = nodes.get(nodeId);
  if (!node) throw new Error(`progression node ${nodeId} is required`);
  return node;
}

function gradingCheckIds(action: ProgressionWorkAction): string[] {
  if (!isRecord(action.grading) || !Array.isArray(action.grading.checks)) {
    throw new Error('progression work must have grading checks');
  }
  return action.grading.checks.map((check, index) => {
    if (!isRecord(check) || typeof check.id !== 'string') {
      throw new Error(`progression grading check ${index} must have an id`);
    }
    return check.id;
  });
}

function actionPromptNodeIds(action: ProgressionWorkAction): string[] {
  if (!isRecord(action.prompt) || !Array.isArray(action.prompt.nodeIds)
    || !action.prompt.nodeIds.every(id => typeof id === 'string')) {
    throw new Error('progression work must have prompt node ids');
  }
  return action.prompt.nodeIds;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
