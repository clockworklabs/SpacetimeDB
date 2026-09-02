import assert from 'node:assert/strict';
import test from 'node:test';

import {
  type ConclusiveResult,
  type DependencyGradingSelection,
  type DependencyPromptSelection,
} from '../src/progression/dependency-mode.js';
import { compileDependencyMode } from '../src/progression/dependency-definition.js';
import type { DependencyScore } from '../src/progression/dependency-score.js';
import { compileDependencyPolicyInput, compileFeatureCatalogInput }
  from '../src/progression/progression-definition.js';
import {
  createProgressionEngine,
  progressionEngine,
  type ProgressionWorkAction,
} from '../src/progression/progression-engine.js';
import type { ProgressionState } from '../src/progression/progression-state.js';

interface FixtureDependency {
  id: string;
  reason: string;
}

interface FixtureNode {
  id: string;
  title: string;
  questline: string;
  dependencies: FixtureDependency[];
  featureRefs: string[];
  promptModules: string[];
  gradingChecks: Array<{ id: string; points: number; role: 'feature' | 'guarantee';
    requiresFeatures?: string[] }>;
  level?: number;
}

interface FixtureDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  policy: string;
  strikes: { default?: number; levels: Record<number, number> };
  unchangedFailureLimit?: number;
  repairSelection?: 'feature' | 'batch';
  strikePolicy?: 'feature' | 'depth' | 'banked';
  workSelection?: 'progressive' | 'all-at-once';
  nodes: FixtureNode[];
  questlines: Array<{ id: string; title: string }>;
}

type Outcome = 'pass' | 'fail' | 'not-run';
type Outcomes = Record<string, Outcome | Record<string, Outcome>>;

const dependencyPrompt = (state: ProgressionState): DependencyPromptSelection =>
  progressionEngine.promptSelection(state) as DependencyPromptSelection;
const dependencyGrading = (state: ProgressionState): DependencyGradingSelection =>
  progressionEngine.gradingSelection(state) as DependencyGradingSelection;
const dependencyScore = (state: ProgressionState): DependencyScore =>
  progressionEngine.score(state);
const dependencyAction = (state: ProgressionState): ProgressionWorkAction => {
  const action = progressionEngine.nextAction(state);
  if (action.type === 'terminal') throw new Error('expected active dependency work');
  return action;
};

const node = (id: string, dependencies: string[], questline: string,
  points: number[] = [1]): FixtureNode => ({
  id,
  title: id,
  questline,
  dependencies: dependencies.map(dependency => ({
    id: dependency,
    reason: `${id} requires ${dependency}`,
  })),
  featureRefs: [`features.${id}@1.0.0`],
  promptModules: [`prompt.${id}@1.0.0`],
  gradingChecks: points.map((value, index) => ({ id: `check.${id}.${index + 1}`, points: value,
    role: 'feature' })),
});

const fixture = (): FixtureDefinition => ({
  schemaVersion: 4,
  kind: 'progression-mode',
  id: 'storefront-paths',
  version: '1.0.0',
  state: 'draft',
  title: 'Storefront paths',
  policy: 'dependency-graph',
  strikes: { default: 2, levels: { 2: 1 } },
  nodes: [
    node('accounts', [], 'identity', [2, 1]),
    node('catalog', [], 'discovery'),
    node('ownership', ['accounts'], 'identity'),
    node('search', ['catalog'], 'discovery'),
    node('recovery', ['ownership'], 'identity'),
    node('recommendations', ['search'], 'discovery'),
  ],
  questlines: [
    { id: 'identity', title: 'Identity' },
    { id: 'discovery', title: 'Discovery' },
  ],
});

function conclusive(state: ProgressionState, attemptId: string,
  outcomes: Outcomes): ConclusiveResult {
  const selection = dependencyGrading(state);
  return {
    attemptId,
    outcome: 'conclusive',
    nodes: selection.nodeIds.map(nodeId => ({
      id: nodeId,
      checks: selection.checks.filter(check => check.nodeId === nodeId).map(check => ({
        id: check.id,
        outcome: typeof outcomes[nodeId] === 'string'
          ? outcomes[nodeId] : outcomes[nodeId]?.[check.id] ?? 'pass',
      })),
    })),
  };
}

test('a valid graph compiles in deterministic level and id order', () => {
  const input = fixture();
  input.nodes.reverse();
  input.questlines.reverse();
  input.version = '1.0.0-rc.1+build.7';
  const compiled = compileDependencyMode(input);
  assert.equal(compiled.version, input.version);
  assert.deepEqual(compiled.nodes.map(item => item.id), [
    'accounts', 'catalog', 'ownership', 'search', 'recommendations', 'recovery',
  ]);
  assert.deepEqual(compiled.questlines.map(item => item.id), ['discovery', 'identity']);
  assert.deepEqual(compiled.questlines.find(item => item.id === 'identity')!.nodes,
    ['accounts', 'ownership', 'recovery']);
  assert.deepEqual(compiled.strikes, { levels: { 1: 2, 2: 1, 3: 2 } });
  assert.equal(compiled.unchangedFailureLimit, 3);
  assert.equal(compiled.repairSelection, 'feature');
  assert.equal(compiled.strikePolicy, 'feature');
  assert.equal(compiled.workSelection, 'progressive');
});

