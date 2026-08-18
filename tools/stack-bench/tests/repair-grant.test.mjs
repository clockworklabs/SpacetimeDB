import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { emptyArtifactIdentities, writeRunJson } from '../src/evidence/artifacts.mjs';
import { compareRepairBaseline, createRepairGrant, inspectRepairParent } from '../src/runtime/repair-grant.mjs';
import { preserveLevelCheckpoint } from '../src/runtime/source-checkpoint.mjs';

function parentFixture(root, overrides = {}) {
  const app = join(root, 'app');
  mkdirSync(app, { recursive: true });
  writeFileSync(join(app, 'app.js'), 'export const broken = true;\n');
  const id = 'parent-run';
  const repair = overrides.repair ?? { status: 'budget-exhausted', budgetRounds: 3,
    roundsUsed: 3, stopReason: 'budget-exhausted' };
  const outcome = overrides.outcome ?? { kind: 'app_failure', phase: 'grading', reason: null,
    appFailures: ['feature/failure'], inconclusive: [], harnessFailures: [] };
  const selection = overrides.selection ?? { sha256: 'c'.repeat(64), scoredPoints: 2,
    recipe: { id: 'ecommerce.l1-standard', version: '1.0.0' } };
  const identities = overrides.identities ?? emptyArtifactIdentities({
    agentAdapter: { id: 'deterministic', version: '1.0.0', sha256: 'b'.repeat(64) },
    stackAdapter: { id: 'stub', version: '1.0.0' },
  });
  const checkpoint = preserveLevelCheckpoint({ appDir: app, outputDir: root, runId: id,
    identities, track: 'loop', backend: 'stub', level: 1, repair, outcome,
    selectionSha256: selection.sha256 });
  const level = { level: 1, graded: true, score: 1, max: 2, fixRounds: repair.roundsUsed,
    repair, outcome, selection, checkpoint, buildCostUsd: 2, fixCostUsd: 0.5, durationSec: 60 };
  const downstream = { level: 2, graded: true, score: 4, max: 4, fixRounds: 0,
    repair: { status: 'not-needed', budgetRounds: 3, roundsUsed: 0, stopReason: 'not-needed' },
    outcome: { kind: 'passed', phase: 'grading', reason: null, appFailures: [], inconclusive: [],
      harnessFailures: [] }, buildCostUsd: 99, fixCostUsd: 0, durationSec: 100 };
  writeRunJson(join(root, 'run.json'), {
    id,
    kind: 'benchmark_run',
    startedAt: '2026-08-16T12:00:00.000Z',
    completedAt: '2026-08-16T12:01:00.000Z',
    identities,
    track: 'loop', backend: 'stub', model: 'deterministic', guidance: 'prescribed',
    condition: null, selectionRequest: { packs: [], checks: [] }, skills: [],
    runtime: { buildImage: 'test-build-image', url: 'http://localhost:1234' },
    backendLease: { runIndex: 0 },
    levels: [level, downstream], contaminated: false,
    totals: { score: 5, max: 6, fixRounds: repair.roundsUsed, costUsd: 101.5, durationSec: 160 },
    outcome: { kind: outcome.kind, levels: { 1: outcome, 2: downstream.outcome } },
  });
  return { app, level, checkpoint };
}

