import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { finalizeRunTotals, formatLevelSummary, gradeArgv, levelGradeIsUsable, parseArgs,
  pristineMutationBaselinePath, repairHistoryEntry, repairProgressState }
  from '../commands/bench.mjs';
import { repairEvidenceDecision } from '../src/evidence/repair-evidence.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';
import { writeArtifact } from '../src/evidence/artifacts.mjs';

test('direct runs default to ten repair rounds while an explicit budget still wins', () => {
  assert.equal(parseArgs(['node', 'bench', '--backend', 'postgres']).fixRounds, 10);
  assert.equal(parseArgs(['node', 'bench', '--backend', 'postgres',
    '--fix-rounds', '4']).fixRounds, 4);
});

test('progression level usability follows its stricter evidence result', () => {
  assert.equal(levelGradeIsUsable({ kind: 'app_failure' }), true);
  assert.equal(levelGradeIsUsable({ kind: 'app_failure' }, { outcome: 'inconclusive' }), false);
  assert.equal(levelGradeIsUsable({ kind: 'app_failure' }, { outcome: 'conclusive' }), true);
});

test('resumed dependency costs separate prior, current, and cumulative execution usage', () => {
  const run = { levels: [
    { level: 1, graded: true, score: 1, max: 1, buildCostUsd: 4,
      sessionTotals: { sessions: 1, tokens: 10, outputTokens: 2, turns: 1, durationMs: 100 } },
    { level: 2, graded: true, score: 1, max: 1, fixCostUsd: 2,
      sessionTotals: { sessions: 1, tokens: 5, outputTokens: 1, turns: 1, durationMs: 50 } },
  ], progressionResume: { inheritedLevels: [1],
    priorTotals: { costUsd: 4, costComplete: true } } };
  finalizeRunTotals(run, 1_000, { now: 3_000 });
  assert.equal(run.totals.priorExecutionCostUsd, 4);
  assert.equal(run.totals.currentExecutionCostUsd, 2);
  assert.equal(run.totals.cumulativeCostUsd, 6);
  assert.equal(run.totals.costUsd, 6);
  assert.equal(run.totals.costComplete, true);
});

test('ungraded level summaries contain useful failure values', () => {
  assert.equal(formatLevelSummary({ level: 1, graded: false,
    error: 'coding-session-failed', buildCostUsd: 1.25, durationMs: 4_400 }),
  'L1: NOT GRADED | 0 repairs | $1.25 total ($0.00 repairs) | '
    + 'stopped: coding session failed | 4s');
});

