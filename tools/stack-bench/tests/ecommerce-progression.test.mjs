import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { compileCampaignFile } from '../src/campaigns/campaign-compiler.mjs';
import { compilePackDefinition, compileRecipeFile } from '../src/composition/composition-compiler.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';
import { compileProgressionDefinitionFile,
  compileProgressionInput } from '../src/progression/progression-definition.mjs';
import { compileProgressionGraph,
  renderProgressionGraphHtml } from '../src/progression/progression-graph.mjs';
import { progressionEngine } from '../src/progression/progression-engine.mjs';
import { resolveProgressionRecipeAction,
  validateProgressionRecipeBindings } from '../src/progression/progression-recipe-selection.mjs';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const definitionPath = join(trackRoot, 'progression', 'ecommerce-1.0.0.json');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));

function packChecks(pack) {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    const criteria = check.criteria === undefined
      ? feature.criteria
      : check.criteria.map(id => feature.criteria.find(criterion => criterion.id === id));
    return criteria.map(criterion =>
      `${pack.stableId ?? pack.id}.${check.stableId ?? check.id}.${criterion.id}`);
  });
}

test('the ecommerce progression definition is complete and calculated from its dependencies', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  assert.equal(definition.nodes.length, 39);
  assert.deepEqual(Object.fromEntries([1, 2, 3, 4, 5].map(level => [
    level,
    definition.nodes.filter(node => node.level === level).length,
  ])), { 1: 4, 2: 10, 3: 12, 4: 8, 5: 5 });
  assert.equal(definition.questlines.length, 10);
  assert.equal(new Set(definition.nodes.flatMap(node => node.gradingChecks.map(check => check.id))).size,
    142);
  assert.equal(definition.nodes.flatMap(node => node.gradingChecks)
    .reduce((total, check) => total + check.points, 0), 277);
  assert(definition.nodes.every(node => Object.keys(node.dependencyReasons).length
    === node.dependencies.length));
  assert(definition.questlines.every(questline =>
    definition.nodes.filter(node => node.questline === questline.id).length >= 2));

  const byId = new Map(definition.nodes.map(node => [node.id, node]));
  assert.deepEqual(byId.get('faceted-search').dependencies, ['catalog']);
  assert.deepEqual(byId.get('scheduled-restocks').dependencies, ['warehouse-admin']);
  assert.deepEqual(byId.get('price-history').dependencies,
    ['cart-checkout', 'catalog-management', 'purchasing']);
  assert.deepEqual(byId.get('warehouse-admin').dependencies, ['catalog', 'staff-access']);
  assert(byId.get('warehouse-admin').gradingChecks.every(check =>
    !check.id.includes('spec.access-control.admin-ui')
    && !check.id.includes('spec.access-control.admin-write')
    && !check.id.includes('spec.live-state.warehouse-stock')
    && !check.id.includes('spec.concurrency-safety')));
  assert(byId.get('fulfilment-queue').gradingChecks.some(check =>
    check.id === 'ecommerce.spec.concurrency-safety.last-unit.201a'));
  assert(byId.get('fulfilment-queue').gradingChecks.some(check =>
    check.id === 'ecommerce.spec.concurrency-safety.restock-race.202a'));
  assert(byId.get('order-delivery').gradingChecks.some(check =>
    check.id === 'ecommerce.returns-pricing.cancellation-and-return.3d'));
  assert.deepEqual(byId.get('personalized-recommendations').dependencies,
    ['operational-views', 'purchasing']);
  assert.deepEqual(byId.get('order-delivery').dependencies,
    ['fulfilment-queue', 'order-cancellation']);
  assert.deepEqual(byId.get('order-returns').dependencies,
    ['order-delivery', 'warehouse-admin']);
  assert.deepEqual(byId.get('support-refunds').dependencies,
    ['order-cancellation', 'order-support']);
});