test('repair selection and the unchanged failure limit are versioned dependency policy inputs', () => {
  const runtime = compileDependencyMode(fixture());
  const { policy: _policy, strikes, unchangedFailureLimit: _limit,
    repairSelection: _repairSelection, strikePolicy: _strikePolicy,
    workSelection: _workSelection,
    ...catalogDefinition } = runtime;
  const catalog = compileFeatureCatalogInput({
    ...catalogDefinition,
    schemaVersion: 1,
    kind: 'feature-catalog',
  });
  const first = compileDependencyPolicyInput({ levels: strikes.levels }, catalog,
    { unchangedFailureLimit: 2 });
  const second = compileDependencyPolicyInput({ levels: strikes.levels }, catalog,
    { unchangedFailureLimit: 3 });
  const batch = compileDependencyPolicyInput({ levels: strikes.levels }, catalog,
    { unchangedFailureLimit: 2, repairSelection: 'batch' });
  assert.equal(first.definition.version, '3.2.0');
  assert.equal(first.definition.unchangedFailureLimit, 2);
  assert.equal(first.definition.repairSelection, 'feature');
  assert.equal(batch.definition.repairSelection, 'batch');
  assert.equal(second.definition.unchangedFailureLimit, 3);
  assert.notEqual(first.identity.sha256, second.identity.sha256);
  assert.notEqual(first.identity.sha256, batch.identity.sha256);
});

test('invalid graphs, questlines, and strike budgets fail before execution', async t => {
  const cases: Array<[string, (value: FixtureDefinition) => void, RegExp]> = [
    ['duplicate node', value => value.nodes.push(structuredClone(value.nodes[0]!)), /duplicates/],
    ['missing parent', value => { value.nodes[2]!.dependencies[0]!.id = 'missing'; }, /unknown parent/],
    ['dependency cycle', value => {
      value.nodes[0]!.dependencies = [{ id: 'ownership', reason: 'cycle' }];
    }, /dependency cycle/],
    ['missing dependency reason', value => { value.nodes[2]!.dependencies[0]!.reason = ''; }, /non-empty string/],
    ['authored level', value => { value.nodes[0]!.level = 1; }, /compiled level and dependency reasons/],
    ['invalid default strikes', value => { value.strikes.default = 0; }, /positive integer/],
    ['invalid level strikes', value => { value.strikes.levels[2] = -1; }, /positive integer/],
    ['invalid unchanged failure limit', value => { value.unchangedFailureLimit = 0; },
      /positive integer/],
    ['invalid repair selection', value => {
      value.repairSelection = 'invalid' as 'feature';
    }, /feature.*batch/],
    ['invalid work selection', value => {
      value.workSelection = 'invalid' as 'progressive';
    }, /progressive.*all-at-once/],
    ['negative check points', value => { value.nodes[0]!.gradingChecks[0]!.points = -1; }, /positive integer/],
    ['zero-point gate check', value => {
      value.nodes[0]!.gradingChecks[0]!.points = 0;
    }, /positive integer/],
    ['missing feature check', value => {
      value.nodes[0]!.gradingChecks.forEach(check => { check.role = 'guarantee'; });
    }, /at least one feature check/],
    ['duplicate feature owner', value => {
      value.nodes[1]!.featureRefs = ['features.accounts@2.0.0'];
    }, /features\.accounts is already owned by accounts/],
    ['empty feature requirements', value => {
      value.nodes[0]!.gradingChecks[0]!.requiresFeatures = [];
    }, /non-empty array/],
    ['invalid semantic version', value => { value.version = '1.0.0-01'; }, /exact semantic version/],
    ['missing budget', value => { value.strikes = { levels: { 1: 1 } }; }, /is required/],
    ['unknown questline', value => { value.nodes[0]!.questline = 'missing'; }, /unknown questline/],
    ['empty questline', value => { value.nodes.forEach(item => { item.questline = 'identity'; }); }, /owns no nodes/],
  ];
  for (const [name, mutate, expected] of cases) {
    await t.test(name, () => {
      const value = fixture();
      mutate(value);
      assert.throws(() => compileDependencyMode(value), expected);
    });
  }
});

test('the engine dispatches modes through registered policies without mode conditionals', () => {
  const fake = {
    id: 'fake',
    compile: (value: Record<string, unknown>) => value,
    initialize: (value: Record<string, unknown>) => ({ policy: value.policy }),
    activeNodes: () => ['fake'],
    promptSelection: () => ({ fake: 'prompt' }),
    gradingSelection: () => ({ fake: 'grade' }),
    recordResult: (state: Record<string, unknown>) => state,
    grantStrikes: (state: Record<string, unknown>) => state,
    replay: (definition: Record<string, unknown>) => ({ policy: definition.policy }),
    nextAction: () => ({ type: 'complete' }),
    score: () => ({ score: 1 }),
  };
  const engine = createProgressionEngine([fake]);
  const state = engine.initialize({ policy: 'fake' });
  assert.deepEqual(engine.activeNodes(state), ['fake']);
  assert.deepEqual(engine.promptSelection(state), { fake: 'prompt' });
  assert.deepEqual(engine.gradingSelection(state), { fake: 'grade' });
  assert.throws(() => engine.initialize({ policy: 'missing' }), /unknown progression policy/);
});

