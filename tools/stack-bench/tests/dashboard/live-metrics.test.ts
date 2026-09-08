import assert from 'node:assert/strict';
import test from 'node:test';
import { parseRunProgress } from '../../dashboard/dashboard-model.js';
import { elapsed } from '../../dashboard/public/format.js';
import { compareCampaign, type MetricAttempt } from '../../dashboard/public/metrics.js';

test('cost per valid run uses only completed comparable runs with complete exact costs', () => {
  const run: MetricAttempt = {
    id: 'valid', stack: 'example', status: 'completed', execution: null, dependency: null,
    spend: { status: 'exact', costUsd: 2 }, result: { levels: [{
      level: 1, firstScore: null, firstAbort: null, finalScore: { score: 5, max: 10 },
      used: 0, repairStatus: null, outcome: null, durationSec: null, costUsd: 2,
      cost: { status: 'exact', costUsd: 2 }, failures: [], regressions: 0, repairs: null, continued: false,
    }] },
  };
  const second = { ...run, id: 'second', spend: { status: 'exact' as const, costUsd: 10 } };
  const attempts = [run, second,
    { ...run, id: 'invalid', status: 'invalid', spend: { status: 'exact' as const, costUsd: 100 } },
    { ...run, id: 'running', status: 'running', spend: { status: 'exact' as const, costUsd: 50 } }];
  const row = compareCampaign({ attempts }).rows[0]!;
  assert.equal(row.n, 2);
  assert.equal(row.costPerValidRun, 6);
  assert.equal(row.spendSoFar, 162);
  for (const spend of [{ status: 'unknown' as const, costUsd: null },
    { status: 'upper-bound' as const, costUsd: 10 }]) {
    assert.equal(compareCampaign({ attempts: [run, { ...second, spend }] }).rows[0]!.costPerValidRun, null);
  }
  assert.equal(compareCampaign({ attempts: [run, { ...second, comparisonKey: 'different' }] }).rows[0]!.costPerValidRun, null);
  assert.equal(compareCampaign({ attempts: [{ ...run, status: 'running' }] }).rows[0]!.costPerValidRun, null);
});

test('execution elapsed time uses the execution clock, and stops at completion', () => {
  const start = '2026-09-07T12:00:00Z';
  const end = '2026-09-07T12:03:00Z';
  const now = Date.parse('2026-09-07T12:05:00Z');
  assert.equal(elapsed(start, null, now), '5m');
  assert.equal(elapsed(start, end, now), '3m');
  assert.equal(elapsed(null, null, now), '—');
  assert.equal(elapsed('invalid', null, now), '—');
  assert.equal(elapsed(start, null, Date.parse(start) - 1), '0m');
});

test('stopped attempts do not claim a previous grading or repair phase is live', () => {
  for (const log of ['=== postgres-l1-first (postgres) ===',
    '--- feature repair 2: Catalog ---']) {
    assert.equal(parseRunProgress(log, { running: false, status: 'invalid' }).phase,
      'Stopped without a valid result');
    assert.equal(parseRunProgress(log, { running: false, status: 'completed' }).phase, 'Finished');
    assert.equal(parseRunProgress(log, { running: false, status: 'pending' }).phase, 'Waiting to start');
    assert.notEqual(parseRunProgress(log, { running: true, status: 'running' }).phase, 'Finished');
  }
});
