import assert from 'node:assert/strict';
import test from 'node:test';

import { progressionEngine } from '../src/progression/progression-engine.js';
import {
  runProgressionMode,
  type ProgressionAttemptResult,
  type ProgressionEngine,
  type ProgressionState,
  type ProgressionWorkAction,
} from '../src/progression/progression-runner.js';

interface TestAction extends ProgressionWorkAction {
  prompt: { nodeIds: string[] };
  grading: {
    nodeIds: string[];
    checks: Array<{ id: string; nodeId: string }>;
  };
}

const definition = (): Record<string, unknown> => ({
  schemaVersion: 4,
  kind: 'progression-mode',
  id: 'runner-fixture',
  version: '1.0.0',
  state: 'draft',
  title: 'Runner fixture',
  policy: 'dependency-gated',
  strikes: { default: 1, levels: {} },
  repairSelection: 'feature',
  nodes: [
    { id: 'account', title: 'Account', questline: 'identity', dependencies: [],
      featureRefs: ['feature.account@1.0.0'], promptModules: ['prompt.account@1.0.0'],
      gradingChecks: [{ id: 'check.account', points: 1, role: 'feature' }] },
    { id: 'recovery', title: 'Recovery', questline: 'identity',
      dependencies: [{ id: 'account', reason: 'Recovery requires an account.' }],
      featureRefs: ['feature.recovery@1.0.0'], promptModules: ['prompt.recovery@1.0.0'],
      gradingChecks: [{ id: 'check.recovery', points: 1, role: 'feature' }] },
  ],
  questlines: [{ id: 'identity', title: 'Identity' }],
});

function passSelected(action: TestAction, attemptId: string): ProgressionAttemptResult {
  return {
    attemptId,
    outcome: 'conclusive',
    nodes: action.grading.nodeIds.map(nodeId => ({
      id: nodeId,
      checks: action.grading.checks.filter(check => check.nodeId === nodeId)
        .map(check => ({ id: check.id, outcome: 'pass' })),
    })),
  };
}

test('the production runner executes exact prompt and grading selections through the mode engine', async () => {
  const actions: TestAction[] = [];
  const snapshots: ProgressionState[] = [];
  const result = await runProgressionMode({
    definition: definition(),
    execute: async action => {
      const selected = action as TestAction;
      actions.push(selected);
      return passSelected(selected, `attempt-${actions.length}`);
    },
    onState: state => { snapshots.push(state); },
  });
  assert.deepEqual(actions.map(action => action.prompt.nodeIds), [['account'], ['recovery']]);
  assert.deepEqual(actions.map(action => action.grading.nodeIds), [['account'], ['account', 'recovery']]);
  assert.equal(result.status, 'terminal');
  assert.deepEqual(result.outcome, { kind: 'passed', reason: 'graph-complete', level: 2 });
  const score = result.score as { questlineAveragePercentage: number };
  assert.equal(score.questlineAveragePercentage, 100);
  assert.equal(snapshots.length, 2);
  const engine = progressionEngine as unknown as ProgressionEngine;
  assert.deepEqual(engine.resume(result.state), result.state);
});

test('the runner pauses an inconclusive attempt without publishing a zero score', async () => {
  const result = await runProgressionMode({
    definition: definition(),
    execute: async () => ({ attemptId: 'provider-failure', outcome: 'inconclusive',
      category: 'provider_failure', reason: 'provider response ended early' }),
  });
  assert.equal(result.status, 'paused');
  const outcome = result.outcome as { kind: string };
  assert.equal(outcome.kind, 'provider_failure');
  const score = result.score as {
    questlineAveragePercentage: number | null;
    questlines: Array<{ percentage: number | null }>;
  };
  assert.equal(score.questlineAveragePercentage, null);
  assert.equal(score.questlines[0]?.percentage, null);
  const state = result.state as {
    nodes: { account: { strikes: { used: number } } };
  };
  assert.equal(state.nodes.account.strikes.used, 0);
});

test('the runner rejects incomplete grading results instead of advancing', async () => {
  await assert.rejects(() => runProgressionMode({
    definition: definition(),
    execute: async () => ({ attemptId: 'bad-grade', outcome: 'conclusive', nodes: [] }),
  }), /result is missing nodes: account/);
});

test('the runner validates resumed state before executing more work', async () => {
  const engine = progressionEngine as unknown as ProgressionEngine;
  const state = engine.initialize(definition()) as {
    nodes: { account: { status: string } };
  } & ProgressionState;
  state.nodes.account.status = 'passed';
  await assert.rejects(() => runProgressionMode({
    state,
    execute: async () => { throw new Error('must not execute'); },
  }), /snapshot contradicts its event history/);
});
