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
import { compileProgressionDefinitionFile,
  compileDependencyPolicyInput, compileFeatureCatalogInput,
  dependencyRuntimeDefinition, type CompiledProgressionNode }
  from '../src/progression/progression-definition.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import type { ProgressionWorkAction } from '../src/progression/progression-engine.js';
import { resolveProgressionRecipeAction,
  validateProgressionRecipeBindings } from '../src/progression/progression-recipe-selection.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const definitionPath = join(trackRoot, 'progression', 'ecommerce-2.0.1.json');
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
    146);
  assert.equal(definition.nodes.flatMap(node => node.gradingChecks)
    .reduce((total, check) => total + check.points, 0), 281);
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
  assert(requiredNode(byId, 'warehouse-admin').gradingChecks.every(check =>
    !check.id.includes('spec.access-control.admin-ui')
    && !check.id.includes('spec.access-control.admin-write')
    && !check.id.includes('spec.live-state.warehouse-stock')
    && !check.id.includes('spec.concurrency-safety')));
  assert(requiredNode(byId, 'fulfilment-queue').gradingChecks.some(check =>
    check.id === 'ecommerce.spec.concurrency-safety.last-unit.201a'));
  assert(requiredNode(byId, 'fulfilment-queue').gradingChecks.some(check =>
    check.id === 'ecommerce.spec.concurrency-safety.restock-race.202a'));
  assert(requiredNode(byId, 'order-delivery').gradingChecks.some(check =>
    check.id === 'ecommerce.returns-pricing.cancellation-and-return.3d'));
  assert.deepEqual(requiredNode(byId, 'personalized-recommendations').dependencies,
    ['recommendations']);
  assert.deepEqual(requiredNode(byId, 'automatic-reorder').dependencies,
    ['scheduled-restocks', 'staff-roles']);
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
      return [`${pack.id}@${pack.version}`, pack];
  });
  const packByRef = new Map(packs);
  assert.equal(packByRef.size, packs.length, 'pack id and version pairs must be unique');

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
    'progression-catalog-2.0.1.json'), {
    trackRoot,
  });
  const actual = new Map(definition.nodes.flatMap(node => node.gradingChecks)
    .map(check => [check.id, check.points]));
  const movedChecks = new Map([
    ['ecommerce.spec.access-control.admin-ui.7a',
      'ecommerce.feature.warehouse-admin.access-boundary.7a'],
    ['ecommerce.spec.access-control.admin-write.103a',
      'ecommerce.feature.warehouse-admin.admin-write.103a'],
    ['ecommerce.spec.live-state.warehouse-stock.7c',
      'ecommerce.feature.warehouse-admin.warehouse-stock.7c'],
    ['ecommerce.operations-access.fulfilment-queue.1e',
      'ecommerce.operations-access.operator-authorization.201c'],
  ]);
  for (const check of recipe.checks.filter(item => item.points > 0)) {
    if (check.stableKey.startsWith('ecommerce.feature.catalog.catalog.')) continue;
    if (movedChecks.has(check.stableKey)) {
      const movedKey = movedChecks.get(check.stableKey);
      assert(movedKey, `${check.stableKey} must have a moved key`);
      assert.equal(actual.get(movedKey), check.points,
        `progression must preserve ${check.stableKey} under its feature owner`);
      continue;
    }
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
    readJson(join(packRoot, 'spec-access-control-2.0.0.json')),
    { source: 'spec-access-control-2.0.0.json' },
  );
  const purchase = pack.checks.find(check => check.id === 'signed-out-purchase');
  assert(purchase, 'access control must include signed-out-purchase');
  assert.deepEqual(purchase.requiresFeatures, ['ecommerce.feature.purchasing']);
  assert.equal(purchase.source, 'scenarios/progression-signed-out-purchase-1.0.0.json');

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

test('every progression feature is a whole module and every direct graph edge is required', () => {
  const packByRef = new Map<string, CompiledPackDefinition>(readdirSync(packRoot)
    .filter(name => name.endsWith('.json'))
    .map(name => {
      const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
      return [`${pack.id}@${pack.version}`, pack];
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
        `${node.id} must have one testing interface module`);
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
    binding: resolveRecipeRelease(track, level, 'ecommerce.progression-catalog@2.0.1'),
  }));
  validateProgressionRecipeBindings(input, bindings);
  assert.equal(new Set(bindings.map(item => item.binding.release.contentSha256)).size, 1);

  const allFeatureIds = new Set(definition.nodes.flatMap(node => node.featureRefs)
    .map(reference => reference.slice(0, reference.lastIndexOf('@'))));
  const firstBinding = bindings[0];
  const secondBinding = bindings[1];
  assert(firstBinding && secondBinding, 'progression must bind levels 1 and 2');
  const release = firstBinding.binding.release;
  assert([...allFeatureIds].every(id => release.components.packs.some(pack => pack.id === id)));
  assert(definition.nodes.flatMap(node => node.gradingChecks)
    .every(check => release.checkCatalog.some(item => item.stableKey === check.id)));

  const policy = compileDependencyPolicyInput({ default: 3, levels: {} }, input);
  let state = progressionEngine.initialize(dependencyRuntimeDefinition(input, policy));
  const first = resolveProgressionRecipeAction(firstBinding.binding, state);
  if (first.action.type === 'terminal') throw new Error('L1 must produce work');
  assert('agent' in first && 'grader' in first, 'L1 must have agent and grader selections');
  const firstFeatures = definition.nodes.filter(node => node.level === 1)
    .flatMap(node => node.featureRefs).map(reference => reference.slice(0, reference.lastIndexOf('@')))
    .sort();
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
    .flatMap(node => node.featureRefs).map(reference => reference.slice(0, reference.lastIndexOf('@')))
    .sort();
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
      .map(reference => reference.slice(0, reference.lastIndexOf('@'))).sort());
});

test('all-at-once composes every selected feature into one fresh request', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const catalog = compileFeatureCatalogInput(definition);
  const policy = compileDependencyPolicyInput({ default: 1, levels: {} }, catalog, {
    workSelection: 'all-at-once', repairSelection: 'batch',
  });
  const state = progressionEngine.initialize(dependencyRuntimeDefinition(catalog, policy));
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 6,
    'ecommerce.progression-catalog@2.0.1');
  const selected = resolveProgressionRecipeAction(binding, state);
  if (selected.action.type === 'terminal' || !('agent' in selected)) {
    throw new Error('all-at-once must produce one build request');
  }
  const featureIds = definition.nodes.flatMap(node => node.featureRefs)
    .map(reference => reference.slice(0, reference.lastIndexOf('@'))).sort();
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
