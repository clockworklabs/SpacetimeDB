import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { progressionEngine } from '../src/progression/progression-engine.js';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.js';
import { sha256 } from '../src/evidence/provenance.js';
import { compileProgressionInput } from '../src/progression/progression-definition.js';
import { grantProgressionState, readProgressionState, writeProgressionState }
  from '../src/progression/progression-state.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';

interface TestState extends Record<string, unknown> {
  phase: string;
  events: Array<{ type: string; result: { attemptId: string } }>;
  nodes: Record<string, { status: string; repairs: { used: number } }>;
}

interface TestProgressionEngine {
  initialize(definition: unknown): TestState;
  recordResult(state: TestState, result: unknown): TestState;
  nextAction(state: TestState): unknown;
}

const engine = progressionEngine as unknown as TestProgressionEngine;

const progression = () => compileProgressionInput({
  schemaVersion: 5,
  kind: 'progression-mode',
  id: 'persisted-runner',
  version: '1.0.0',
  state: 'draft',
  title: 'Persisted runner',
  policy: 'dependency-graph',
  repair: { selection: 'feature', budget: { total: 0 } },
  unchangedFailureLimit: 3,
  workSelection: 'progressive',
  nodes: [{ id: 'account', title: 'Account', questline: 'identity', dependencies: [],
    featureRefs: ['feature.account@1.0.0'], promptModules: [],
    gradingChecks: [{ id: 'check.account', points: 1, role: 'feature' }] }],
  questlines: [{ id: 'identity', title: 'Identity', nodes: ['account'] }],
});
const stateIdentities = Object.freeze({
  featureCatalogIdentity: { id: 'persisted-runner', version: '1.0.0',
    sha256: 'c'.repeat(64), state: 'draft' },
  dependencyPolicyIdentity: { id: 'dependency-graph', version: '4.0.0',
    sha256: 'd'.repeat(64) },
});
const owner = () => ({ schemaVersion: 1,
  campaign: { id: 'campaign', version: '1.0.0', sha256: 'a'.repeat(64) },
  attempt: { id: 'campaign-r1', track: 'ecommerce', stack: 'postgres',
    agentAdapter: 'claude-code', model: 'test-model', conditionSha256: 'b'.repeat(64) },
  workspace: { appDirectory: 'app' } });

const grade = (attemptId: string, outcome: string, sourceSha256?: string) =>
  ({ attemptId, outcome: 'conclusive',
  ...(sourceSha256 ? { sourceSha256 } : {}),
  nodes: [{ id: 'account', checks: [{ id: 'check.account', outcome }] }] });

test('progression state stores one replay-verified event log', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-state-'));
  const path = join(root, 'state.json');
  try {
    const input = progression();
    let state = engine.initialize(input.definition);
    state = engine.recordResult(state, grade('first', 'fail'));
    const scope = owner();
    const written = writeProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope, state });
    assert.equal(written.artifact.payload.events.length, 1);
    assert.equal(written.artifact.payload.snapshot, undefined);
    assert.match(written.stateSha256, /^[a-f0-9]{64}$/);
    assert.deepEqual(readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope }).state, state);
    const wrongOwner = owner();
    wrongOwner.attempt.id = 'campaign-r2';
    assert.throws(() => readProgressionState(path, { progression: input, ...stateIdentities,
      owner: wrongOwner }),
      /wrong campaign attempt owner/);

    const contradictory = structuredClone(state);
    contradictory.nodes.account!.status = 'active';
    assert.throws(() => writeProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope, state: contradictory }), /contradicts its event history/);

    const changed = JSON.parse(readFileSync(path, 'utf8'));
    changed.payload.events[0].sequence = 2;
    writeFileSync(path, JSON.stringify(changed));
    assert.throws(() => readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope }),
      /state identity does not match/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('continuation grants keep the current accepted source and dispatch through the mode engine', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-grant-'));
  const path = join(root, 'state.json');
  try {
    const input = progression();
    const scope = owner();
    const app = join(root, 'app');
    mkdirSync(app);
    writeFileSync(join(app, 'app.js'), 'export const version = 3;\n');
    const sourceSha256 = hashAppSource(app).sha256;
    let state = engine.initialize(input.definition);
    state = engine.recordResult(state, { ...grade('failed', 'fail', sourceSha256),
      runId: 'run-1', selectionSha256: 'c'.repeat(64) });
    const resume = { actionSha256: sha256(canonicalDefinitionJson(engine.nextAction(state))), source: {
      directory: 'app', sha256: sourceSha256, files: hashAppSource(app).files.length,
    } };
    const terminal = writeProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope, state, resume });

    writeFileSync(join(app, 'app.js'), 'export const version = 4;\n');
    assert.throws(() => grantProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope,
      expectedStateSha256: terminal.stateSha256,
      grant: { grantId: 'wrong-source', level: 1, nodeIds: ['account'], repairs: 2 } }),
    /current accepted source does not match/);
    assert.equal(readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope }).stateSha256,
      terminal.stateSha256);

    writeFileSync(join(app, 'app.js'), 'export const version = 3;\n');
    const granted = grantProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope,
      expectedStateSha256: terminal.stateSha256,
      grant: { grantId: 'grant-1', level: 1, nodeIds: ['account'], repairs: 2 } });
    assert.equal(granted.state.phase, 'active');
    const grantedState = granted.state as unknown as TestState;
    const grantedResume = granted.resume as {
      source: { directory: string; sha256: string };
      actionSha256: string;
    };
    assert.equal(grantedState.nodes.account!.repairs.used, 0);
    assert.deepEqual(grantedState.events.map(event => event.type),
      ['attempt-recorded', 'repairs-granted']);
    assert.equal(grantedResume.source.directory, 'app');
    assert.equal(grantedResume.source.sha256, sourceSha256);
    assert.match(grantedResume.actionSha256, /^[a-f0-9]{64}$/);
    assert.equal(readFileSync(join(app, 'app.js'), 'utf8'), 'export const version = 3;\n');
    assert.throws(() => grantProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope,
      expectedStateSha256: terminal.stateSha256,
      grant: { grantId: 'grant-2', level: 1, nodeIds: ['account'], repairs: 1 } }),
    /state changed/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
