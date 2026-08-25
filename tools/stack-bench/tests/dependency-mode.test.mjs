import assert from 'node:assert/strict';
import test from 'node:test';

import { compileDependencyMode } from '../src/progression/dependency-mode.mjs';
import { createProgressionEngine, progressionEngine } from '../src/progression/progression-engine.mjs';

const node = (id, level, dependencies, points = [1]) => ({
  id,
  level,
  dependencies,
  featureRefs: [`features.${id}@1.0.0`],
  promptModules: [`prompt.${id}`],
  gradingChecks: points.map((value, index) => ({ id: `check.${id}.${index + 1}`, points: value })),
});

const fixture = () => ({
  schemaVersion: 1,
  kind: 'progression-mode',
  id: 'storefront-paths',
  version: '1.0.0',
  policy: 'dependency-gated',
  strikes: { default: 2, levels: { 2: 1 } },
  nodes: [
    node('accounts', 1, [], [2, 1]),
    node('catalog', 1, []),
    node('ownership', 2, ['accounts']),
    node('search', 2, ['catalog']),
    node('recovery', 3, ['ownership']),
    node('recommendations', 3, ['search']),
  ],
  questlines: [
    { id: 'identity', title: 'Identity', nodes: ['accounts', 'ownership', 'recovery'] },
    { id: 'discovery', title: 'Discovery', nodes: ['catalog', 'search', 'recommendations'] },
  ],
});

function conclusive(state, attemptId, outcomes) {
  const selection = progressionEngine.gradingSelection(state);
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
  const compiled = compileDependencyMode(input);
  assert.deepEqual(compiled.nodes.map(item => item.id), [
    'accounts', 'catalog', 'ownership', 'search', 'recommendations', 'recovery',
  ]);
  assert.deepEqual(compiled.questlines.map(item => item.id), ['discovery', 'identity']);
  assert.deepEqual(compiled.strikes, { levels: { 1: 2, 2: 1, 3: 2 } });
});

test('invalid graphs, questlines, and strike budgets fail before execution', async t => {
  const cases = [
    ['duplicate node', value => value.nodes.push(structuredClone(value.nodes[0])), /duplicates/],
    ['missing parent', value => { value.nodes[2].dependencies = ['missing']; }, /unknown parent/],
    ['dependency cycle', value => {
      value.nodes[0].dependencies = ['ownership'];
      value.nodes[2].dependencies = ['accounts'];
    }, /dependency cycle/],
    ['same level parent', value => { value.nodes[2].dependencies = ['catalog', 'search']; }, /must be in level 1/],
    ['skipped level parent', value => { value.nodes[4].dependencies = ['accounts']; }, /must be in level 2/],
    ['missing level', value => { value.nodes.filter(item => item.level === 3).forEach(item => { item.level = 4; }); }, /contiguous/],
    ['invalid default strikes', value => { value.strikes.default = 0; }, /positive integer/],
    ['invalid level strikes', value => { value.strikes.levels[2] = -1; }, /positive integer/],
    ['negative check points', value => { value.nodes[0].gradingChecks[0].points = -1; }, /non-negative integer/],
    ['no scored checks', value => {
      value.nodes[0].gradingChecks.forEach(check => { check.points = 0; });
    }, /at least one scored check/],
    ['missing budget', value => { value.strikes = { levels: { 1: 1 } }; }, /is required/],
    ['unknown quest node', value => { value.questlines[0].nodes[1] = 'missing'; }, /unknown node/],
    ['contradictory questline', value => { value.questlines[0].nodes = ['accounts', 'search']; }, /does not directly depend/],
    ['orphaned node', value => { value.questlines[0].nodes.pop(); }, /do not cover nodes/],
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
    compile: value => value,
    initialize: value => ({ policy: value.policy }),
    activeNodes: () => ['fake'],
    promptSelection: () => ({ fake: 'prompt' }),
    gradingSelection: () => ({ fake: 'grade' }),
    recordResult: state => state,
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
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['accounts', 'catalog']);
  assert.deepEqual(progressionEngine.promptSelection(state).promptModules,
    ['prompt.accounts', 'prompt.catalog']);
  assert.doesNotMatch(JSON.stringify(progressionEngine.promptSelection(state)), /ownership|recovery|search/);

  state = progressionEngine.recordResult(state, conclusive(state, 'level-1', {
    accounts: 'pass',
    catalog: 'fail',
  }));
  assert.equal(progressionEngine.nextAction(state).type, 'repair');
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['catalog']);
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds, ['catalog']);

  state = progressionEngine.recordResult(state, conclusive(state, 'level-1-repair', { catalog: 'fail' }));
  assert.equal(state.level, 2);
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['ownership']);
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds, ['accounts', 'ownership']);
  assert.doesNotMatch(JSON.stringify(progressionEngine.promptSelection(state)), /accounts|recovery|search/);
});