test('prompt work contains only unlocked nodes while grading also contains their dependencies', () => {
  let state = progressionEngine.initialize(fixture());
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts', 'catalog']);
  assert.deepEqual(dependencyPrompt(state).promptModules,
    ['prompt.accounts@1.0.0', 'prompt.catalog@1.0.0']);
  assert.doesNotMatch(JSON.stringify(dependencyPrompt(state)), /ownership|recovery|search/);

  state = progressionEngine.recordResult(state, conclusive(state, 'level-1', {
    accounts: 'pass',
    catalog: 'fail',
  }));
  assert.equal(dependencyAction(state).type, 'repair');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['catalog']);
  state = progressionEngine.recordResult(state, conclusive(state, 'level-1-repair', { catalog: 'fail' }));
  assert.equal(state.nodes.catalog!.status, 'failed');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['ownership']);
  state = progressionEngine.recordResult(state, conclusive(state, 'ownership', { ownership: 'pass' }));
  state = progressionEngine.recordResult(state, conclusive(state, 'recovery', { recovery: 'pass' }));
  assert.equal(state.phase, 'terminal');
  assert.equal(state.nodes.recovery!.status, 'passed');
});

test('a production failure keeps its child ready while the feature is repaired', () => {
  const value = fixture();
  value.strikes.default = 2;
  value.nodes.find(item => item.id === 'accounts')!.gradingChecks[1]!.role = 'guarantee';
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'first-grade', {
    accounts: { 'check.accounts.1': 'pass', 'check.accounts.2': 'fail' },
    catalog: 'pass',
  }));
  assert.equal(state.nodes.accounts!.status, 'working');
  assert.equal(state.nodes.ownership!.status, 'active');
  assert.equal(state.nodes.accounts!.strikes.used, 1);
  assert.equal(dependencyAction(state).type, 'repair');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  const score = dependencyScore(state);
  assert.equal(score.questlines.find(item => item.id === 'identity')!.failedPoints, 1);
  assert.equal(score.questlines.find(item => item.id === 'identity')!.availablePoints, 5);
});

test('a failed feature check blocks its product child', () => {
  const value = fixture();
  value.strikes.default = 1;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'first-grade', {
    accounts: 'fail', catalog: 'pass',
  }));
  assert.equal(state.nodes.accounts!.status, 'failed');
  assert.equal(state.nodes.ownership!.status, 'blocked');
  assert.equal(state.nodes.search!.status, 'active');
  assert.equal(dependencyScore(state).questlines.find(item => item.id === 'identity')!.blockedPoints, 2);
});

test('feature repairs isolate prompt work, strikes, and repeated-failure tracking', () => {
  const value = fixture();
  value.strikes.default = 3;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'initial', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.equal(state.nodes.accounts!.strikes.used, 1);
  assert.equal(state.nodes.catalog!.strikes.used, 1);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  assert.deepEqual(dependencyGrading(state).nodeIds, ['accounts']);

  state = progressionEngine.recordResult(state, conclusive(state, 'repair-accounts', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.equal(state.nodes.accounts!.strikes.used, 2);
  assert.equal(state.nodes.catalog!.strikes.used, 1);
  assert.equal(state.nodes.accounts!.unchangedFailure.count, 2);
  assert.equal(state.nodes.catalog!.unchangedFailure.count, 1);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);

  state = progressionEngine.recordResult(state, conclusive(state, 'fix-accounts', {
    accounts: 'pass', catalog: 'fail',
  }));
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['catalog']);
  assert.equal(state.nodes.catalog!.strikes.used, 1);
  state = progressionEngine.recordResult(state, conclusive(state, 'fix-catalog', { catalog: 'pass' }));
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['ownership', 'search']);
});

test('batch repairs keep all failed features in one repair action', () => {
  const value = fixture();
  value.repairSelection = 'batch';
  value.strikes.default = 3;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'initial', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts', 'catalog']);
  state = progressionEngine.recordResult(state, conclusive(state, 'batch-repair', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.equal(state.nodes.accounts!.strikes.used, 2);
  assert.equal(state.nodes.catalog!.strikes.used, 2);
});

test('all-at-once requests and grades every selected feature without dependency blocking', () => {
  const value = fixture();
  value.workSelection = 'all-at-once';
  value.repairSelection = 'batch';
  let state = progressionEngine.initialize(value);
  assert.equal(dependencyAction(state).level, 3);
  assert.deepEqual(dependencyPrompt(state).nodeIds,
    ['accounts', 'catalog', 'ownership', 'search', 'recommendations', 'recovery']);
  assert.deepEqual(dependencyGrading(state).nodeIds, dependencyPrompt(state).nodeIds);

  state = progressionEngine.recordResult(state, conclusive(state, 'full-grade', {
    accounts: 'fail', catalog: 'pass', ownership: 'pass', search: 'pass',
    recommendations: 'pass', recovery: 'pass',
  }));
  assert.equal(state.nodes.ownership!.status, 'passed');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  assert.deepEqual(dependencyGrading(state).nodeIds,
    ['accounts', 'catalog', 'ownership', 'search', 'recommendations', 'recovery']);
  assert.equal(dependencyAction(state).level, 3);
  state = progressionEngine.recordResult(state, conclusive(state, 'full-repair', {
    accounts: 'fail',
  }));
  assert.equal(state.phase, 'terminal');
  assert.equal(state.nodes.accounts!.status, 'failed');
  assert.equal(state.nodes.ownership!.status, 'passed');
});

test('all-at-once regrades checks owned by failures outside a feature-scoped repair', () => {
  const value = fixture();
  value.workSelection = 'all-at-once';
  value.repairSelection = 'feature';
  value.nodes.find(node => node.id === 'search')!.gradingChecks[0]!.requiresFeatures =
    ['features.catalog'];
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'full-grade', {
    accounts: 'fail', catalog: 'fail', ownership: 'pass', search: 'pass',
    recommendations: 'pass', recovery: 'pass',
  }));
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  assert(dependencyGrading(state).checks.some(check => check.id === 'check.search.1'));
});

