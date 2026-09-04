import assert from 'node:assert/strict';
import test from 'node:test';

import { durableCostLedger } from '../src/evidence/cost-proof.js';
import type { CostRun } from '../src/evidence/cost-proof.js';

const receipt = (costUsd: number) => ({ invocation: 1, receipt: {
  complete: true, reconciled: true, error: null, costUsd,
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
