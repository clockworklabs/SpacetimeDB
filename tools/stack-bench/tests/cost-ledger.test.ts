import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createBackendLease, publicBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { retainedRunCost } from '../src/evidence/retained-run-cost.js';
import assert from 'node:assert/strict';
import test from 'node:test';

import { durableCostLedger, runCostEvidence } from '../src/evidence/cost-proof.js';
import { recordedExecutionSpend } from '../src/campaigns/campaign-inspection.js';
import { executionSpend } from '../src/campaigns/campaign-report.js';
import { spend } from '../dashboard/public/format.js';
import type { CostRun } from '../src/evidence/cost-proof.js';

const receipt = (costUsd: number) => ({ invocation: 1, receipt: {
  complete: true, reconciled: true, error: null, costUsd, exact: true,
} });

test('cost ledger uses stored receipts and does not reprice usage', () => {
  const run: CostRun = { id: 'run-1', totals: { costUsd: 3.5, costComplete: true },
    pricing: { id: 'recorded-pricing' }, levels: [{ level: 1, buildSessions: [{
      costUsd: 2, costComplete: true, costReceipts: [receipt(2)],
      usage: { input: 999_999_999, output: 999_999_999 },
    }], repairSessions: [{ costUsd: 1.5, costComplete: true,
      costReceipts: [receipt(1.5)] }] }] };
  const ledger = durableCostLedger(run);
  assert.equal(ledger.receiptCostUsd, 3.5);
  assert.equal(ledger.differenceUsd, 0);
  assert.equal(ledger.complete, true);
  assert.equal(ledger.exact, true);
});

test('cost ledger stays complete and marks itself inexact when a receipt charged a ceiling', () => {
  const estimated = { invocation: 1, receipt: {
    complete: true, reconciled: true, error: null, costUsd: 2, exact: false } };
  const ledger = durableCostLedger({ id: 'run-estimated', totals: { costUsd: 2, costComplete: true },
    levels: [{ level: 1, buildSessions: [{ costUsd: 2, costComplete: true,
      costReceipts: [estimated] }] }] });
  assert.equal(ledger.complete, true);
  assert.equal(ledger.exact, false);
  assert.deepEqual(ledger.rows.map(row => row.exact), [false]);
});

test('cost ledger rejects incomplete receipt proof', () => {
  const run: CostRun = { id: 'run-2', totals: { costUsd: 2, costComplete: false },
    levels: [{ level: 1, buildSessions: [{
      costUsd: 2, costComplete: false, costReceipts: [{ invocation: 1, receipt: {
        complete: false, reconciled: false, error: 'provider result was incomplete', costUsd: 2,
      } }],
    }] }] };
  const ledger = durableCostLedger(run);
  assert.equal(ledger.receiptCostUsd, 2);
  assert.equal(ledger.complete, false);
});

test('cost ledger accepts a complete non-billable session without receipts', () => {
  const run: CostRun = { id: 'run-3', totals: { costUsd: 0, costComplete: true },
    levels: [{ level: 1, buildSessions: [{
      costUsd: 0, costComplete: true, costReceipts: [],
    }] }] };
  const ledger = durableCostLedger(run);
  assert.equal(ledger.receiptCostUsd, 0);
  assert.equal(ledger.complete, true);
});

test('cost ledger preserves every same-depth feature build session', () => {
  const run: CostRun = { id: 'run-feature', totals: { costUsd: 3, costComplete: true },
    levels: [{ level: 1, buildSessions: [
      { costUsd: 1, costComplete: true, costReceipts: [receipt(1)] },
      { costUsd: 2, costComplete: true, costReceipts: [receipt(2)] },
    ] }] };
  const ledger = durableCostLedger(run);
  assert.equal(ledger.rows.length, 2);
  assert.equal(ledger.receiptCostUsd, 3);
  assert.equal(ledger.complete, true);
});

test('cost evidence rejects negative and malformed money values', () => {
  assert.throws(() => durableCostLedger({ levels: [{ level: 1, buildSessions: [{
    costUsd: -1,
  }] }] }), /costUsd/);
  assert.throws(() => durableCostLedger({ totals: { costUsd: -1 } }), /totals\.costUsd/);
  assert.throws(() => durableCostLedger({ levels: [{ level: 1, buildSessions: [{
    costUsd: 1, costReceipts: [{ receipt: { complete: true, reconciled: true, error: null } }],
  }] }] }), /receipt\[0\]\.costUsd/);
});

