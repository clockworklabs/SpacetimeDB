import assert from 'node:assert/strict';
import test from 'node:test';

import { gradeArgv, parseArgs } from '../commands/bench.mjs';
import { repairEvidenceDecision } from '../src/evidence/repair-evidence.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';

test('direct runs default to ten repair rounds while an explicit budget still wins', () => {
  assert.equal(parseArgs(['node', 'bench', '--backend', 'postgres']).fixRounds, 10);
  assert.equal(parseArgs(['node', 'bench', '--backend', 'postgres',
    '--fix-rounds', '4']).fixRounds, 4);
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
