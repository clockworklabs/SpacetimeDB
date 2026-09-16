import assert from 'node:assert/strict';
import test from 'node:test';
import { parseRunProgress } from '../../dashboard/dashboard-model.js';
import { elapsed, executionClock } from '../../dashboard/public/format.js';
import { attemptMetrics, compareCampaign, outputSilentMinutes, type MetricAttempt } from '../../dashboard/public/metrics.js';
import { recordedExecutionSpend } from '../../src/evidence/run-checkpoints.js';
import { runCostEvidence } from '../../src/evidence/cost-proof.js';
import { campaignActiveDurationMs, campaignFirstBuildRate, campaignMeasuredRunCost,
  campaignRunMetrics, executionSpend } from '../../src/campaigns/campaign-report.js';

test('dashboard and export share retry, prior-repair, wait and pause definitions', () => {
  const session = (costUsd: number) => ({ costUsd, costComplete: true,
    costReceipts: [{ receipt: { costUsd, exact: true, complete: true, reconciled: true, error: null } }] });
  const failed = { id: 'failed', totals: { costUsd: 2, costComplete: true },
    levels: [{ level: 1, buildSessions: [session(2)] }] };
  const measured = { id: 'measured', totals: { costUsd: 3, costComplete: true, durationSec: 100, pausedDurationSec: 10 },
    levels: [
      { level: 1, firstBuild: { score: 5, max: 10 }, buildSessions: [session(1)], repairSessions: [session(1)],
        sessionTotals: { providerThrottle: { waitedMs: 20_000 } } },
      { level: 2, firstBuild: { score: 10, max: 10 }, buildSessions: [session(1)] },
    ] };
  const cost = campaignMeasuredRunCost(measured, [failed, measured]);
  const report = campaignRunMetrics(measured as Parameters<typeof campaignRunMetrics>[0]);
  const first = campaignFirstBuildRate(measured), active = campaignActiveDurationMs(measured);
  assert.equal(first, 0.75); assert.equal(active, 70_000);
  const spend = executionSpend([failed, measured].map(run => ({ cost: runCostEvidence(run, 'execution') })));
  const attempt: MetricAttempt = { id: 'measured', stack: 'example', status: 'completed', execution: null,
    dependency: null, measuredCost: cost, spend, result: {
      firstBuildRate: first, activeDurationSec: active! / 1000, durationSec: 100,
      levels: [{ level: 2, firstScore: { score: 10, max: 10 }, firstAbort: null,
        finalScore: { score: 10, max: 10 }, used: 1, repairStatus: null, outcome: null,
        durationSec: 100, costUsd: 3, cost, failures: [], regressions: 0, repairs: null, continued: false }],
    } };
  const row = compareCampaign({ attempts: [attempt] }).rows[0]!;
  assert.equal(row.first, report.firstBuildScoreRate);
  assert.equal(row.duration, report.totalDurationMs! / 1000);
  assert.equal(row.costPerValidRun, report.totalCostUsd);
  assert.equal(row.costPerValidRun, 3); assert.equal(row.spendSoFar, 5);
  assert.equal(attemptMetrics({ ...attempt, result: { ...attempt.result!, firstBuildRate: null } })!.raw.first, null);
  const resumed = { id: 'resumed', progressionResume: { priorRunId: 'measured', inheritedLevels: [1, 2] },
    totals: { currentExecutionCostUsd: 1 }, levels: [{ level: 3, buildSessions: [session(1)] }] };
  assert.deepEqual(campaignMeasuredRunCost(resumed, [failed, measured, resumed]), { status: 'exact', costUsd: 4 });
  assert.equal(campaignMeasuredRunCost(resumed, [resumed]).status, 'unknown');
});