test('depth strikes are shared while repair prompts remain feature-scoped', () => {
  const value = fixture();
  value.strikePolicy = 'depth';
  value.strikes.default = 3;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'initial', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.equal(dependencyAction(state).strikes.scope, 'depth');
  assert.equal(state.nodes.accounts!.strikes.used, 1);
  assert.equal(state.nodes.catalog!.strikes.used, 1);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  state = progressionEngine.recordResult(state, conclusive(state, 'accounts-fail', {
    accounts: 'fail',
  }));
  assert.equal(state.nodes.accounts!.strikes.used, 2);
  assert.equal(state.nodes.catalog!.strikes.used, 2);
  assert.equal(state.nodes.accounts!.status, 'active');
  assert.equal(state.nodes.catalog!.status, 'active');
  state = progressionEngine.recordResult(state, conclusive(state, 'accounts-fail-again', {
    accounts: 'fail',
  }));
  assert.equal(state.phase, 'terminal');
  assert.equal(state.nodes.accounts!.status, 'failed');
  assert.equal(state.nodes.catalog!.status, 'failed');
});

test('banked strikes carry unused budget into later depths', () => {
  const value = fixture();
  value.strikePolicy = 'banked';
  value.strikes.default = 3;
  value.strikes.levels = {};
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'initial', {
    accounts: 'pass', catalog: 'fail',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'catalog-pass', {
    catalog: 'pass',
  }));
  const action = dependencyAction(state);
  assert.equal(action.level, 2);
  assert.equal(action.strikes.scope, 'banked');
  assert(action.strikes.nodes.every(item => item.budget === 6 && item.used === 1
    && item.remaining === 5));
});

test('passed siblings are regraded and return to repair work if they regress', () => {
  const value = fixture();
  value.strikes.default = 3;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'first-grade', {
    accounts: 'pass', catalog: 'fail',
  }));
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['catalog']);
  state = progressionEngine.recordResult(state, conclusive(state, 'catalog-repair', {
    catalog: 'pass',
  }));
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['ownership', 'search']);
  state = progressionEngine.recordResult(state, conclusive(state, 'sibling-regression', {
    accounts: 'fail', catalog: 'pass', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.nodes.accounts!.status, 'active');
  assert.equal(state.nodes.catalog!.status, 'passed');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  assert.deepEqual(dependencyGrading(state).nodeIds,
    ['accounts', 'catalog', 'search']);
});

test('an application startup failure spends strikes only on current work', () => {
  const value = fixture();
  value.strikes.default = 3;
  value.strikes.levels = {};
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'first-grade', {
    accounts: 'pass', catalog: 'fail',
  }));
  const guardStrikes = state.nodes.accounts!.strikes.used;
  state = progressionEngine.recordResult(state, {
    ...conclusive(state, 'startup-failure', {
      accounts: 'not-run', catalog: 'fail',
    }),
    applicationFailure: { phase: 'application-start', reason: 'server did not start' },
  });
  assert.equal(state.nodes.accounts!.status, 'passed');
  assert.equal(state.nodes.accounts!.strikes.used, guardStrikes);
  assert.equal(state.nodes.catalog!.status, 'active');
  assert.equal(state.nodes.catalog!.strikes.used, 2);
  assert.equal(state.nodes.ownership!.status, 'active');
  assert.equal(state.nodes.ownership!.strikes.used, 0);
  assert.equal(dependencyAction(state).type, 'repair');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['catalog']);
});

test('feature repair selection isolates work after an application startup failure', () => {
  const value = fixture();
  value.strikes.default = 3;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, {
    ...conclusive(state, 'startup-failure', { accounts: 'fail', catalog: 'fail' }),
    applicationFailure: { phase: 'application-start', reason: 'server did not start' },
  });
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  assert.equal(state.nodes.accounts!.strikes.used, 1);
  assert.equal(state.nodes.catalog!.strikes.used, 1);
});