test('missing totals never become an exact zero, even with complete set', () => {
  for (const totals of [undefined, {}, { costComplete: true }, { costComplete: true, costUsd: null }]) {
    assert.deepEqual(runCostEvidence({ totals }), { status: 'unknown', costUsd: null });
  }
  assert.deepEqual(runCostEvidence({ totals: { costComplete: true, costUsd: 0 } }),
    { status: 'exact', costUsd: 0 });
});

test('resumed execution spend excludes inherited costs and retains current bounds', () => {
  const resumed = { progressionResume: { inheritedLevels: [1] },
    totals: { costUsd: 5, currentExecutionCostUsd: 3, costComplete: true }, levels: [
      { level: 1, buildSessions: [{ costUsd: 2, costComplete: true, costReceipts: [receipt(2)] }] },
      { level: 2, buildSessions: [{ costUsd: 3, costComplete: true,
        costReceipts: [{ receipt: { ...receipt(3).receipt, exact: false } }] }] },
    ] };
  assert.deepEqual(runCostEvidence(resumed, 'execution'), { status: 'upper-bound', costUsd: 3 });
  delete (resumed.totals as Partial<typeof resumed.totals>).currentExecutionCostUsd;
  assert.deepEqual(runCostEvidence(resumed, 'execution'), { status: 'unknown', costUsd: null });
});


test('interrupted totals cannot hide a later checkpoint or claim complete spend', () => {
  const run: CostRun = { totals: { costUsd: 2, costComplete: true },
    levels: [{ level: 2, buildSessions: [{ costUsd: 2, costComplete: true, costReceipts: [receipt(2)] }] }],
    checkpoints: [{ executionCost: { status: 'exact', costUsd: 12 } }] };
  const cost = runCostEvidence(run, 'execution');
  assert.equal(cost.status, 'unknown');
  const recorded = recordedExecutionSpend(run);
  assert.deepEqual(recorded, { status: 'exact', costUsd: 12 });
  const total = executionSpend([{ cost, recorded }]);
  assert.equal(total.status, 'unknown');
  assert.equal(total.knownCostUsd, 12);
  assert.match(spend(total), /\$12.00 recorded/);
  run.checkpoints = [];
  run.progressionStatus = { phase: 'active' };
  assert.equal(runCostEvidence(run).status, 'unknown');
});


test('retained broker accounting is bound to the run lease and keeps interrupted requests incomplete', () => {
  const root = mkdtempSync(join(tmpdir(), 'retained-cost-'));
  const lease = createBackendLease({ runId: 'retained', backend: 'spacetime', track: 'ecommerce',
    runIndex: 0, module: 'app', serverUri: 'http://127.0.0.1:3000', dataDir: join(root, 'data') });
  const directory = join(root, lease.runId);
  mkdirSync(join(directory, 'stack-bench-credential-broker-one'), { recursive: true });
  writeBackendLease(join(directory, 'backend-lease.json'), lease);
  const ledger = { schemaVersion: 4, model: 'claude-sonnet-5', maxBudgetUsd: 50,
    acceptedRequests: 2, billableRequests: 2, completedBillableRequests: 2,
    estimatedBillableRequests: 0, estimatedByReason: { 'no-usage': 0, 'response-aborted': 0, 'upstream-error': 0 },
    spentUsd: 12, reservedUsd: 0, usage: { input: 0, output: 0, cacheRead: 0, cacheWrite5m: 0, cacheWrite1h: 0 },
    complete: true, updatedAt: new Date().toISOString() };
  const file = join(directory, 'stack-bench-credential-broker-one', 'spend-ledger.json');
  const run = { id: lease.runId, backend: 'spacetime', model: ledger.model, backendLease: publicBackendLease(lease), levels: [] };
  try {
    writeFileSync(file, JSON.stringify(ledger));
    assert.deepEqual(retainedRunCost(run, root)?.cost, { status: 'exact', costUsd: 12 });
    writeFileSync(file, JSON.stringify({ ...ledger, complete: false, reservedUsd: 3, completedBillableRequests: 1 }));
    const partial = retainedRunCost(run, root);
    assert.equal(partial?.cost.status, 'unknown');
    assert.equal(partial?.recorded.costUsd, 12);
    assert.equal(retainedRunCost({ ...run, id: '../retained' }, root), null);
    assert.equal(retainedRunCost({ ...run, backendLease: { ownership: { markerSha256: 'wrong' } } }, root), null);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