test('every progression feature reference and scored check binds to repository data', () => {
  const packs = readdirSync(packRoot).filter(name => name.endsWith('.json')).map(name => {
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

  const recipe = compileRecipeFile(join(trackRoot, 'composition', 'recipes', 'l3-standard-1.0.0.json'), {
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
  ]);
  for (const check of recipe.checks.filter(item => item.points > 0)) {
    if (check.stableKey.startsWith('ecommerce.feature.catalog.catalog.')) continue;
    if (check.stableKey === 'ecommerce.returns-pricing.cancellation-and-return.3a') {
      assert.equal(actual.get(check.stableKey)
        + actual.get('ecommerce.returns-pricing.cancellation-and-return.3d'), check.points,
      'progression must preserve cancellation points after separating queue integration');
      continue;
    }
    if (movedChecks.has(check.stableKey)) {
      assert.equal(actual.get(movedChecks.get(check.stableKey)), check.points,
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

test('one progression catalog binds every node and selects only current work', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  const input = compileProgressionInput(definition);
  const track = loadTrack('ecommerce');
  const bindings = [1, 2, 3, 4, 5].map(level => ({
    level,
    binding: resolveRecipeRelease(track, level, 'ecommerce.progression-catalog@1.0.0'),
  }));
  validateProgressionRecipeBindings(input, bindings);
  assert.equal(new Set(bindings.map(item => item.binding.release.contentSha256)).size, 1);

  const allFeatureIds = new Set(definition.nodes.flatMap(node => node.featureRefs)
    .map(reference => reference.slice(0, reference.lastIndexOf('@'))));
  const release = bindings[0].binding.release;
  assert([...allFeatureIds].every(id => release.components.packs.some(pack => pack.id === id)));
  assert(definition.nodes.flatMap(node => node.gradingChecks)
    .every(check => release.checkCatalog.some(item => item.stableKey === check.id)));

  let state = progressionEngine.initialize(input.definition);
  const first = resolveProgressionRecipeAction(bindings[0].binding, state);
  const firstFeatures = definition.nodes.filter(node => node.level === 1)
    .flatMap(node => node.featureRefs).map(reference => reference.slice(0, reference.lastIndexOf('@')))
    .sort();
  assert.equal(first.agent.request.task.mode, 'fresh');
  assert.deepEqual(first.agent.request.selection.requested.features, firstFeatures);
  assert.deepEqual([...first.grader.checkKeys].sort(),
    first.action.grading.checks.map(check => check.id).sort());
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
  const second = resolveProgressionRecipeAction(bindings[1].binding, state);
  const secondFeatures = definition.nodes.filter(node => node.level === 2)
    .flatMap(node => node.featureRefs).map(reference => reference.slice(0, reference.lastIndexOf('@')))
    .sort();
  assert.equal(second.action.level, 2);
  assert.equal(second.agent.request.task.mode, 'upgrade');
  assert.deepEqual(second.agent.request.selection.requested.features, secondFeatures);
  assert.deepEqual([...second.grader.checkKeys].sort(),
    second.action.grading.checks.map(check => check.id).sort());
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
  const repair = resolveProgressionRecipeAction(bindings[1].binding, state);
  assert.equal(repair.action.type, 'repair');
  assert.equal(repair.agent.request.task.mode, 'upgrade');
  assert(repair.agent.request.selection.requested.features.includes('ecommerce.feature.accounts'));
  assert(repair.agent.task.requirementIds.includes('ecommerce.feature.accounts.requirement'));
});

test('a campaign can bind the full graph to one catalog across five levels', t => {
  const directory = mkdtempSync(join(tmpdir(), 'stack-bench-full-graph-campaign-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const campaign = readJson(join(import.meta.dirname, 'fixtures',
    'dependency-model-free-campaign.json'));
  campaign.id = 'full-graph-catalog-proof';
  campaign.title = 'Full graph catalog proof';
  campaign.progression = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  campaign.selection.levels = [1, 2, 3, 4, 5].map(level => ({
    level,
    recipe: 'ecommerce.progression-catalog@1.0.0',
  }));
  const path = join(directory, 'campaign.json');
  writeFileSync(path, `${JSON.stringify(campaign, null, 2)}\n`);
  const plan = compileCampaignFile(path);
  assert.deepEqual(plan.definition.levels, [1, 2, 3, 4, 5]);
  assert.equal(plan.progression.definition.nodes.length, 39);
  assert.equal(new Set(plan.bindings.map(binding => binding.recipe.contentSha256)).size, 1);
  assert(plan.conditions[0].requested.levels.every(level =>
    ['fresh', 'upgrade'].includes(level.task.mode)));
});

test('the dependency graph page is generated from the ecommerce definition', () => {
  const htmlPath = join(import.meta.dirname, '..', 'docs', 'dependency-graph.html');
  const html = readFileSync(htmlPath, 'utf8');
  const graph = compileProgressionGraph(definitionPath, { trackRoot });
  assert.equal(graph.nodes.length, 39);
  assert.equal(graph.levels, 5);
  assert.deepEqual(graph.nodes.filter(node => node.level === 1).map(node => node.id),
    ['accounts', 'catalog', 'staff-access', 'support-intake']);
  assert.equal(renderProgressionGraphHtml(html, graph), html,
    'run npm run graph after changing the progression definition');
});
