import assert from 'node:assert/strict';
import test from 'node:test';

import { GRADER_SOURCE_TIMEOUT_MS, gradingRunTimeoutMs, selectedGradingSourceCount }
  from '../src/runtime/grading-timeout.mjs';

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
