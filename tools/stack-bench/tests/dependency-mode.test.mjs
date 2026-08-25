import assert from 'node:assert/strict';
import test from 'node:test';

import { compileDependencyMode } from '../src/progression/dependency-mode.mjs';
import { createProgressionEngine, progressionEngine } from '../src/progression/progression-engine.mjs';

const node = (id, dependencies, questline, points = [1]) => ({
  id,
  title: id,
  questline,
  dependencies: dependencies.map(dependency => ({
    id: dependency,
    reason: `${id} requires ${dependency}`,
  })),
  featureRefs: [`features.${id}@1.0.0`],
  promptModules: [`prompt.${id}@1.0.0`],
  gradingChecks: points.map((value, index) => ({ id: `check.${id}.${index + 1}`, points: value })),
});

const fixture = () => ({
  schemaVersion: 2,
  kind: 'progression-mode',
  id: 'storefront-paths',
  version: '1.0.0',
  state: 'draft',
  title: 'Storefront paths',
  policy: 'dependency-gated',
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
  assert.deepEqual(compiled.questlines.find(item => item.id === 'identity').nodes,
    ['accounts', 'ownership', 'recovery']);
  assert.deepEqual(compiled.strikes, { levels: { 1: 2, 2: 1, 3: 2 } });
});

test('invalid graphs, questlines, and strike budgets fail before execution', async t => {
  const cases = [
    ['duplicate node', value => value.nodes.push(structuredClone(value.nodes[0])), /duplicates/],
    ['missing parent', value => { value.nodes[2].dependencies[0].id = 'missing'; }, /unknown parent/],
    ['dependency cycle', value => {
      value.nodes[0].dependencies = [{ id: 'ownership', reason: 'cycle' }];
    }, /dependency cycle/],
    ['missing dependency reason', value => { value.nodes[2].dependencies[0].reason = ''; }, /non-empty string/],
    ['authored level', value => { value.nodes[0].level = 1; }, /compiled level and dependency reasons/],
    ['invalid default strikes', value => { value.strikes.default = 0; }, /positive integer/],
    ['invalid level strikes', value => { value.strikes.levels[2] = -1; }, /positive integer/],
    ['negative check points', value => { value.nodes[0].gradingChecks[0].points = -1; }, /positive integer/],
    ['zero-point gate check', value => {
      value.nodes[0].gradingChecks[0].points = 0;
    }, /positive integer/],
    ['missing budget', value => { value.strikes = { levels: { 1: 1 } }; }, /is required/],
    ['unknown questline', value => { value.nodes[0].questline = 'missing'; }, /unknown questline/],
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
    compile: value => value,
    initialize: value => ({ policy: value.policy }),
    activeNodes: () => ['fake'],
    promptSelection: () => ({ fake: 'prompt' }),
    gradingSelection: () => ({ fake: 'grade' }),
    recordResult: state => state,
    grantStrikes: state => state,
    resume: state => state,
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
    ['prompt.accounts@1.0.0', 'prompt.catalog@1.0.0']);
  assert.doesNotMatch(JSON.stringify(progressionEngine.promptSelection(state)), /ownership|recovery|search/);

  state = progressionEngine.recordResult(state, conclusive(state, 'level-1', {
    accounts: 'pass',
    catalog: 'fail',
  }));
  assert.equal(progressionEngine.nextAction(state).type, 'repair');
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['catalog']);
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds, ['accounts', 'catalog']);

  state = progressionEngine.recordResult(state, conclusive(state, 'level-1-repair', { catalog: 'fail' }));
  assert.equal(state.level, 2);
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['ownership']);
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds, ['accounts', 'ownership']);
  assert.doesNotMatch(JSON.stringify(progressionEngine.promptSelection(state)), /accounts|recovery|search/);
});

test('passed siblings are regraded and return to repair work if they regress', () => {
  const value = fixture();
  value.strikes.default = 3;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'first-grade', {
    accounts: 'pass', catalog: 'fail',
  }));
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['catalog']);
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds, ['accounts', 'catalog']);

  state = progressionEngine.recordResult(state, conclusive(state, 'sibling-regression', {
    accounts: 'fail', catalog: 'pass',
  }));
  assert.equal(state.nodes.accounts.status, 'regressed');
  assert.equal(state.nodes.catalog.status, 'passed');
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['accounts']);
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds, ['accounts', 'catalog']);
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
  assert.equal(state.phase, 'terminal');
  assert.deepEqual(progressionEngine.nextAction(state), {
    type: 'terminal', outcome: { kind: 'partial', reason: 'graph-complete', level: 3 },
  });
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
    kind: 'failed', reason: 'no-unlocked-nodes', level: 1, blockedLevel: 2,
  });
  assert.equal(state.nodes.ownership.status, 'blocked');
  assert.equal(state.nodes.search.status, 'blocked');
  assert.equal(state.nodes.recovery.status, 'locked');
});

