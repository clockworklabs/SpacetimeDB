import assert from 'node:assert/strict';
import test from 'node:test';

import { GRADER_SOURCE_TIMEOUT_MS, gradingRunTimeoutMs, gradingSourceTimeoutMs, selectedGradingSourceCount }
  from '../src/runtime/grading-timeout.js';
import { mutationGradeTimeoutMs } from '../src/evidence/mutation-control.js';

test('grading source count deduplicates checks that share one scenario source', () => {
  assert.equal(selectedGradingSourceCount(
    [{ source: 'scenarios/accounts.json' }, { source: 'scenarios/accounts.json' }],
    [{ source: 'scenarios/orders.json' }],
  ), 2);
});

test('grading timeout gives each selected source one child deadline', () => {
  assert.equal(gradingRunTimeoutMs(0), 20 * 60_000);
  assert.equal(gradingRunTimeoutMs(1), 20 * 60_000 + GRADER_SOURCE_TIMEOUT_MS);
  assert.equal(gradingRunTimeoutMs(6), 110 * 60_000);
});

test('grading timeout remains bounded for a full catalog', () => {
  assert.equal(gradingRunTimeoutMs(92), 120 * 60_000);
  assert.equal(gradingRunTimeoutMs(Number.MAX_SAFE_INTEGER), 120 * 60_000);
  assert.throws(() => gradingRunTimeoutMs(-1), /non-negative safe integer/);
  assert.throws(() => gradingRunTimeoutMs(1.5), /non-negative safe integer/);
});

test('large selected pack fits child and parent deadlines without bypassing the mutation batch limit', () => {
  const packs = [
    { id: 'volume', budget: { status: 'bounded', maxRuntimeMs: 2_500_000 } },
    { id: 'accounts', budget: { status: 'bounded', maxRuntimeMs: 40_000 } },
  ] as const;
  const checks = [{ source: 'catalog.json', packId: 'volume' }];
  const child = gradingSourceTimeoutMs(packs, checks);
  assert.equal(child, 2_560_000);
  assert.equal(gradingSourceTimeoutMs(packs, [...checks, ...checks]), child);
  assert.equal(gradingSourceTimeoutMs(packs, [{ packId: 'accounts' }]), GRADER_SOURCE_TIMEOUT_MS);
  assert.equal(gradingRunTimeoutMs(1, packs, checks), child + 20 * 60_000);
  assert.equal(mutationGradeTimeoutMs(4_000_000, 1_000, child), child);
  assert.equal(mutationGradeTimeoutMs(31_000, 1_000, child), 30_000);
  assert.equal(mutationGradeTimeoutMs(1_000, 1_000, child), 0);
  assert.throws(() => mutationGradeTimeoutMs(31_000, 1_000, NaN), /positive safe integer/);
});
