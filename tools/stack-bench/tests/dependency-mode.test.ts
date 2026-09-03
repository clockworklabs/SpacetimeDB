import assert from 'node:assert/strict';
import test from 'node:test';

import { type ConclusiveResult, type DependencyGradingSelection,
  type DependencyPromptSelection } from '../src/progression/dependency-mode.js';
import { compileDependencyMode } from '../src/progression/dependency-definition.js';
import type { DependencyScore } from '../src/progression/dependency-score.js';
import { compileDependencyPolicyInput, compileFeatureCatalogInput }
  from '../src/progression/progression-definition.js';
import { createProgressionEngine, progressionEngine, type ProgressionWorkAction }
  from '../src/progression/progression-engine.js';
import type { ProgressionState } from '../src/progression/progression-state.js';
import type { RepairPlan } from '../src/progression/repair-plan.js';

type Outcome = 'pass' | 'fail' | 'not-run';
type Outcomes = Record<string, Outcome | Record<string, Outcome>>;

interface FixtureNode {
  id: string;
  title: string;
  questline: string;
  dependencies: Array<{ id: string; reason: string }>;
  featureRefs: string[];
  promptModules: string[];
  gradingChecks: Array<{ id: string; points: number; role: 'feature' | 'guarantee' }>;
}

interface FixtureDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  policy: string;
  repair: RepairPlan;
  unchangedFailureLimit: number;
  workSelection: 'feature' | 'progressive' | 'all-at-once';
  nodes: FixtureNode[];
  questlines: Array<{ id: string; title: string }>;
}

const node = (id: string, dependencies: string[], questline: string,
  points: number[] = [1]): FixtureNode => ({
  id, title: id, questline,
  dependencies: dependencies.map(parent => ({ id: parent, reason: `${id} requires ${parent}` })),
  featureRefs: [`features.${id}@1.0.0`],
  promptModules: [`prompt.${id}@1.0.0`],
  gradingChecks: points.map((value, index) => ({
    id: `check.${id}.${index + 1}`, points: value, role: 'feature',
  })),
});

