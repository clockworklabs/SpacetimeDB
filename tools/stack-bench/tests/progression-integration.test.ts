import assert from 'node:assert/strict';
import test from 'node:test';

import { loadTrack } from '../src/composition/tracks.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.js';
import type { RecipeBinding } from '../src/composition/recipe-release.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { compileProgressionInput, progressionLevels,
  validateProgressionInput } from '../src/progression/progression-definition.js';
import { resolveProgressionRecipeAction,
  validateProgressionRecipeBindings,
  type ProgressionRecipeAction } from '../src/progression/progression-recipe-selection.js';
import type { ProgressionState } from '../src/progression/progression-state.js';

interface DefinitionNode {
  id: string;
  title: string;
  questline: string;
  dependencies: Array<{ id: string; reason: string }>;
  featureRefs: string[];
  promptModules: string[];
  gradingChecks: Array<{ id: string; points: number }>;
}

interface DefinitionFixture {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  policy: string;
  strikes: { default: number; levels: Record<string, number> };
  nodes: DefinitionNode[];
  questlines: Array<{ id: string; title: string; nodes: string[] }>;
}

type WorkRecipeAction = Exclude<ProgressionRecipeAction,
  { action: { type: 'terminal' } }>;

function resolveWorkRecipeAction(binding: RecipeBinding,
  state: ProgressionState): WorkRecipeAction {
  const selected = resolveProgressionRecipeAction(binding, state);
  if (!('agent' in selected)) throw new Error('expected active progression work');
  return selected;
}

const definition = (): DefinitionFixture => ({
  schemaVersion: 3,
  kind: 'progression-mode',
  id: 'ecommerce-dependency',
  version: '1.0.0',
  state: 'draft',
  title: 'Ecommerce dependency fixture',
  policy: 'dependency-gated',
  strikes: { default: 2, levels: {} },
  nodes: [{
    id: 'accounts',
    title: 'Accounts',
    questline: 'identity',
    dependencies: [],
    featureRefs: ['ecommerce.feature.accounts@1.1.0'],
    promptModules: [],
    gradingChecks: [
      { id: 'ecommerce.feature.accounts.accounts.1a', points: 1 },
      { id: 'ecommerce.feature.accounts.accounts.1b', points: 1 },
    ],
  }],
  questlines: [{ id: 'identity', title: 'Identity', nodes: ['accounts'] }],
});

test('progression input freezes the compiled graph and rejects a rewritten identity', () => {
  const input = compileProgressionInput(definition());
  assert.deepEqual(progressionLevels(input), [1]);
  assert.deepEqual(validateProgressionInput(input), input);
  const changed = structuredClone(input);
  changed.identity.sha256 = 'a'.repeat(64);
  assert.throws(() => validateProgressionInput(changed), /identity does not match/);
});

test('the recipe boundary keeps agent work separate from exact grader checks', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.5.0');
  const input = compileProgressionInput(definition());
  validateProgressionRecipeBindings(input, [{ level: 1, binding }]);
  const state = progressionEngine.initialize(input.definition);
  const selected = resolveWorkRecipeAction(binding, state);

  assert.equal(selected.action.type, 'build');
  assert.deepEqual(selected.agent.request.selection.requested, {
    features: ['ecommerce.feature.accounts'],
    specifications: { expected: [], observed: [], requested: [] },
    checks: [],
    dependencyExpansion: 'exact',
  });
  assert.deepEqual(selected.grader.checkKeys, [
    'ecommerce.feature.accounts.accounts.1a',
    'ecommerce.feature.accounts.accounts.1b',
  ]);
  assert.deepEqual(selected.grader.request.selection.requested.checks,
    selected.grader.checkKeys);
  assert.deepEqual(selected.agent.request.selection.promptPacks,
    ['ecommerce.feature.accounts']);
});

test('dependent feature actions validate graph ancestors without restating their prompts', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.5.0');
  const value = definition();
  value.nodes = [
    { id: 'accounts', title: 'Accounts', questline: 'account-purchase', dependencies: [],
      featureRefs: ['ecommerce.feature.accounts@1.1.0'], promptModules: [],
      gradingChecks: [{ id: 'ecommerce.feature.accounts.accounts.1a', points: 1 }] },
    { id: 'catalog', title: 'Catalog', questline: 'catalog-purchase', dependencies: [],
      featureRefs: ['ecommerce.feature.catalog@1.1.0'], promptModules: [],
      gradingChecks: [{ id: 'ecommerce.feature.catalog.catalog.2a', points: 1 }] },
    { id: 'purchasing', title: 'Purchasing', questline: 'account-purchase',
      dependencies: [
        { id: 'accounts', reason: 'Purchasing requires an account.' },
        { id: 'catalog', reason: 'Purchasing requires a catalog item.' },
      ],
      featureRefs: ['ecommerce.feature.purchasing@1.1.0'], promptModules: [],
      gradingChecks: [{ id: 'ecommerce.feature.purchasing.purchase-order.3c', points: 1 }] },
  ];
  value.questlines = [
    { id: 'account-purchase', title: 'Account purchase', nodes: ['accounts', 'purchasing'] },
    { id: 'catalog-purchase', title: 'Catalog purchase', nodes: ['catalog'] },
  ];
  const input = compileProgressionInput(value);
  validateProgressionRecipeBindings(input, [
    { level: 1, binding }, { level: 2, binding },
  ]);
  let state = progressionEngine.initialize(input.definition);
  state = progressionEngine.recordResult(state, {
    attemptId: 'parents', outcome: 'conclusive', nodes: [
      { id: 'accounts', checks: [{ id: 'ecommerce.feature.accounts.accounts.1a', outcome: 'pass' }] },
      { id: 'catalog', checks: [{ id: 'ecommerce.feature.catalog.catalog.2a', outcome: 'pass' }] },
    ],
  });
  const selected = resolveWorkRecipeAction(binding, state);
  assert.deepEqual(selected.agent.selection.promptPacks, ['ecommerce.feature.purchasing']);
  assert.deepEqual(selected.agent.task.requirementIds, [
    'ecommerce.l1-modular.framing',
    'ecommerce.feature.purchasing.requirement',
  ]);
  assert.deepEqual(selected.agent.task.contractIds, [
    'ecommerce.l1-modular.interface',
    'ecommerce.feature.purchasing.hooks',
    'ecommerce.l1-modular.purchase-action-input',
  ]);

  const malformed = structuredClone(value);
  malformed.nodes[2]!.dependencies = [
    { id: 'accounts', reason: 'Purchasing requires an account.' },
  ];
  malformed.questlines = [
    { id: 'account-purchase', title: 'Account purchase', nodes: ['accounts', 'purchasing'] },
    { id: 'catalog-purchase', title: 'Catalog purchase', nodes: ['catalog'] },
  ];
  assert.throws(() => validateProgressionRecipeBindings(compileProgressionInput(malformed), [
    { level: 1, binding }, { level: 2, binding },
  ]), /requires ecommerce\.feature\.catalog@1\.1\.0 in its node or ancestors/);
});