test('one failed branch stops while passed branches continue through any number of levels', () => {
  let state = progressionEngine.initialize(fixture());
  state = progressionEngine.recordResult(state, conclusive(state, 'l1-a', {
    accounts: 'pass', catalog: 'fail',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'l1-b', {
    catalog: 'fail',
  }));
  assert.equal(state.nodes.catalog!.status, 'failed');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['ownership']);
  state = progressionEngine.recordResult(state, conclusive(state, 'ownership', {
    ownership: 'pass',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'recovery', {
    recovery: 'pass',
  }));
  assert.equal(state.nodes.search!.status, 'blocked');
  assert.equal(state.nodes.ownership!.status, 'passed');
  assert.equal(state.level, 3);
  assert.equal(state.nodes.recovery!.status, 'passed');
  assert.equal(state.nodes.recommendations!.status, 'blocked');
  assert.equal(state.phase, 'terminal');
  assert.deepEqual(progressionEngine.nextAction(state), {
    type: 'terminal', outcome: { kind: 'partial', reason: 'graph-complete', level: 3 },
  });
});

test('an unchanged branch stops without stopping a branch whose failures changed', () => {
  const value = fixture();
  value.repairSelection = 'batch';
  value.strikes.default = 4;
  value.unchangedFailureLimit = 2;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'first-failures', {
    accounts: { 'check.accounts.1': 'fail', 'check.accounts.2': 'pass' },
    catalog: 'fail',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'changed-accounts', {
    accounts: { 'check.accounts.1': 'pass', 'check.accounts.2': 'fail' },
    catalog: 'fail',
  }));

  assert.equal(state.nodes.catalog!.status, 'failed');
  assert.equal(state.nodes.catalog!.unchangedFailure.count, 2);
  assert.equal(state.nodes.accounts!.status, 'active');
  assert.equal(state.nodes.accounts!.unchangedFailure.count, 1);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);

  state = progressionEngine.recordResult(state, conclusive(state, 'accounts-pass', {
    accounts: 'pass',
  }));
  assert.equal(state.nodes.accounts!.status, 'passed');
  assert.equal(state.nodes.catalog!.status, 'failed');
  assert.equal(state.nodes.ownership!.status, 'active');
  assert.equal(state.nodes.search!.status, 'blocked');
});

test('no passed node at a level stops progression', () => {
  const value = fixture();
  value.strikes.default = 1;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'l1', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.equal(state.phase, 'terminal');
  assert.deepEqual(state.terminalOutcome, {
    kind: 'failed', reason: 'no-unlocked-nodes', level: 1, blockedLevel: 1,
  });
  assert.equal(state.nodes.ownership!.status, 'blocked');
  assert.equal(state.nodes.search!.status, 'blocked');
  assert.equal(state.nodes.recovery!.status, 'blocked');
});

test('inconclusive attempts do not consume strikes or change selections', () => {
  const state = progressionEngine.initialize(fixture());
  const beforeStrikes = Object.fromEntries(Object.entries(state.nodes)
    .map(([nodeId, node]) => [nodeId, node.strikes]));
  const beforeScore = dependencyScore(state);
  assert.equal(beforeScore.questlineAveragePercentage, null);
  assert(beforeScore.questlines.every(questline => questline.percentage === null));
  const next = progressionEngine.recordResult(state, {
    attemptId: 'provider-503', outcome: 'inconclusive', category: 'provider_failure',
    reason: 'provider failure',
  });
  assert.deepEqual(Object.fromEntries(Object.entries(next.nodes)
    .map(([nodeId, node]) => [nodeId, node.strikes])), beforeStrikes);
  assert.deepEqual(dependencyPrompt(next), dependencyPrompt(state));
  assert.equal(next.attempts.at(-1)!.outcome, 'inconclusive');
  const score = dependencyScore(next);
  assert.equal(score.questlineAveragePercentage, null);
  assert.equal(score.attempts.inconclusive, 1);
  assert.equal(score.attempts.inconclusiveByCategory.provider_failure, 1);
  assert(score.questlines.every(questline => questline.ungradedPoints === questline.availablePoints));
});

test('a terminal failed run can receive an auditable continuation grant', () => {
  const value = fixture();
  value.strikes.default = 1;
  let state = progressionEngine.initialize(value);
  assert.throws(() => progressionEngine.grantStrikes(state,
    { grantId: 'too-early', level: 1, nodeIds: ['accounts'], strikes: 1 }),
  /only after progression terminates/);
  state = progressionEngine.recordResult(state, conclusive(state, 'failed-l1', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.deepEqual(state.terminalOutcome, {
    kind: 'failed', reason: 'no-unlocked-nodes', level: 1, blockedLevel: 1,
  });
  state = progressionEngine.grantStrikes(state, {
    grantId: 'grant-1', level: 1, nodeIds: ['accounts', 'catalog'], strikes: 2,
  });
  assert.equal(state.phase, 'active');
  assert.deepEqual(state.nodes.accounts!.strikes, {
    initialBudget: 1, granted: 2, budget: 3, used: 1,
  });
  assert.deepEqual(state.nodes.catalog!.strikes, state.nodes.accounts!.strikes);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  assert.deepEqual(state.events.map(event => event.type), ['attempt-recorded', 'strikes-granted']);
  state = progressionEngine.recordResult(state, conclusive(state, 'continued-l1', {
    accounts: 'pass',
  }));
  assert.equal(state.level, 1);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['catalog']);
  state = progressionEngine.recordResult(state, conclusive(state, 'continued-catalog', {
    accounts: 'pass', catalog: 'pass',
  }));
  assert.equal(state.level, 2);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['ownership', 'search']);
});