const fixture = (): FixtureDefinition => ({
  schemaVersion: 5,
  kind: 'progression-mode',
  id: 'storefront-paths',
  version: '1.0.0',
  state: 'draft',
  title: 'Storefront paths',
  policy: 'dependency-graph',
  repair: { selection: 'feature', budget: { total: 4, perFeature: 1 } },
  unchangedFailureLimit: 3,
  workSelection: 'progressive',
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

const prompt = (state: ProgressionState): DependencyPromptSelection =>
  progressionEngine.promptSelection(state) as DependencyPromptSelection;
const grading = (state: ProgressionState): DependencyGradingSelection =>
  progressionEngine.gradingSelection(state) as DependencyGradingSelection;
const action = (state: ProgressionState): ProgressionWorkAction => {
  const next = progressionEngine.nextAction(state);
  if (next.type === 'terminal') throw new Error('expected active work');
  return next;
};

function grade(state: ProgressionState, attemptId: string,
  outcomes: Outcomes): ConclusiveResult {
  const selection = grading(state);
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

function repairedGrade(state: ProgressionState, attemptId: string,
  outcomes: Outcomes): ConclusiveResult {
  const current = action(state);
  if (current.type !== 'repair') throw new Error('expected repair work');
  return { ...grade(state, attemptId, outcomes), completedRepair: true };
}

test('the graph and repair plan compile deterministically', () => {
  const input = fixture();
  input.nodes.reverse();
  input.questlines.reverse();
  const compiled = compileDependencyMode(input);
  assert.deepEqual(compiled.nodes.map(item => item.id), [
    'accounts', 'catalog', 'ownership', 'search', 'recommendations', 'recovery',
  ]);
  assert.deepEqual(compiled.questlines.map(item => item.id), ['discovery', 'identity']);
  assert.deepEqual(compiled.repair, { selection: 'feature', budget: { total: 4, perFeature: 1 } });
  const { policy: _policy, repair, unchangedFailureLimit: _limit,
    workSelection: _workSelection, ...catalogDefinition } = compiled;
  const catalog = compileFeatureCatalogInput({
    ...catalogDefinition, schemaVersion: 1, kind: 'feature-catalog',
  });
  const first = compileDependencyPolicyInput(repair, catalog, { unchangedFailureLimit: 2 });
  const second = compileDependencyPolicyInput(repair, catalog, { unchangedFailureLimit: 3 });
  assert.equal(first.definition.version, '4.0.0');
  assert.notEqual(first.identity.sha256, second.identity.sha256);
});

test('invalid graphs and repair plans fail before execution', async t => {
  const cases: Array<[string, (value: FixtureDefinition) => void, RegExp]> = [
    ['duplicate node', value => value.nodes.push(structuredClone(value.nodes[0]!)), /duplicates/],
    ['missing parent', value => { value.nodes[2]!.dependencies[0]!.id = 'missing'; }, /unknown parent/],
    ['dependency cycle', value => {
      value.nodes[0]!.dependencies = [{ id: 'ownership', reason: 'cycle' }];
    }, /dependency cycle/],
    ['empty budget', value => { value.repair.budget = {}; }, /contain a limit/],
    ['undefined budget', value => {
      value.repair.budget = { total: undefined } as unknown as FixtureDefinition['repair']['budget'];
    }, /contain a limit/],
    ['negative budget', value => { value.repair.budget.total = -1; }, /non-negative safe integer/],
    ['bad selection', value => { value.repair.selection = 'other' as 'feature'; }, /feature.*batch/],
    ['bad work selection', value => {
      value.workSelection = 'other' as 'feature';
    }, /feature.*progressive.*all-at-once/],
    ['unknown questline', value => { value.nodes[0]!.questline = 'missing'; }, /unknown questline/],
  ];
  for (const [name, mutate, expected] of cases) {
    await t.test(name, () => {
      const value = fixture();
      mutate(value);
      assert.throws(() => compileDependencyMode(value), expected);
    });
  }
});

test('the engine dispatches registered policies', () => {
  const fake = {
    id: 'fake', compile: (value: Record<string, unknown>) => value,
    initialize: (value: Record<string, unknown>) => ({ policy: value.policy }),
    activeNodes: () => ['fake'], promptSelection: () => ({ fake: 'prompt' }),
    gradingSelection: () => ({ fake: 'grade' }),
    recordResult: (state: Record<string, unknown>) => state,
    grantRepairs: (state: Record<string, unknown>) => state,
    replay: (definition: Record<string, unknown>) => ({ policy: definition.policy }),
    nextAction: () => ({ type: 'complete' }), score: () => ({ score: 1 }),
  };
  const engine = createProgressionEngine([fake]);
  const state = engine.initialize({ policy: 'fake' });
  assert.deepEqual(engine.activeNodes(state), ['fake']);
  assert.throws(() => engine.initialize({ policy: 'missing' }), /unknown progression policy/);
});

test('an initial failure costs no repair and passed branches continue', () => {
  let state = progressionEngine.initialize(fixture());
  state = progressionEngine.recordResult(state, grade(state, 'initial', {
    accounts: 'fail', catalog: 'pass',
  }));
  assert.equal(state.nodes.accounts!.repairs.used, 0);
  assert.deepEqual(action(state).repair, { nodeIds: ['accounts'], remaining: 1 });
  state = progressionEngine.recordResult(state, repairedGrade(state, 'repair', {
    accounts: 'fail', catalog: 'pass', search: 'pass',
  }));
  assert.equal(state.nodes.accounts!.repairs.used, 1);
  assert.equal(state.nodes.accounts!.status, 'failed');
  assert.equal(state.nodes.ownership!.status, 'blocked');
  assert.equal(state.nodes.search!.status, 'active');
  assert.deepEqual(prompt(state).nodeIds, ['search']);
});

test('zero repair budget stops only the failed branch', () => {
  const definition = fixture();
  definition.repair.budget = { total: 0 };
  let state = progressionEngine.initialize(definition);
  state = progressionEngine.recordResult(state, grade(state, 'initial', {
    accounts: 'fail', catalog: 'pass',
  }));
  assert.equal(state.nodes.accounts!.status, 'failed');
  assert.equal(state.nodes.accounts!.repairs.used, 0);
  assert.deepEqual(prompt(state).nodeIds, ['search']);
});

test('the total budget is shared across features in deterministic order', () => {
  const definition = fixture();
  definition.repair.budget = { total: 1 };
  let state = progressionEngine.initialize(definition);
  state = progressionEngine.recordResult(state, grade(state, 'initial', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.deepEqual(prompt(state).nodeIds, ['accounts']);
  state = progressionEngine.recordResult(state, repairedGrade(state, 'accounts-repair', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.equal(state.nodes.accounts!.repairs.used, 1);
  assert.equal(state.nodes.catalog!.repairs.used, 0);
  assert.equal(state.nodes.catalog!.status, 'failed');
  assert.equal(state.phase, 'terminal');
});

test('one batch repair charges the session and each selected feature once', () => {
  const definition = fixture();
  definition.repair = { selection: 'batch', budget: { total: 1, perFeature: 1 } };
  let state = progressionEngine.initialize(definition);
  state = progressionEngine.recordResult(state, grade(state, 'initial', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.deepEqual(action(state).repair.nodeIds, ['accounts', 'catalog']);
  state = progressionEngine.recordResult(state, repairedGrade(state, 'batch-repair', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.equal(state.attempts.filter(attempt => attempt.repair).length, 1);
  assert.equal(state.nodes.accounts!.repairs.used, 1);
  assert.equal(state.nodes.catalog!.repairs.used, 1);
});

test('provider failure costs zero and completed repair with failed grading costs one', () => {
  let state = progressionEngine.initialize(fixture());
  state = progressionEngine.recordResult(state, grade(state, 'initial', {
    accounts: 'fail', catalog: 'pass',
  }));
  state = progressionEngine.recordResult(state, {
    attemptId: 'provider-failure', outcome: 'inconclusive',
    category: 'provider_failure', reason: 'provider stopped',
  });
  assert.equal(state.nodes.accounts!.repairs.used, 0);
  state = progressionEngine.recordResult(state, {
    attemptId: 'grade-failure', outcome: 'inconclusive',
    category: 'harness_failure', reason: 'grader stopped',
    completedRepair: true,
  });
  assert.equal(state.nodes.accounts!.repairs.used, 1);
  assert.equal(state.nodes.accounts!.status, 'failed');
});

test('a completed repair exhausts shared budget even when grading is inconclusive', () => {
  const definition = fixture();
  definition.repair.budget = { total: 1 };
  let state = progressionEngine.initialize(definition);
  state = progressionEngine.recordResult(state, grade(state, 'initial', {
    accounts: 'fail', catalog: 'fail',
  }));
  state = progressionEngine.recordResult(state, {
    attemptId: 'inconclusive-repair', outcome: 'inconclusive',
    category: 'harness_failure', reason: 'grader stopped',
    completedRepair: true,
  });
  assert.equal(state.nodes.accounts!.status, 'failed');
  assert.equal(state.nodes.catalog!.status, 'failed');
  assert.equal(state.phase, 'terminal');
});

test('unused depth repairs carry forward when configured', () => {
  const definition = fixture();
  definition.repair.budget = { perDepth: { count: 1, carry: true } };
  let state = progressionEngine.initialize(definition);
  state = progressionEngine.recordResult(state, grade(state, 'roots', {
    accounts: 'pass', catalog: 'pass',
  }));
  state = progressionEngine.recordResult(state, grade(state, 'depth-two', {
    accounts: 'pass', catalog: 'pass', ownership: 'fail', search: 'fail',
  }));
  assert.deepEqual(action(state).repair, { nodeIds: ['ownership'], remaining: 2 });
  state = progressionEngine.recordResult(state, repairedGrade(state, 'ownership-repair', {
    accounts: 'pass', ownership: 'fail',
  }));
  assert.deepEqual(action(state).repair, { nodeIds: ['ownership'], remaining: 1 });
});

test('repeated failures stop their branch while unmeasured work remains build work', () => {
  const definition = fixture();
  definition.unchangedFailureLimit = 1;
  definition.repair.budget = { perFeature: 5 };
  let state = progressionEngine.initialize(definition);
  state = progressionEngine.recordResult(state, grade(state, 'initial', {
    accounts: 'not-run', catalog: 'fail',
  }));
  state = progressionEngine.recordResult(state, repairedGrade(state, 'unchanged', {
    catalog: 'fail',
  }));
  assert(Object.values(state.nodes.accounts!.checks).every(outcome => outcome === 'test-system'));
  assert.equal(state.nodes.accounts!.repairs.used, 0);
  assert.equal(state.nodes.catalog!.exhaustionReason, 'repeated-findings');
  assert.equal(action(state).type, 'build');
  assert.deepEqual(prompt(state).nodeIds, ['accounts']);
});

test('application startup failures target current work without consuming a repair', () => {
  let state = progressionEngine.initialize(fixture());
  state = progressionEngine.recordResult(state, {
    ...grade(state, 'startup-failure', { accounts: 'fail', catalog: 'fail' }),
    applicationFailure: { phase: 'startup', reason: 'application did not start' },
  });
  assert.equal(state.nodes.accounts!.repairs.used, 0);
  assert.equal(state.nodes.catalog!.repairs.used, 0);
  assert.deepEqual(action(state).repair, { nodeIds: ['accounts'], remaining: 1 });
  const missing = grade(state, 'missing-check', { accounts: 'fail' });
  missing.nodes[0]!.checks.pop();
  assert.throws(() => progressionEngine.recordResult(state, missing), /missing checks/);
});

test('feature work selection requests one ready feature at a time', () => {
  const definition = fixture();
  definition.workSelection = 'feature';
  definition.repair.budget = { total: 0 };
  let state = progressionEngine.initialize(definition);
  assert.deepEqual(prompt(state).nodeIds, ['accounts']);
  state = progressionEngine.recordResult(state, grade(state, 'accounts', { accounts: 'pass' }));
  assert.deepEqual(prompt(state).nodeIds, ['catalog']);
  state = progressionEngine.recordResult(state, grade(state, 'catalog', {
    accounts: 'pass', catalog: 'pass',
  }));
  assert.deepEqual(prompt(state).nodeIds, ['ownership']);
});

test('all-at-once repairs one failure and regrades the complete graph', () => {
  const definition = fixture();
  definition.workSelection = 'all-at-once';
  let state = progressionEngine.initialize(definition);
  assert.deepEqual(prompt(state).nodeIds, [
    'accounts', 'catalog', 'ownership', 'search', 'recommendations', 'recovery',
  ]);
  assert.equal(grading(state).checks.length, 7);
  state = progressionEngine.recordResult(state, grade(state, 'initial', {
    accounts: 'fail',
  }));
  assert.deepEqual(prompt(state).nodeIds, ['accounts']);
  assert.equal(grading(state).nodeIds.length, 6);
  state = progressionEngine.recordResult(state, repairedGrade(state, 'repair', {
    accounts: 'pass',
  }));
  assert.equal(state.nodes.accounts!.repairs.used, 1);
  assert.equal(state.phase, 'terminal');
  assert.equal(state.terminalOutcome?.kind, 'passed');
});

test('multi-parent work waits for both parents and an ancestor regression closes its path', () => {
  const definition = fixture();
  definition.nodes.find(item => item.id === 'ownership')!.dependencies = [
    { id: 'accounts', reason: 'ownership requires accounts' },
    { id: 'catalog', reason: 'ownership requires catalog' },
  ];

  let waiting = progressionEngine.initialize(definition);
  waiting = progressionEngine.recordResult(waiting, grade(waiting, 'one-parent', {
    accounts: 'pass', catalog: 'fail',
  }));
  assert.equal(waiting.nodes.ownership!.status, 'locked');

  let regressed = progressionEngine.initialize(definition);
  regressed = progressionEngine.recordResult(regressed, grade(regressed, 'roots', {
    accounts: 'pass', catalog: 'pass',
  }));
  regressed = progressionEngine.recordResult(regressed, grade(regressed, 'children', {
    ownership: 'pass', search: 'pass',
  }));
  regressed = progressionEngine.recordResult(regressed, grade(regressed, 'ancestor-regression', {
    accounts: 'fail', recommendations: 'pass', recovery: 'pass',
  }));
  assert.deepEqual(prompt(regressed).nodeIds, ['accounts']);
  assert.equal(regressed.nodes.ownership!.status, 'locked');
  assert.equal(regressed.nodes.recovery!.status, 'locked');
  assert.equal(regressed.nodes.recommendations!.status, 'passed');
});

test('a repair grant reopens the exact exhausted branch', () => {
  const definition = fixture();
  definition.repair.budget = { total: 0 };
  let state = progressionEngine.initialize(definition);
  state = progressionEngine.recordResult(state, grade(state, 'initial', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.equal(state.phase, 'terminal');
  state = progressionEngine.grantRepairs(state, {
    grantId: 'grant-1', level: 1, nodeIds: ['accounts'], repairs: 1,
  });
  assert.deepEqual(action(state).repair, {
    nodeIds: ['accounts'], remaining: 1, grantId: 'grant-1',
  });
  state = progressionEngine.recordResult(state, repairedGrade(state, 'granted-repair', {
    accounts: 'pass',
  }));
  assert.equal(state.nodes.accounts!.status, 'passed');
  assert.equal(state.nodes.accounts!.repairs.used, 1);
  assert.deepEqual(prompt(state).nodeIds, ['ownership']);
});

test('score keeps blocked points and averages questlines equally', () => {
  const definition = fixture();
  definition.repair.budget = { total: 0 };
  let state = progressionEngine.initialize(definition);
  state = progressionEngine.recordResult(state, grade(state, 'initial', {
    accounts: 'pass', catalog: 'fail',
  }));
  state = progressionEngine.recordResult(state, grade(state, 'ownership', {
    accounts: 'pass', ownership: 'pass',
  }));
  state = progressionEngine.recordResult(state, grade(state, 'recovery', {
    accounts: 'pass', ownership: 'pass', recovery: 'pass',
  }));
  const score = progressionEngine.score(state) as DependencyScore;
  assert.equal(score.questlines.find(item => item.id === 'identity')!.passedPoints, 5);
  assert.equal(score.questlines.find(item => item.id === 'discovery')!.blockedPoints, 2);
  assert.equal(score.questlineAveragePercentage, 50);
});
