import assert from 'node:assert/strict';
import test from 'node:test';

import { gradeArgv, parseArgs, repairHistoryEntry, repairProgressState } from '../commands/bench.mjs';
import { repairEvidenceDecision } from '../src/evidence/repair-evidence.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';

test('direct runs default to ten repair rounds while an explicit budget still wins', () => {
  assert.equal(parseArgs(['node', 'bench', '--backend', 'postgres']).fixRounds, 10);
  assert.equal(parseArgs(['node', 'bench', '--backend', 'postgres',
    '--fix-rounds', '4']).fixRounds, 4);
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