test('dependency campaign progression rejects an incomplete or unbound plan reference', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-progression-plan-'));
  try {
    const path = join(root, 'plan.json');
    writeArtifact(path, { kind: 'campaign_plan', id: 'plan', payload: {} });
    assert.throws(() => parseArgs(['node', 'bench', '--backend', 'postgres',
      '--campaign-file', path, '--feature-catalog-sha256', 'a'.repeat(64),
      '--campaign-sha256', 'b'.repeat(64), '--campaign-attempt-id', 'attempt']),
    /compiled campaign/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('repair progress pauses only on repeated findings without a score gain', () => {
  const bundle = (score, failures) => ({ totals: { score, max: 10, contractPass: true },
    suites: {}, outcome: { kind: 'app_failure', phase: 'grading',
      appFailures: failures, inconclusive: [], harnessFailures: [] } });
  let state = repairProgressState(null, bundle(5, ['a']));
  state = repairProgressState(state, bundle(5, ['a']));
  assert.equal(state.stalledRounds, 1);
  state = repairProgressState(state, bundle(5, ['b']));
  assert.equal(state.stalledRounds, 0);
  state = repairProgressState(state, bundle(6, ['b']));
  assert.equal(state.stalledRounds, 0);
  state = repairProgressState(state, bundle(6, ['b']));
  assert.equal(state.stalledRounds, 1);
});

test('repair history reports score movement and exact remaining findings', () => {
  const before = { totals: { score: 4, max: 6 }, suites: {}, outcome: {
    kind: 'app_failure', phase: 'grading', appFailures: ['suite/a'] } };
  const after = { totals: { score: 5, max: 6 }, suites: { lint: { results: [
    { id: 'review-average', status: 'FAIL' },
  ] } }, outcome: { kind: 'app_failure', phase: 'grading',
    appFailures: ['contract-lint', 'suite/b'] } };
  assert.deepEqual(repairHistoryEntry(2, before, after, 'kept'), {
    round: 2, beforeScore: 4, beforeMax: 6, afterScore: 5, afterMax: 6,
    result: 'kept', remainingFailures: ['suite/b', 'testing-interface/review-average'],
  });
});

test('mutation shard coordinates are paired', () => {
  const args = parseArgs(['node', 'bench', '--backend', 'postgres',
    '--mutation-shard-index', '1', '--mutation-shard-count', '3']);
  assert.equal(args.mutationShardIndex, 1);
  assert.equal(args.mutationShardCount, 3);
  assert.throws(() => parseArgs(['node', 'bench', '--backend', 'postgres',
    '--mutation-shard-index', '1']), /must be supplied together/);
});

test('mutation-only execution is restricted to model-free reference runs', () => {
  assert.throws(() => parseArgs(['node', 'bench', '--backend', 'postgres',
    '--reference-mutation-only']), /requires a mutation-bound reference fixture/);
  const args = parseArgs(['node', 'bench', '--backend', 'postgres', '--fix-rounds', '0',
    '--agent-adapter', 'reference-fixture', '--app', 'fixture', '--mutations', 'mutations.json',
    '--reference-mutation-only', '--mutation-baseline-bundle', 'baseline.json']);
  assert.equal(args.referenceMutationOnly, true);
  assert.match(args.mutationBaselineBundle, /baseline\.json$/);
  assert.throws(() => parseArgs(['node', 'bench', '--backend', 'postgres',
    '--mutation-baseline-bundle', 'baseline.json']), /require --mutations/);
  assert.throws(() => parseArgs(['node', 'bench', '--backend', 'postgres',
    '--mutations', 'mutations.json', '--mutation-baseline-bundle', 'baseline.json']),
  /internal reference mutation option/);
});

test('mutation control reuses the existing clean grade when it is present', () => {
  const args = { out: 'results/run', levelList: [1, 3] };
  assert.equal(pristineMutationBaselinePath(args, () => true),
    join('results/run', 'first-build-l3-grading', 'bundle.json'));
  assert.equal(pristineMutationBaselinePath(args, () => false), null);
  assert.equal(pristineMutationBaselinePath({ ...args, referenceMutationOnly: true }), null);
});

test('the first repair that makes an unstartable app gradeable is never rolled back', () => {
  const after = { suites: {}, totals: { score: 35, max: 58 } };
  for (const phase of ['application-restart', 'application-seed']) {
    const before = { outcome: { kind: 'app_failure', phase },
      suites: {}, totals: { score: 0, max: 58 } };
    assert.equal(repairEvidenceDecision(before, after).action, 'keep-setup-repair');
  }
  assert.equal(repairEvidenceDecision({ suites: {} }, after).action, 'rollback-no-comparison');
});

test('grading forwards the track startup-data expectation', () => {
  const track = loadTrack('ecommerce');
  const argv = gradeArgv({ backend: 'postgres', track: 'ecommerce', runIndex: 0,
    media: false }, '/app', 'http://localhost:6573', 'postgres-l1', 1, track, 'attempt');
  const index = argv.indexOf('--reseed-probe-expectation-json');
  assert(index > 0);
  assert.deepEqual(JSON.parse(argv[index + 1]), { jsonPath: 'items', minCount: 1 });
});

test('Spacetime grading probes the application instead of a missing API port', () => {
  const track = loadTrack('ecommerce');
  const argv = gradeArgv({ backend: 'spacetime', track: 'ecommerce', runIndex: 0,
    media: false }, '/app', 'http://localhost:6473', 'spacetime-l1', 1, track, 'attempt');
  assert.equal(argv.includes('--reseed-probe'), false);
  assert.equal(argv.includes('--reseed-probe-expectation-json'), false);
});

test('grading binds scored evidence to the selected application source', () => {
  const track = loadTrack('ecommerce');
  const sourceSha256 = 'a'.repeat(64);
  const argv = gradeArgv({ backend: 'spacetime', track: 'ecommerce', runIndex: 0,
    media: false }, '/app', 'http://localhost:6481', 'spacetime-l1', 1, track, 'attempt',
  { sourceSha256 });
  const index = argv.indexOf('--source-sha256');
  assert(index > 0);
  assert.equal(argv[index + 1], sourceSha256);
});

test('later-level grading receives prior selected checks as regression scope', () => {
  const track = loadTrack('ecommerce');
  const args = { backend: 'postgres', track: 'ecommerce', runIndex: 0, media: false,
    recipeTasks: new Map([
      [1, { request: { schemaVersion: 3 }, selection: { scoredChecks: [
        { stableKey: 'prior/a' }, { stableKey: 'prior/b' },
      ] } }],
      [2, { request: { schemaVersion: 3 }, selection: { scoredChecks: [
        { stableKey: 'current/c' },
      ] } }],
    ]) };
  const argv = gradeArgv(args, '/app', 'http://localhost:6573', 'postgres-l2', 2,
    track, 'attempt');
  const index = argv.indexOf('--regression-checks-json');
  assert(index > 0);
  assert.deepEqual(JSON.parse(argv[index + 1]), ['prior/a', 'prior/b']);
});

test('dependency grading uses its exact action scope without a second regression selection', () => {
  const track = loadTrack('ecommerce');
  const args = { backend: 'postgres', track: 'ecommerce', runIndex: 0, media: false,
    progression: { identity: { policy: 'dependency-gated' } },
    recipeTasks: new Map([
      [1, { request: { schemaVersion: 3 }, selection: { scoredChecks: [
        { stableKey: 'prior/a' },
      ] } }],
      [2, { request: { schemaVersion: 3 }, selection: { scoredChecks: [
        { stableKey: 'prior/a' }, { stableKey: 'current/b' },
      ] } }],
    ]) };
  const argv = gradeArgv(args, '/app', 'http://localhost:6573', 'postgres-l2', 2,
    track, 'attempt');
  assert.equal(argv.includes('--regression-checks-json'), false);
});