test('a finite grant is derived only from the exact exhausted parent checkpoint', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-grant-'));
  try {
    const fixture = parentFixture(root);
    const resolved = createRepairGrant(root, { level: 1, rounds: 4 });
    assert.equal(resolved.parent.id, 'parent-run');
    assert.equal(resolved.sourcePath, join(root, fixture.checkpoint.directory));
    assert.equal(resolved.grant.roundsGranted, 4);
    assert.equal(resolved.grant.cumulativeRoundsBefore, 3);
    assert.equal(resolved.grant.cumulativeRoundsAfter, 3);
    assert.equal(resolved.configuration.agentAdapter, 'deterministic');
    assert.equal(resolved.configuration.recipe, 'ecommerce.l1-standard@1.0.0');
    assert.equal(resolved.configuration.runIndex, 0);
    assert.equal(resolved.configuration.url, 'http://localhost:1234');
    assert.equal(resolved.grant.cumulativeCostBeforeUsd, 2.5);
    assert.equal(resolved.grant.cumulativeDurationBeforeSec, 60);
    assert.deepEqual(resolved.grant.downstreamLevelsToRerun, [2]);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('repair grants reject incomplete evidence, remaining budget, and changed source bytes', () => {
  const incompleteRoot = mkdtempSync(join(tmpdir(), 'stack-bench-repair-incomplete-'));
  const remainingRoot = mkdtempSync(join(tmpdir(), 'stack-bench-repair-remaining-'));
  const changedRoot = mkdtempSync(join(tmpdir(), 'stack-bench-repair-changed-'));
  try {
    parentFixture(incompleteRoot, { outcome: { kind: 'app_failure', phase: 'grading', reason: null,
      appFailures: ['failure'], inconclusive: ['missing'], harnessFailures: [] } });
    assert.throws(() => inspectRepairParent(incompleteRoot, 1), /complete conclusive measurement/);

    parentFixture(remainingRoot, { repair: { status: 'incomplete', budgetRounds: 3,
      roundsUsed: 2, stopReason: 'agent-session-failure' } });
    assert.throws(() => inspectRepairParent(remainingRoot, 1), /did not exhaust/);

    const changed = parentFixture(changedRoot);
    writeFileSync(join(changedRoot, changed.checkpoint.directory, 'app.js'),
      'export const broken = false;\n');
    assert.throws(() => inspectRepairParent(changedRoot, 1), /source bytes do not match/);
  } finally {
    rmSync(incompleteRoot, { recursive: true, force: true });
    rmSync(remainingRoot, { recursive: true, force: true });
    rmSync(changedRoot, { recursive: true, force: true });
  }
});

test('repair grant rounds are finite and explicitly bounded', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-repair-rounds-'));
  try {
    parentFixture(root);
    for (const rounds of [0, 21, 1.5]) {
      assert.throws(() => createRepairGrant(root, { level: 1, rounds }), /integer from 1 through 20/);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a continuation baseline must reproduce score, scope, and exact failed criteria', () => {
  const parent = { score: 4, max: 6, selection: { sha256: 'a'.repeat(64) }, outcome: {
    kind: 'app_failure', appFailures: ['suite/b', 'suite/a'], inconclusive: [], harnessFailures: [],
  } };
  const exact = compareRepairBaseline(parent, { score: 4, max: 6,
    selectionSha256: 'a'.repeat(64), sourceSha256: 'b'.repeat(64),
    expectedSourceSha256: 'b'.repeat(64), outcome: { kind: 'app_failure',
      appFailures: ['suite/a', 'suite/b'], inconclusive: [], harnessFailures: [] } });
  assert.deepEqual(exact, { reproduced: true, mismatches: [] });
  const changed = compareRepairBaseline(parent, { score: 4, max: 6,
    selectionSha256: 'a'.repeat(64), sourceSha256: 'b'.repeat(64),
    expectedSourceSha256: 'b'.repeat(64), outcome: { kind: 'app_failure',
      appFailures: ['suite/a'], inconclusive: ['suite/b'], harnessFailures: [] } });
  assert.equal(changed.reproduced, false);
  assert.deepEqual(changed.mismatches, ['measurement', 'failed criteria']);
  const changedSource = compareRepairBaseline(parent, { score: 4, max: 6,
    selectionSha256: 'a'.repeat(64), sourceSha256: 'c'.repeat(64),
    expectedSourceSha256: 'b'.repeat(64), outcome: { kind: 'app_failure',
      appFailures: ['suite/a', 'suite/b'], inconclusive: [], harnessFailures: [] } });
  assert.deepEqual(changedSource, { reproduced: false, mismatches: ['source'] });
});