test('live spend validates recorded sessions without treating them as final execution cost', () => {
  const session = (costUsd: number) => ({ costUsd, costComplete: true,
    costReceipts: [{ receipt: { costUsd, exact: true, complete: true, reconciled: true, error: null } }] });
  const run = { progressionResume: { inheritedLevels: [1] }, levels: [
    { level: 1, buildSessions: [session(100)] },
    { level: 2, buildSessions: [session(2)], repairSessions: [session(3)] },
  ] };
  assert.deepEqual(recordedExecutionSpend(run), { status: 'exact', costUsd: 5 });
  assert.equal(runCostEvidence(run, 'execution').status, 'unknown');
  run.levels[1]!.repairSessions!.push(session(4));
  assert.equal(recordedExecutionSpend(run).costUsd, 9);
  run.levels[1]!.repairSessions![0]!.costComplete = false;
  assert.equal(recordedExecutionSpend(run).status, 'unknown');
  assert.equal(recordedExecutionSpend({}).status, 'unknown');
  assert.deepEqual(recordedExecutionSpend({ ...run, checkpoints: [
    { executionCost: { status: 'exact', costUsd: 12 } },
  ] }), { status: 'exact', costUsd: 12 }, 'active-level checkpoints include repairs missing from completed levels');
  assert.deepEqual(recordedExecutionSpend({ ...run, checkpoints: [
    { executionCost: { status: 'upper-bound', costUsd: 15 } },
  ] }), { status: 'upper-bound', costUsd: 15 });
  assert.equal(recordedExecutionSpend({ ...run, checkpoints: [
    { executionCost: { status: 'unknown', costUsd: null } },
  ] }).status, 'unknown');
});

test('cost per valid run uses only completed comparable runs with complete exact costs', () => {
  const run: MetricAttempt = {
    id: 'valid', stack: 'example', status: 'completed', execution: null, dependency: null,
    spend: { status: 'exact', costUsd: 2 }, measuredCost: { status: 'exact', costUsd: 2 }, result: { levels: [{
      level: 1, firstScore: null, firstAbort: null, finalScore: { score: 5, max: 10 },
      used: 0, repairStatus: null, outcome: null, durationSec: null, costUsd: 2,
      cost: { status: 'exact', costUsd: 2 }, failures: [], regressions: 0, repairs: null, continued: false,
    }] },
  };
  const second = { ...run, id: 'second', spend: { status: 'exact' as const, costUsd: 10 }, measuredCost: { status: 'exact' as const, costUsd: 10 } };
  const attempts = [run, second,
    { ...run, id: 'invalid', status: 'invalid', spend: { status: 'exact' as const, costUsd: 100 } },
    { ...run, id: 'running', status: 'running', spend: { status: 'exact' as const, costUsd: 50 } }];
  const row = compareCampaign({ attempts }).rows[0]!;
  assert.equal(row.n, 2);
  assert.equal(row.costPerValidRun, 6);
  assert.equal(row.spendSoFar, 162);
  for (const spend of [{ status: 'unknown' as const, costUsd: null },
    { status: 'upper-bound' as const, costUsd: 10 }]) {
    assert.equal(compareCampaign({ attempts: [run, { ...second, measuredCost: spend }] }).rows[0]!.costPerValidRun, null);
  }
  assert.equal(compareCampaign({ attempts: [run, { ...second, comparisonKey: 'different' }] }).rows[0]!.costPerValidRun, null);
  assert.equal(compareCampaign({ attempts: [{ ...run, status: 'running' }] }).rows[0]!.costPerValidRun, null);
});

test('execution elapsed time uses the execution clock, and stops at completion', () => {
  const start = '2026-09-07T12:00:00Z';
  const end = '2026-09-07T12:03:00Z';
  const now = Date.parse('2026-09-07T12:05:00Z');
  assert.equal(elapsed(start, null, now), '5m 0s');
  assert.equal(elapsed(start, null, now + 1000), '5m 1s');
  assert.equal(elapsed(start, end, now), '3m 0s');
  assert.match(executionClock(start, null), /data-started-at=/);
  assert.doesNotMatch(executionClock(start, end), /data-started-at=/);
  assert.equal(elapsed(null, null, now), '—');
  assert.equal(elapsed('invalid', null, now), '—');
  assert.equal(elapsed(start, null, Date.parse(start) - 1), '0m 0s');
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

test('activity warnings require agent evidence and exclude planned pauses', () => {
  const now = Date.parse('2026-09-10T12:00:00Z');
  const attempt = { status: 'running', activityUpdatedAt: '2026-09-10T11:40:00Z' };
  assert.equal(outputSilentMinutes(attempt, now), 20);
  assert.equal(outputSilentMinutes({ ...attempt, paused: true }, now), 0);
  assert.equal(outputSilentMinutes({ ...attempt, status: 'completed' }, now), 0);
  assert.equal(outputSilentMinutes({ status: 'running' }, now), 0);
  assert.equal(outputSilentMinutes({ ...attempt, activityUpdatedAt: 'invalid' }, now), 0);
});
