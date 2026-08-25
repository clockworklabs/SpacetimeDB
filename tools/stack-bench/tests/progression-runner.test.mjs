import assert from 'node:assert/strict';
import test from 'node:test';

import { progressionEngine } from '../src/progression/progression-engine.mjs';
import { runProgressionMode } from '../src/progression/progression-runner.mjs';

const definition = () => ({
  schemaVersion: 1,
  kind: 'progression-mode',
  id: 'runner-fixture',
  version: '1.0.0',
  policy: 'dependency-gated',
  strikes: { default: 1, levels: {} },
  nodes: [
    { id: 'account', level: 1, dependencies: [],
      featureRefs: ['feature.account@1.0.0'], promptModules: ['prompt.account'],
      gradingChecks: [{ id: 'check.account', points: 1 }] },
    { id: 'recovery', level: 2, dependencies: ['account'],
      featureRefs: ['feature.recovery@1.0.0'], promptModules: ['prompt.recovery'],
      gradingChecks: [{ id: 'check.recovery', points: 1 }] },
  ],
  questlines: [{ id: 'identity', title: 'Identity', nodes: ['account', 'recovery'] }],
});

const passSelected = (action, attemptId) => ({
  attemptId,
  outcome: 'conclusive',
  nodes: action.grading.nodeIds.map(nodeId => ({
    id: nodeId,
    checks: action.grading.checks.filter(check => check.nodeId === nodeId)
      .map(check => ({ id: check.id, outcome: 'pass' })),
  })),
});

test('the production runner executes exact prompt and grading selections through the mode engine', async () => {
  const actions = [];
  const snapshots = [];
  const result = await runProgressionMode({
    definition: definition(),
    execute: async action => {
      actions.push(action);
      return passSelected(action, `attempt-${actions.length}`);
    },
    onState: async state => snapshots.push(state),
  });
  assert.deepEqual(actions.map(action => action.prompt.nodeIds), [['account'], ['recovery']]);
  assert.deepEqual(actions.map(action => action.grading.nodeIds), [['account'], ['account', 'recovery']]);
  assert.equal(result.status, 'terminal');
  assert.deepEqual(result.outcome, { kind: 'passed', reason: 'graph-complete', level: 2 });
  assert.equal(result.score.averagePercentage, 100);
  assert.equal(snapshots.length, 2);
  assert.deepEqual(progressionEngine.resume(result.state), result.state);
});

test('the runner pauses an inconclusive attempt without publishing a zero score', async () => {
  const result = await runProgressionMode({
    definition: definition(),
    execute: async () => ({ attemptId: 'provider-failure', outcome: 'inconclusive',
      category: 'provider_failure',
      reason: 'provider response ended early' }),
  });
  assert.equal(result.status, 'paused');
  assert.equal(result.outcome.kind, 'provider_failure');
  assert.equal(result.score.averagePercentage, null);
  assert.equal(result.score.questlines[0].percentage, null);
  assert.equal(result.state.strikes['1'].used, 0);
});

test('the runner rejects incomplete grading results instead of advancing', async () => {
  await assert.rejects(() => runProgressionMode({
    definition: definition(),
    execute: async () => ({ attemptId: 'bad-grade', outcome: 'conclusive', nodes: [] }),
  }), /result is missing nodes: account/);
});

test('the runner validates resumed state before executing more work', async () => {
  const state = progressionEngine.initialize(definition());
  state.nodes.account.status = 'passed';
  await assert.rejects(() => runProgressionMode({
    state,
    execute: async () => { throw new Error('must not execute'); },
  }), /snapshot contradicts its event history/);
});