test('a continuation grant can reopen an exhausted branch after another branch completed', () => {
  const value = fixture();
  value.strikes.default = 1;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'partial-l1', {
    accounts: 'pass', catalog: 'fail',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'partial-l2', {
    accounts: 'pass', ownership: 'pass',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'partial-l3', {
    accounts: 'pass', ownership: 'pass', recovery: 'pass',
  }));
  assert.deepEqual(state.terminalOutcome, { kind: 'partial', reason: 'graph-complete', level: 3 });

  state = progressionEngine.grantStrikes(state, {
    grantId: 'catalog-grant', level: 1, nodeIds: ['catalog'], strikes: 1,
  });
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['catalog']);
  assert.deepEqual(dependencyGrading(state).nodeIds, ['accounts', 'catalog', 'ownership', 'recovery']);
  state = progressionEngine.recordResult(state, conclusive(state, 'continued-catalog', {
    accounts: 'pass', catalog: 'pass',
  }));
  assert.equal(state.level, 2);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['search']);
  assert.deepEqual(dependencyGrading(state).nodeIds,
    ['accounts', 'catalog', 'ownership', 'search', 'recovery']);
  assert.equal(state.nodes.ownership!.checks['check.ownership.1'], 'pass');
  assert.equal(state.nodes.recovery!.checks['check.recovery.1'], 'pass');
});

test('an earlier grant does not reopen a later exhausted level or retain its evidence', () => {
  const value = fixture();
  value.strikes.default = 1;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'partial-l1', {
    accounts: 'pass', catalog: 'fail',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'failed-l2', {
    accounts: 'pass', ownership: 'fail',
  }));
  assert.equal(state.phase, 'terminal');
  assert.equal(state.nodes.catalog!.exhaustedAtLevel, 1);
  assert.equal(state.nodes.ownership!.exhaustedAtLevel, 2);

  state = progressionEngine.grantStrikes(state, {
    grantId: 'l1-only', level: 1, nodeIds: ['catalog'], strikes: 1,
  });
  assert.equal(state.nodes.ownership!.status, 'failed');
  assert.equal(state.nodes.ownership!.exhaustedAtLevel, 2);
  assert.equal(state.nodes.ownership!.checks['check.ownership.1'], 'fail');
  state = progressionEngine.recordResult(state, conclusive(state, 'repair-l1', {
    accounts: 'pass', catalog: 'pass',
  }));
  assert.equal(state.level, 2);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['search']);
  state = progressionEngine.recordResult(state, conclusive(state, 'finish-search', {
    accounts: 'pass', catalog: 'pass', search: 'pass',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'finish-recommendations', {
    accounts: 'pass', catalog: 'pass', search: 'pass', recommendations: 'pass',
  }));
  assert.equal(state.phase, 'terminal');
  assert.throws(() => progressionEngine.grantStrikes(state,
    { grantId: 'wrong-level', level: 1, nodeIds: ['catalog'], strikes: 1 }),
  /without an exhausted repair budget/);
  state = progressionEngine.grantStrikes(state,
    { grantId: 'l2-needed', level: 2, nodeIds: ['ownership'], strikes: 1 });
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['ownership']);
});

test('state replay rebuilds the event history and rejects invalid sequences', () => {
  let state = progressionEngine.initialize(fixture());
  state = progressionEngine.recordResult(state, conclusive(state, 'first', {
    accounts: 'pass', catalog: 'fail',
  }));
  assert.deepEqual(progressionEngine.replay(state.definition, state.events), state);
  const missingEvent = structuredClone(state);
  missingEvent.events[0]!.sequence = 2;
  assert.throws(() => progressionEngine.replay(missingEvent.definition, missingEvent.events),
    /event sequence 2 must be 1/);
});

test('a child with multiple parents opens only when every parent passes', () => {
  const value = {
    schemaVersion: 4,
    kind: 'progression-mode',
    id: 'multi-parent',
    version: '1.0.0',
    state: 'draft',
    title: 'Multi-parent graph',
    policy: 'dependency-graph',
    strikes: { default: 1, levels: {} },
    nodes: [node('account', [], 'orders'), node('cart', [], 'shopping'),
      node('checkout', ['account', 'cart'], 'orders')],
    questlines: [
      { id: 'orders', title: 'Orders' },
      { id: 'shopping', title: 'Shopping' },
    ],
  };
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'one-parent', {
    account: 'pass', cart: 'fail',
  }));
  assert.equal(state.phase, 'terminal');
  assert.equal(state.nodes.checkout!.status, 'blocked');

  state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'both-parents', {
    account: 'pass', cart: 'pass',
  }));
  assert.equal(state.nodes.checkout!.status, 'active');
  assert.deepEqual(dependencyGrading(state).nodeIds,
    ['account', 'cart', 'checkout']);
});