test('one failed branch stops while passed branches continue through any number of levels', () => {
  let state = progressionEngine.initialize(fixture());
  state = progressionEngine.recordResult(state, conclusive(state, 'l1-a', {
    accounts: 'pass', catalog: 'fail',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'l1-b', {
    catalog: 'fail',
  }));
  assert.equal(state.nodes.catalog.status, 'exhausted');
  assert.equal(state.nodes.search.status, 'blocked');
  assert.equal(state.nodes.ownership.status, 'active');

  state = progressionEngine.recordResult(state, conclusive(state, 'l2', {
    accounts: 'pass', ownership: 'pass',
  }));
  assert.equal(state.level, 3);
  assert.equal(state.nodes.recovery.status, 'active');
  assert.equal(state.nodes.recommendations.status, 'blocked');
  state = progressionEngine.recordResult(state, conclusive(state, 'l3', {
    accounts: 'pass', ownership: 'pass', recovery: 'pass',
  }));
  assert.equal(state.phase, 'complete');
  assert.deepEqual(progressionEngine.nextAction(state), { type: 'complete' });
});

test('no passed node at a level stops progression', () => {
  const value = fixture();
  value.strikes.default = 1;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'l1', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.equal(state.phase, 'complete');
  assert.equal(state.nodes.ownership.status, 'blocked');
  assert.equal(state.nodes.search.status, 'blocked');
  assert.equal(state.nodes.recovery.status, 'locked');
});

test('inconclusive attempts do not consume strikes or change selections', () => {
  const state = progressionEngine.initialize(fixture());
  const next = progressionEngine.recordResult(state, {
    attemptId: 'provider-503', outcome: 'inconclusive', reason: 'provider failure',
  });
  assert.deepEqual(next.strikes, state.strikes);
  assert.deepEqual(progressionEngine.promptSelection(next), progressionEngine.promptSelection(state));
  assert.equal(next.attempts.at(-1).outcome, 'inconclusive');
});

test('dependency regressions return to the prompt and prevent descendant passes', () => {
  const value = fixture();
  value.strikes.levels[2] = 2;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'l1', {
    accounts: 'pass', catalog: 'pass',
  }));
  const accountsCheck = progressionEngine.gradingSelection(state).checks
    .find(check => check.nodeId === 'accounts').id;
  state = progressionEngine.recordResult(state, conclusive(state, 'l2-regression', {
    accounts: { [accountsCheck]: 'fail' },
    ownership: 'pass',
    catalog: 'pass',
    search: 'pass',
  }));
  assert.equal(state.phase, 'active');
  assert.equal(state.nodes.accounts.status, 'regressed');
  assert.equal(state.nodes.ownership.status, 'active');
  assert.equal(state.nodes.search.status, 'passed');
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['accounts', 'ownership']);
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds, ['accounts', 'ownership']);

  state = progressionEngine.recordResult(state, conclusive(state, 'l2-repair', {
    accounts: 'pass', ownership: 'pass',
  }));
  assert.equal(state.level, 3);
  assert.equal(state.nodes.recovery.status, 'active');
  assert.equal(state.nodes.recommendations.status, 'active');
});

test('questline percentages use partial check points and the overall score weights paths equally', () => {
  const value = fixture();
  value.strikes.default = 1;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'partial', {
    accounts: { 'check.accounts.1': 'pass', 'check.accounts.2': 'fail' },
    catalog: 'pass',
  }));
  const score = progressionEngine.score(state);
  assert.deepEqual(score.questlines.map(item => ({ id: item.id,
    passedPoints: item.passedPoints, availablePoints: item.availablePoints })), [
    { id: 'discovery', passedPoints: 1, availablePoints: 3 },
    { id: 'identity', passedPoints: 2, availablePoints: 5 },
  ]);
  assert.equal(score.averagePercentage, ((1 / 3) * 100 + (2 / 5) * 100) / 2);
  assert.deepEqual(score.uniqueChecks, {
    passedPoints: 3,
    availablePoints: 8,
    percentage: 37.5,
  });
});

test('conclusive results fail closed when selected nodes or checks are missing or repeated', () => {
  const state = progressionEngine.initialize(fixture());
  const missing = conclusive(state, 'missing', {});
  missing.nodes.pop();
  assert.throws(() => progressionEngine.recordResult(state, missing), /missing nodes/);
  const repeated = conclusive(state, 'repeated', {});
  repeated.nodes[0].checks.push(structuredClone(repeated.nodes[0].checks[0]));
  assert.throws(() => progressionEngine.recordResult(state, repeated), /repeats check/);
});
