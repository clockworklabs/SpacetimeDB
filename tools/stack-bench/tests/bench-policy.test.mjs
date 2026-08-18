import assert from 'node:assert/strict';
import test from 'node:test';

import { parseArgs, repairEvidenceDecision } from '../bench.mjs';

test('direct runs default to ten repair rounds while an explicit budget still wins', () => {
  assert.equal(parseArgs(['node', 'bench', '--backend', 'postgres']).fixRounds, 10);
  assert.equal(parseArgs(['node', 'bench', '--backend', 'postgres',
    '--fix-rounds', '4']).fixRounds, 4);
});

test('the first repair that makes an unstartable app gradeable is never rolled back', () => {
  const before = { outcome: { kind: 'app_failure', phase: 'application-restart' },
    suites: {}, totals: { score: 0, max: 58 } };
  const after = { suites: {}, totals: { score: 35, max: 58 } };
  assert.equal(repairEvidenceDecision(before, after).action, 'keep-setup-repair');
  assert.equal(repairEvidenceDecision({ suites: {} }, after).action, 'rollback-no-comparison');
});
