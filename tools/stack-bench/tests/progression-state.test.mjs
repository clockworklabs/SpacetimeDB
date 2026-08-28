import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { progressionEngine } from '../src/progression/progression-engine.mjs';
import { compileProgressionInput } from '../src/progression/progression-definition.mjs';
import { runPersistedProgressionMode } from '../src/progression/progression-runner.mjs';
import { grantProgressionState, readProgressionState, writeProgressionState }
  from '../src/progression/progression-state.mjs';
import { hashAppSource } from '../src/runtime/source-snapshot.mjs';
import { preserveLevelCheckpoint } from '../src/runtime/source-checkpoint.mjs';
import { emptyArtifactIdentities } from '../src/evidence/artifacts.mjs';

const progression = () => compileProgressionInput({
  schemaVersion: 3,
  kind: 'progression-mode',
  id: 'persisted-runner',
  version: '1.0.0',
  state: 'draft',
  title: 'Persisted runner',
  policy: 'dependency-gated',
  strikes: { default: 1, levels: {} },
  nodes: [{ id: 'account', title: 'Account', questline: 'identity', dependencies: [],
    featureRefs: ['feature.account@1.0.0'], promptModules: [],
    gradingChecks: [{ id: 'check.account', points: 1 }] }],
  questlines: [{ id: 'identity', title: 'Identity', nodes: ['account'] }],
});
const stateIdentities = Object.freeze({
  featureCatalogIdentity: { id: 'persisted-runner', version: '1.0.0',
    sha256: 'c'.repeat(64), state: 'draft' },
  dependencyPolicyIdentity: { id: 'dependency-gated', version: '2.1.0',
    sha256: 'd'.repeat(64) },
});
const owner = () => ({ schemaVersion: 1,
  campaign: { id: 'campaign', version: '1.0.0', sha256: 'a'.repeat(64) },
  attempt: { id: 'campaign-r1', track: 'ecommerce', stack: 'postgres',
    agentAdapter: 'claude-code', model: 'test-model', conditionSha256: 'b'.repeat(64) },
  workspace: { appDirectory: 'app' } });

const grade = (attemptId, outcome, sourceSha256) => ({ attemptId, outcome: 'conclusive',
  ...(sourceSha256 ? { sourceSha256 } : {}),
  nodes: [{ id: 'account', checks: [{ id: 'check.account', outcome }] }] });

test('progression state stores one ordered event log and a replay-verified compact snapshot', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-state-'));
  const path = join(root, 'state.json');
  try {
    const input = progression();
    let state = progressionEngine.initialize(input.definition);
    state = progressionEngine.recordResult(state, grade('first', 'fail'));
    const scope = owner(root);
    const written = writeProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope, state });
    assert.equal(written.artifact.payload.events.length, 1);
    assert.equal(written.artifact.payload.snapshot.definition, undefined);
    assert.equal(written.artifact.payload.snapshot.events, undefined);
    assert.deepEqual(readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope }).state, state);
    const wrongOwner = owner(root);
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
    const scope = owner(root);
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
    assert.equal(resumed.outcome.kind, 'passed');
    const stored = readProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope });
    assert.deepEqual(stored.state.events.map(event => event.result.attemptId),
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
    const scope = owner(root);
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
    let state = progressionEngine.initialize(input.definition);
    state = progressionEngine.recordResult(state, { ...grade('failed', 'fail', sourceSha256),
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
    } catch (error) {
      if (!['EPERM', 'EACCES'].includes(error.code)) throw error;
      t.diagnostic('symbolic-link assertion skipped because this host cannot create a file link');
    }
    const granted = grantProgressionState(path, { progression: input, ...stateIdentities,
      owner: scope,
      expectedSnapshotSha256: terminal.snapshotSha256,
      checkpoint: { artifact: join('result', checkpoint.artifact) },
      grant: { grantId: 'grant-1', level: 1, nodeIds: ['account'], strikes: 2 } });
    assert.equal(granted.state.phase, 'active');
    assert.equal(granted.state.nodes.account.strikes.granted, 2);
    assert.deepEqual(granted.state.events.map(event => event.type),
      ['attempt-recorded', 'strikes-granted']);
    assert.equal(granted.resume.source.directory, 'app');
    assert.equal(granted.resume.source.sha256, sourceSha256);
    assert.match(granted.resume.actionSha256, /^[a-f0-9]{64}$/);
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