test('binding validation rejects a check borrowed from a selected sibling', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.5.0');
  const value = definition();
  value.nodes.push({ id: 'catalog', title: 'Catalog', questline: 'catalog', dependencies: [],
    featureRefs: ['ecommerce.feature.catalog@1.1.0'], promptModules: [],
    gradingChecks: [{ id: 'ecommerce.feature.catalog.catalog.2b', points: 1 }] });
  value.nodes[0]!.gradingChecks = [
    { id: 'ecommerce.feature.catalog.catalog.2a', points: 1 },
  ];
  value.questlines = [
    { id: 'identity', title: 'Identity', nodes: ['accounts'] },
    { id: 'catalog', title: 'Catalog', nodes: ['catalog'] },
  ];
  assert.throws(() => validateProgressionRecipeBindings(compileProgressionInput(value), [
    { level: 1, binding },
  ]), /belongs to an unselected feature/);
});

test('binding validation rejects a grader check that needs a feature outside its path', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.5.0');
  const value = definition();
  value.nodes[0]!.gradingChecks.push({
    id: 'ecommerce.spec.state-durability.account-state-recovery.105a', points: 1,
  });
  assert.throws(() => validateProgressionRecipeBindings(compileProgressionInput(value), [
    { level: 1, binding },
  ]), /requires feature ecommerce\.feature\.cart-checkout outside the node and its ancestors/);
});

test('expected specification dependencies stay in grader scope and out of the agent prompt', () => {
  const binding = structuredClone(resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.5.0'));
  const durability = binding.release.components.packs.find(module =>
    module.id === 'ecommerce.spec.state-durability');
  assert(durability);
  durability.requiresPacks = ['ecommerce.spec.access-control@1.2.0'];
  const accessCheck = binding.release.checkCatalog.find(check =>
    check.packId === 'ecommerce.spec.access-control');
  assert(accessCheck);
  accessCheck.requiresFeatures = ['ecommerce.feature.accounts'];
  const value = definition();
  value.nodes[0]!.gradingChecks.push({
    id: 'ecommerce.spec.state-durability.session-reload.1e', points: 1,
  });
  const input = compileProgressionInput(value);
  validateProgressionRecipeBindings(input, [{ level: 1, binding }]);
  const selected = resolveWorkRecipeAction(binding,
    progressionEngine.initialize(input.definition));
  assert.deepEqual(selected.agent.request.selection.requested.specifications.requested, []);
  assert.deepEqual(selected.grader.request.selection.requested.specifications.expected, [
    'ecommerce.spec.access-control@1.2.0',
    'ecommerce.spec.state-durability@1.1.0',
  ]);
  assert.deepEqual(selected.grader.checkKeys, [
    'ecommerce.feature.accounts.accounts.1a',
    'ecommerce.feature.accounts.accounts.1b',
    'ecommerce.spec.state-durability.session-reload.1e',
  ]);
});

test('the recipe boundary rejects stale module versions, check points, and check owners', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.5.0');
  const stale = definition();
  stale.nodes[0]!.featureRefs = ['ecommerce.feature.accounts@1.0.0'];
  assert.throws(() => validateProgressionRecipeBindings(compileProgressionInput(stale),
    [{ level: 1, binding }]), /outside the selected recipe/);
  const wrongPoints = definition();
  wrongPoints.nodes[0]!.gradingChecks[0]!.points = 2;
  assert.throws(() => validateProgressionRecipeBindings(compileProgressionInput(wrongPoints),
    [{ level: 1, binding }]), /points.*differ/);
  const wrongOwner = definition();
  wrongOwner.nodes[0]!.gradingChecks[0]!.id = 'ecommerce.feature.catalog.catalog.2a';
  assert.throws(() => validateProgressionRecipeBindings(compileProgressionInput(wrongOwner),
    [{ level: 1, binding }]), /unselected feature/);
});