test('inconclusive attempts do not consume strikes or change selections', () => {
  const state = progressionEngine.initialize(fixture());
  const beforeScore = progressionEngine.score(state);
  assert.equal(beforeScore.averagePercentage, null);
  assert(beforeScore.questlines.every(questline => questline.percentage === null));
  const next = progressionEngine.recordResult(state, {
    attemptId: 'provider-503', outcome: 'inconclusive', category: 'provider_failure',
    reason: 'provider failure',
  });
  assert.deepEqual(next.strikes, state.strikes);
  assert.deepEqual(progressionEngine.promptSelection(next), progressionEngine.promptSelection(state));
  assert.equal(next.attempts.at(-1).outcome, 'inconclusive');
  const score = progressionEngine.score(next);
  assert.equal(score.averagePercentage, null);
  assert.equal(score.attempts.inconclusive, 1);
  assert.equal(score.attempts.inconclusiveByCategory.provider_failure, 1);
  assert(score.questlines.every(questline => questline.ungradedPoints === questline.availablePoints));
});

test('a terminal failed run can receive an auditable continuation grant', () => {
  const value = fixture();
  value.strikes.default = 1;
  let state = progressionEngine.initialize(value);
  assert.throws(() => progressionEngine.grantStrikes(state,
    { grantId: 'too-early', level: 1, strikes: 1 }), /only after progression terminates/);
  state = progressionEngine.recordResult(state, conclusive(state, 'failed-l1', {
    accounts: 'fail', catalog: 'fail',
  }));
  assert.deepEqual(state.terminalOutcome, {
    kind: 'failed', reason: 'no-unlocked-nodes', level: 1, blockedLevel: 2,
  });
  state = progressionEngine.grantStrikes(state, { grantId: 'grant-1', level: 1, strikes: 2 });
  assert.equal(state.phase, 'active');
  assert.deepEqual(state.strikes['1'], {
    initialBudget: 1, granted: 2, budget: 3, used: 1,
  });
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['accounts', 'catalog']);
  assert.deepEqual(state.events.map(event => event.type), ['attempt-recorded', 'strikes-granted']);
  state = progressionEngine.recordResult(state, conclusive(state, 'continued-l1', {
    accounts: 'pass', catalog: 'pass',
  }));
  assert.equal(state.level, 2);
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['ownership', 'search']);
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

  state = progressionEngine.grantStrikes(state, { grantId: 'catalog-grant', level: 1, strikes: 1 });
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['catalog']);
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds, ['accounts', 'catalog']);
  state = progressionEngine.recordResult(state, conclusive(state, 'continued-catalog', {
    accounts: 'pass', catalog: 'pass',
  }));
  assert.equal(state.level, 2);
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['ownership', 'search']);
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds,
    ['accounts', 'catalog', 'ownership', 'search']);
  assert.equal(state.nodes.ownership.checks['check.ownership.1'], null);
  assert.equal(state.nodes.recovery.checks['check.recovery.1'], null);
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
  assert.equal(state.nodes.catalog.exhaustedAtLevel, 1);
  assert.equal(state.nodes.ownership.exhaustedAtLevel, 2);

  state = progressionEngine.grantStrikes(state, {
    grantId: 'l1-only', level: 1, strikes: 1,
  });
  assert.equal(state.nodes.ownership.status, 'exhausted');
  assert.equal(state.nodes.ownership.exhaustedAtLevel, 2);
  assert.equal(state.nodes.ownership.checks['check.ownership.1'], null);
  state = progressionEngine.recordResult(state, conclusive(state, 'repair-l1', {
    accounts: 'pass', catalog: 'pass',
  }));
  assert.equal(state.phase, 'terminal');
  assert.equal(state.terminalOutcome.blockedLevel, 2);
  assert.throws(() => progressionEngine.grantStrikes(state,
    { grantId: 'wrong-level', level: 1, strikes: 1 }), /no exhausted repair target/);
  state = progressionEngine.grantStrikes(state,
    { grantId: 'l2-needed', level: 2, strikes: 1 });
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds, ['ownership', 'search']);
});

test('state resumes by replay and rejects contradictory snapshots or event sequences', () => {
  let state = progressionEngine.initialize(fixture());
  state = progressionEngine.recordResult(state, conclusive(state, 'first', {
    accounts: 'pass', catalog: 'fail',
  }));
  assert.deepEqual(progressionEngine.resume(state), state);
  const contradictory = structuredClone(state);
  contradictory.nodes.accounts.status = 'regressed';
  assert.throws(() => progressionEngine.resume(contradictory), /contradicts its event history/);
  const missingEvent = structuredClone(state);
  missingEvent.events[0].sequence = 2;
  assert.throws(() => progressionEngine.resume(missingEvent), /event sequence 2 must be 1/);
});

