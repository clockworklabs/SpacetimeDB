import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { progressionEngine } from '../src/progression/progression-engine.js';
import { compileProgressionInput } from '../src/progression/progression-definition.js';
import { runPersistedProgressionMode } from '../src/progression/progression-runner.js';
import { grantProgressionState, readProgressionState, writeProgressionState }
  from '../src/progression/progression-state.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import { preserveLevelCheckpoint } from '../src/runtime/source-checkpoint.js';
import { emptyArtifactIdentities } from '../src/evidence/artifacts.js';

interface TestState extends Record<string, unknown> {
  phase: string;
  events: Array<{ type: string; result: { attemptId: string } }>;
  nodes: Record<string, { strikes: { granted: number } }>;
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
  policy: 'dependency-gated',
  strikes: { default: 1, levels: {} },
  nodes: [{ id: 'account', title: 'Account', questline: 'identity', dependencies: [],
    featureRefs: ['feature.account@1.0.0'], promptModules: [],
    gradingChecks: [{ id: 'check.account', points: 1, role: 'feature' }] }],
  questlines: [{ id: 'identity', title: 'Identity', nodes: ['account'] }],
});
const stateIdentities = Object.freeze({
  featureCatalogIdentity: { id: 'persisted-runner', version: '1.0.0',
    sha256: 'c'.repeat(64), state: 'draft' },
  dependencyPolicyIdentity: { id: 'dependency-gated', version: '3.0.0',
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

test('progression state stores one ordered event log and a replay-verified compact snapshot', () => {
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
    assert.equal(written.artifact.payload.snapshot.definition, undefined);
    assert.equal(written.artifact.payload.snapshot.events, undefined);
    assert.deepEqual(readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope }).state, state);
    const wrongOwner = owner();
    wrongOwner.attempt.id = 'campaign-r2';
    assert.throws(() => readProgressionState(path, { progression: input, ...stateIdentities,
      owner: wrongOwner }),
      /wrong campaign attempt owner/);

    const changed = JSON.parse(readFileSync(path, 'utf8'));
    changed.payload.snapshot.phase = 'active';
    writeFileSync(path, JSON.stringify(changed));
    assert.throws(() => readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope }),
      /snapshot identity does not match/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('persisted runner resumes the exact paused state and atomically appends the next event', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-resume-'));
  const path = join(root, 'state.json');
  try {
    const input = progression();
    const scope = owner();
    const first = await runPersistedProgressionMode({ progression: input, ...stateIdentities,
      owner: scope, statePath: path,
      execute: async () => ({ attemptId: 'provider', outcome: 'inconclusive',
        category: 'provider_failure', reason: 'response ended early' }) });
    assert.equal(first.status, 'paused');
    assert.equal(readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope }).state.events.length, 1);

    const resumed = await runPersistedProgressionMode({ progression: input, ...stateIdentities,
      owner: scope, statePath: path,
      execute: async () => grade('second', 'pass') });
    assert.equal(resumed.status, 'terminal');
    assert.equal((resumed.outcome as { kind: string }).kind, 'passed');
    const stored = readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope });
    assert.deepEqual(stored.state.events.map(event => {
      assert.ok(event.result);
      return event.result.attemptId;
    }),
      ['provider', 'second']);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('continuation grants require the exact terminal snapshot and dispatch through the mode engine', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-grant-'));
  const path = join(root, 'state.json');
  try {
    const input = progression();
    const scope = owner();
    const app = join(root, 'app');
    const output = join(root, 'result');
    mkdirSync(app);
    writeFileSync(join(app, 'app.js'), 'export const version = 1;\n');
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
    let state = engine.initialize(input.definition);
    state = engine.recordResult(state, { ...grade('failed', 'fail', sourceSha256),
      runId: 'run-1', selectionSha256 });
    const terminal = writeProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope, state });
    assert.throws(() => grantProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope,
      expectedSnapshotSha256: terminal.snapshotSha256,
      checkpoint: { artifact: '../outside.json' },
      grant: { grantId: 'wrong-source', level: 1, nodeIds: ['account'], strikes: 2 } }),
    /escapes the progression workspace/);
    assert.equal(readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope }).snapshotSha256,
      terminal.snapshotSha256);
    try {
      symlinkSync(join(output, checkpoint.artifact), join(root, 'checkpoint-link.json'), 'file');
      assert.throws(() => grantProgressionState(path, { progression: input, ...stateIdentities,
        owner: scope,
        expectedSnapshotSha256: terminal.snapshotSha256,
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
    const granted = grantProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope,
      expectedSnapshotSha256: terminal.snapshotSha256,
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
      expectedSnapshotSha256: terminal.snapshotSha256,
      checkpoint: { artifact: join('result', checkpoint.artifact) },
      grant: { grantId: 'grant-2', level: 1, nodeIds: ['account'], strikes: 1 } }),
    /state changed/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
