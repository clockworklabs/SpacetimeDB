import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { progressionEngine } from '../src/progression/progression-engine.js';
import { compileProgressionInput } from '../src/progression/progression-definition.js';
import { grantProgressionState, readProgressionState, writeProgressionState }
  from '../src/progression/progression-state.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import { preserveLevelCheckpoint } from '../src/runtime/source-checkpoint.js';
import { emptyArtifactIdentities } from '../src/evidence/artifacts.js';

interface TestState extends Record<string, unknown> {
  phase: string;
  events: Array<{ type: string; result: { attemptId: string } }>;
  nodes: Record<string, { status: string; strikes: { granted: number } }>;
}

interface TestProgressionEngine {
  initialize(definition: unknown): TestState;
  recordResult(state: TestState, result: unknown): TestState;
}

const engine = progressionEngine as unknown as TestProgressionEngine;

const progression = () => compileProgressionInput({
  schemaVersion: 4,
  kind: 'progression-mode',
  id: 'persisted-runner',
  version: '1.0.0',
  state: 'draft',
  title: 'Persisted runner',
  policy: 'dependency-graph',
  strikes: { default: 1, levels: {} },
  nodes: [{ id: 'account', title: 'Account', questline: 'identity', dependencies: [],
    featureRefs: ['feature.account@1.0.0'], promptModules: [],
    gradingChecks: [{ id: 'check.account', points: 1, role: 'feature' }] }],
  questlines: [{ id: 'identity', title: 'Identity', nodes: ['account'] }],
});
const stateIdentities = Object.freeze({
  featureCatalogIdentity: { id: 'persisted-runner', version: '1.0.0',
    sha256: 'c'.repeat(64), state: 'draft' },
  dependencyPolicyIdentity: { id: 'dependency-graph', version: '3.2.0',
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

test('continuation grants require the exact terminal state and dispatch through the mode engine', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-grant-'));
  const path = join(root, 'state.json');
  try {
    const input = progression();
    const scope = owner();
    const app = join(root, 'app');
    const output = join(root, 'result');
    mkdirSync(app);
    writeFileSync(join(app, 'app.js'), 'export const version = 1;\n');
    writeFileSync(join(app, 'z-conflict'), 'checkpoint source\n');
    const sourceSha256 = hashAppSource(app).sha256;
    const selectionSha256 = 'c'.repeat(64);
    const checkpoint = preserveLevelCheckpoint({ appDir: app, outputDir: output,
      runId: 'run-1', identities: emptyArtifactIdentities({
        agentAdapter: { id: scope.attempt.agentAdapter },
        stackAdapter: { id: scope.attempt.stack },
      }), track: scope.attempt.track, backend: scope.attempt.stack, level: 1,
      repair: { status: 'budget-exhausted', budgetRounds: 1, roundsUsed: 1,
        stallLimitRounds: 3, stopReason: 'budget-exhausted' },
      outcome: { kind: 'app_failure' }, selectionSha256 });
    writeFileSync(join(app, 'app.js'), 'export const version = 3;\n');
    rmSync(join(app, 'z-conflict'));
    mkdirSync(join(app, 'z-conflict', 'node_modules'), { recursive: true });
    writeFileSync(join(app, 'z-conflict', 'node_modules', 'keep'), 'runtime dependency\n');
    let state = engine.initialize(input.definition);
    state = engine.recordResult(state, { ...grade('failed', 'fail', sourceSha256),
      runId: 'run-1', selectionSha256 });
    const terminal = writeProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope, state });
    assert.throws(() => grantProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope,
      expectedStateSha256: terminal.stateSha256,
      checkpoint: { artifact: '../outside.json' },
      grant: { grantId: 'wrong-source', level: 1, nodeIds: ['account'], strikes: 2 } }),
    /escapes the progression workspace/);
    assert.equal(readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope }).stateSha256,
      terminal.stateSha256);
    try {
      symlinkSync(join(output, checkpoint.artifact), join(root, 'checkpoint-link.json'), 'file');
      assert.throws(() => grantProgressionState(path, { progression: input, ...stateIdentities,
        owner: scope,
        expectedStateSha256: terminal.stateSha256,
        checkpoint: { artifact: 'checkpoint-link.json' },
        grant: { grantId: 'linked-source', level: 1, nodeIds: ['account'], strikes: 2 } }),
      /symbolic link/);
    } catch (error: unknown) {
      if (!(error instanceof Error && 'code' in error
        && typeof error.code === 'string' && ['EPERM', 'EACCES'].includes(error.code))) {
        throw error;
      }
      t.diagnostic('symbolic-link assertion skipped because this host cannot create a file link');
    }
    assert.throws(() => grantProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope,
      expectedStateSha256: terminal.stateSha256,
      checkpoint: { artifact: join('result', checkpoint.artifact) },
      grant: { grantId: 'restore-failure', level: 1, nodeIds: ['account'], strikes: 2 } }),
    /cannot restore source file over preserved directory/);
    assert.equal(readFileSync(join(app, 'app.js'), 'utf8'), 'export const version = 3;\n');
    assert.equal(readFileSync(join(app, 'z-conflict', 'node_modules', 'keep'), 'utf8'),
      'runtime dependency\n');
    assert.equal(readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope }).stateSha256, terminal.stateSha256);
    rmSync(join(app, 'z-conflict'), { recursive: true, force: true });
    writeFileSync(join(app, 'z-conflict'), 'live source\n');
    const granted = grantProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope,
      expectedStateSha256: terminal.stateSha256,
      checkpoint: { artifact: join('result', checkpoint.artifact) },
      grant: { grantId: 'grant-1', level: 1, nodeIds: ['account'], strikes: 2 } });
    assert.equal(granted.state.phase, 'active');
    const grantedState = granted.state as unknown as TestState;
    const resume = granted.resume as {
      source: { directory: string; sha256: string };
      actionSha256: string;
    };
    assert.equal(grantedState.nodes.account!.strikes.granted, 2);
    assert.deepEqual(grantedState.events.map(event => event.type),
      ['attempt-recorded', 'strikes-granted']);
    assert.equal(resume.source.directory, 'app');
    assert.equal(resume.source.sha256, sourceSha256);
    assert.match(resume.actionSha256, /^[a-f0-9]{64}$/);
    assert.equal(readFileSync(join(app, 'app.js'), 'utf8'), 'export const version = 1;\n');
    assert.throws(() => grantProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope,
      expectedStateSha256: terminal.stateSha256,
      checkpoint: { artifact: join('result', checkpoint.artifact) },
      grant: { grantId: 'grant-2', level: 1, nodeIds: ['account'], strikes: 1 } }),
    /state changed/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