test('a child with multiple parents opens only when every parent passes', () => {
  const value = {
    schemaVersion: 2,
    kind: 'progression-mode',
    id: 'multi-parent',
    version: '1.0.0',
    state: 'draft',
    title: 'Multi-parent graph',
    policy: 'dependency-gated',
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
  assert.equal(state.nodes.checkout.status, 'blocked');

  state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'both-parents', {
    account: 'pass', cart: 'pass',
  }));
  assert.equal(state.nodes.checkout.status, 'active');
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds,
    ['account', 'cart', 'checkout']);
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
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds,
    ['accounts', 'ownership']);
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds,
    ['accounts', 'catalog', 'ownership', 'search']);

  state = progressionEngine.recordResult(state, conclusive(state, 'l2-repair', {
    accounts: 'pass', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.level, 3);
  assert.equal(state.nodes.recovery.status, 'active');
  assert.equal(state.nodes.recommendations.status, 'active');
});

test('passed nodes outside the active branch are regression guards', () => {
  const value = fixture();
  value.strikes.levels[2] = 2;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'l1-pass', {
    accounts: 'pass', catalog: 'pass',
  }));
  assert.deepEqual(progressionEngine.gradingSelection(state).nodeIds,
    ['accounts', 'catalog', 'ownership', 'search']);
  state = progressionEngine.recordResult(state, conclusive(state, 'catalog-regressed', {
    accounts: 'pass', catalog: 'fail', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.nodes.catalog.status, 'regressed');
  assert.equal(state.nodes.ownership.status, 'passed');
  assert.equal(state.nodes.search.status, 'active');
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds,
    ['catalog', 'search']);
});

test('a continuation grant reopens a regressed ancestor and its affected child', () => {
  const value = fixture();
  value.strikes.levels[2] = 1;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'l1-pass', {
    accounts: 'pass', catalog: 'pass',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'l2-regression', {
    accounts: 'fail', catalog: 'pass', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.level, 3);
  assert.equal(state.nodes.recommendations.status, 'active');
  state = progressionEngine.recordResult(state, conclusive(state, 'l3-other-branch', {
    catalog: 'pass', search: 'pass', recommendations: 'pass',
  }));
  assert.equal(state.phase, 'terminal');
  assert.equal(state.nodes.accounts.status, 'exhausted');
  assert.equal(state.nodes.accounts.exhaustedAtLevel, 2);
  assert.equal(state.nodes.ownership.status, 'exhausted');
  assert.equal(state.nodes.search.status, 'passed');

  state = progressionEngine.grantStrikes(state, {
    grantId: 'repair-regression', level: 2, strikes: 1,
  });
  assert.deepEqual(progressionEngine.promptSelection(state).nodeIds,
    ['accounts', 'ownership']);
  state = progressionEngine.recordResult(state, conclusive(state, 'l2-repair', {
    accounts: 'pass', catalog: 'pass', ownership: 'pass', search: 'pass',
  }));
  assert.equal(state.level, 3);
  assert.equal(state.nodes.recovery.status, 'active');
  assert.equal(state.nodes.recommendations.status, 'active');
});

test('questline percentages use partial check points and the overall score weights groups equally', () => {
  const value = fixture();
  value.strikes.default = 1;
  let state = progressionEngine.initialize(value);
  state = progressionEngine.recordResult(state, conclusive(state, 'partial', {
    accounts: { 'check.accounts.1': 'pass', 'check.accounts.2': 'fail' },
    catalog: 'pass',
  }));
  assert.equal(progressionEngine.score(state).averagePercentage, null);
  state = progressionEngine.recordResult(state, conclusive(state, 'search', {
    catalog: 'pass', search: 'pass',
  }));
  state = progressionEngine.recordResult(state, conclusive(state, 'recommendations', {
    catalog: 'pass', search: 'pass', recommendations: 'pass',
  }));
  const score = progressionEngine.score(state);
  assert.deepEqual(score.questlines.map(item => ({ id: item.id,
    passedPoints: item.passedPoints, availablePoints: item.availablePoints })), [
    { id: 'discovery', passedPoints: 3, availablePoints: 3 },
    { id: 'identity', passedPoints: 2, availablePoints: 5 },
  ]);
  assert.equal(score.averagePercentage, (100 + (2 / 5) * 100) / 2);
  assert.deepEqual(score.uniqueChecks, {
    passedPoints: 5,
    failedPoints: 1,
    gradedPoints: 6,
    ungradedPoints: 2,
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
  repeated.nodes[0].checks.push(structuredClone(repeated.nodes[0].checks[0]));
  assert.throws(() => progressionEngine.recordResult(state, repeated), /repeats check/);
});