test('dependency regressions return to the prompt and prevent descendant passes', () => {
  const value = fixture();
  value.strikes.levels[2] = 2;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'l1', {
    accounts: 'pass', catalog: 'pass',
  }));
  const accountsCheck = dependencyGrading(state).checks
    .find(check => check.nodeId === 'accounts')!.id;
  state = progressionEngine.recordResult(state, conclusive(state, 'l2-regression', {
    accounts: { [accountsCheck]: 'fail' },
    ownership: 'pass',
    catalog: 'pass',
    search: 'pass',
  }));
  assert.equal(state.phase, 'active');
  assert.equal(state.nodes.accounts!.status, 'active');
  assert.equal(state.nodes.ownership!.status, 'locked');
  assert.equal(state.nodes.search!.status, 'passed');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  assert.deepEqual(dependencyGrading(state).nodeIds,
    ['accounts', 'catalog', 'search']);

  state = progressionEngine.recordResult(state, conclusive(state, 'l2-repair', {
    accounts: 'pass', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.level, 3);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['recommendations', 'recovery']);
});

test('passed nodes outside the active branch are regression guards', () => {
  const value = fixture();
  value.strikes.levels[2] = 2;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'l1-pass', {
    accounts: 'pass', catalog: 'pass',
  }));
  assert.deepEqual(dependencyGrading(state).nodeIds,
    ['accounts', 'catalog', 'ownership', 'search']);
  state = progressionEngine.recordResult(state, conclusive(state, 'catalog-regressed', {
    accounts: 'pass', catalog: 'fail', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.nodes.catalog!.status, 'active');
  assert.equal(state.nodes.ownership!.status, 'passed');
  assert.equal(state.nodes.search!.status, 'locked');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['catalog']);
  state = progressionEngine.recordResult(state, conclusive(state, 'catalog-repair', { catalog: 'pass' }));
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['recommendations', 'recovery']);
});

test('a continuation grant repairs an exhausted ancestor without charging its child', () => {
  const value = fixture();
  value.strikes.levels[2] = 1;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'l1-pass', {
    accounts: 'pass', catalog: 'pass',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'l2-regression', {
    accounts: 'fail', catalog: 'pass', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.level, 1);
  assert.equal(state.nodes.accounts!.strikes.used, 0);
  assert.equal(state.nodes.accounts!.strikes.budget, 2);
  state = progressionEngine.recordResult(state, conclusive(state, 'l2-regression-repair', {
    accounts: 'fail', catalog: 'pass', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.level, 1);
  assert.equal(state.nodes.accounts!.strikes.used, 1);
  state = progressionEngine.recordResult(state, conclusive(state, 'l2-regression-exhausted', {
    accounts: 'fail', catalog: 'pass', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.level, 3);
  assert.equal(state.nodes.recommendations!.status, 'active');
  assert.equal(state.nodes.accounts!.status, 'failed');
  assert.equal(state.nodes.accounts!.exhaustedAtLevel, 1);
  assert.equal(state.nodes.ownership!.status, 'blocked');
  assert.equal(state.nodes.ownership!.strikes.used, 0);
  assert.equal(state.nodes.search!.status, 'passed');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['recommendations']);
  state = progressionEngine.recordResult(state, conclusive(state, 'finish-recommendations', {
    recommendations: 'pass',
  }));
  assert.equal(state.phase, 'terminal');
  const blockedOwnership = dependencyScore(state).nodes.find(item => item.id === 'ownership')!;
  assert.equal(blockedOwnership.passedPoints, 0);
  assert.equal(blockedOwnership.blockedPoints, blockedOwnership.availablePoints);
  assert.deepEqual(blockedOwnership.blockedBy, ['accounts']);
  assert.throws(() => progressionEngine.grantStrikes(state, {
    grantId: 'child-only', level: 2, nodeIds: ['ownership'], strikes: 1,
  }), /without an exhausted repair budget/);

  state = progressionEngine.grantStrikes(state, {
    grantId: 'repair-regression', level: 2, nodeIds: ['accounts'], strikes: 1,
  });
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  state = progressionEngine.recordResult(state, conclusive(state, 'l2-repair', {
    accounts: 'pass', catalog: 'pass', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.level, 2);
  assert.equal(state.nodes.recovery!.status, 'locked');
  assert.equal(state.nodes.recommendations!.status, 'passed');
});

test('questline percentages use partial check points and the overall score weights groups equally', () => {
  const value = fixture();
  value.strikes.default = 1;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'partial', {
    accounts: { 'check.accounts.1': 'pass', 'check.accounts.2': 'fail' },
    catalog: 'pass',
  }));
  assert.equal(dependencyScore(state).questlineAveragePercentage, null);
  state = progressionEngine.recordResult(state, conclusive(state, 'search', {
    catalog: 'pass', search: 'pass',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'recommendations', {
    catalog: 'pass', search: 'pass', recommendations: 'pass',
  }));
  const score = dependencyScore(state);
  assert.deepEqual(score.questlines.map(item => ({ id: item.id,
    passedPoints: item.passedPoints, availablePoints: item.availablePoints })), [
    { id: 'discovery', passedPoints: 3, availablePoints: 3 },
    { id: 'identity', passedPoints: 2, availablePoints: 5 },
  ]);
  assert.equal(score.questlineAveragePercentage, (100 + (2 / 5) * 100) / 2);
  assert.deepEqual(score.uniqueChecks, {
    passedPoints: 5,
    failedPoints: 1,
    blockedPoints: 2,
    testSystemPoints: 0,
    gradedPoints: 6,
    ungradedPoints: 0,
    availablePoints: 8,
    percentage: 62.5,
    provisionalPercentage: null,
  });
});

test('conclusive results fail closed when selected nodes or checks are missing or repeated', () => {
  const state = progressionEngine.initialize(fixture());
  const missing = conclusive(state, 'missing', {});
  missing.nodes.pop();
  assert.throws(() => progressionEngine.recordResult(state, missing), /missing nodes/);
  const repeated = conclusive(state, 'repeated', {});
  repeated.nodes[0]!.checks.push(structuredClone(repeated.nodes[0]!.checks[0]!));
  assert.throws(() => progressionEngine.recordResult(state, repeated), /repeats check/);
});

test('grading excludes ready nodes that are not in the selected repair prompt', () => {
  const value = fixture();
  value.strikes.default = 3;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'isolate', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
  assert.deepEqual(dependencyGrading(state).nodeIds, ['accounts']);
  assert.equal(state.nodes.catalog!.status, 'active');
});

test('a check waits for each required feature to be usable or selected', () => {
  const value = fixture();
  value.nodes[0]!.gradingChecks.push({
    id: 'check.accounts.search-ready', points: 1, role: 'guarantee',
    requiresFeatures: ['features.search'],
  });
  let state = progressionEngine.initialize(value);
  assert(!dependencyGrading(state).checks.some(check => check.id === 'check.accounts.search-ready'));
  state = progressionEngine.recordResult(state, conclusive(state, 'roots', {
    accounts: 'pass', catalog: 'pass',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'level-two', {
    accounts: 'pass', ownership: 'pass',
  }));
  assert(dependencyGrading(state).checks.some(check => check.id === 'check.accounts.search-ready'));
});

test('selected node scope can move across branch depths without a depth gate', () => {
  const value = fixture();
  value.strikes.default = 3;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'roots-pass', {
    accounts: 'pass', catalog: 'pass',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'children-pass', {
    accounts: 'pass', catalog: 'pass', ownership: 'pass', search: 'pass',
  }));
  assert.equal(dependencyAction(state).level, 3);
  state = progressionEngine.recordResult(state, conclusive(state, 'deep-with-regression', {
    accounts: 'pass', catalog: 'fail', ownership: 'pass', search: 'pass', recovery: 'pass',
  }));
  const action = dependencyAction(state);
  assert.equal(action.level, 1);
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['catalog']);
  assert(!dependencyGrading(state).nodeIds.includes('recommendations'));
  assert.equal(state.nodes.search!.status, 'locked');
  assert.deepEqual(dependencyScore(state).nodes.find(item => item.id === 'search')!.blockedBy, []);
});

test('working-only terminal progress is partial', () => {
  const value = fixture();
  value.nodes = [node('accounts', [], 'identity', [1, 1])];
  value.questlines = [{ id: 'identity', title: 'Identity' }];
  value.strikes.levels = {};
  value.strikes.default = 1;
  value.nodes[0]!.gradingChecks[1]!.role = 'guarantee';
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'production-failure', {
    accounts: { 'check.accounts.1': 'pass', 'check.accounts.2': 'fail' },
  }));
  assert.equal(state.nodes.accounts!.status, 'working');
  assert.deepEqual(state.terminalOutcome, {
    kind: 'partial', reason: 'no-unlocked-nodes', level: 1, blockedLevel: 1,
  });
});

test('test-system checks remain build work', () => {
  const value = fixture();
  value.nodes = [node('accounts', [], 'identity'), node('child', ['accounts'], 'identity')];
  value.questlines = [{ id: 'identity', title: 'Identity' }];
  value.strikes.default = 1;
  value.strikes.levels = {};
  value.nodes[0]!.gradingChecks.push({ id: 'check.accounts.child-ready', points: 1,
    role: 'guarantee', requiresFeatures: ['features.child'] });
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'parent-pass', {
    accounts: 'pass',
  }));
  state = progressionEngine.recordResult(state, {
    ...conclusive(state, 'test-system', {
      accounts: 'not-run', child: 'fail',
    }),
    applicationFailure: { phase: 'application-start', reason: 'server did not start' },
  });
  assert.equal(state.nodes.accounts!.checks['check.accounts.child-ready'], 'test-system');
  assert.equal(state.nodes.accounts!.status, 'working');
  assert.equal(dependencyAction(state).type, 'build');
  assert.deepEqual(dependencyPrompt(state).nodeIds, ['accounts']);
});

test('an unmeasured check does not hide an independent measured failure', () => {
  const value = fixture();
  value.strikes.default = 3;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'mixed-grade', {
    accounts: 'not-run', catalog: 'fail',
  }));
  assert.deepEqual(Object.values(state.nodes.accounts!.checks), ['test-system', 'test-system']);
  assert.equal(state.nodes.accounts!.strikes.used, 0);
  assert.equal(state.nodes.catalog!.checks['check.catalog.1'], 'fail');
  assert.equal(state.nodes.catalog!.strikes.used, 1);
});
