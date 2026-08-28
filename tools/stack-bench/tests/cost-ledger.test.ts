import assert from 'node:assert/strict';
import test from 'node:test';

import { durableCostLedger } from '../commands/cost-ledger.js';
import type { CostRun } from '../src/evidence/cost-proof.mjs';

const receipt = (costUsd: number) => ({ invocation: 1, receipt: {
  complete: true, reconciled: true, error: null, costUsd,
} });

test('cost ledger uses stored receipts and does not reprice usage', () => {
  const run: CostRun = { id: 'run-1', totals: { costUsd: 3.5, costComplete: true },
    pricing: { id: 'recorded-pricing' }, levels: [{ level: 1, buildSession: {
      costUsd: 2, costComplete: true, costReceipts: [receipt(2)],
      usage: { input: 999_999_999, output: 999_999_999 },
    }, fixSessions: [{ costUsd: 1.5, costComplete: true,
      costReceipts: [receipt(1.5)] }] }] };
  const ledger = durableCostLedger(run);
  assert.equal(ledger.receiptCostUsd, 3.5);
  assert.equal(ledger.differenceUsd, 0);
  assert.equal(ledger.complete, true);
});

test('cost ledger rejects incomplete receipt proof', () => {
  const run: CostRun = { id: 'run-2', totals: { costUsd: 2, costComplete: false },
    levels: [{ level: 1, buildSession: {
      costUsd: 2, costComplete: false, costReceipts: [{ invocation: 1, receipt: {
        complete: false, reconciled: false, error: 'provider result was incomplete', costUsd: 2,
      } }],
    } }] };
  const ledger = durableCostLedger(run);
  assert.equal(ledger.receiptCostUsd, 2);
  assert.equal(ledger.complete, false);
});

test('cost ledger accepts a complete non-billable session without receipts', () => {
  const run: CostRun = { id: 'run-3', totals: { costUsd: 0, costComplete: true },
    levels: [{ level: 1, buildSession: {
      costUsd: 0, costComplete: true, costReceipts: [],
    } }] };
  const ledger = durableCostLedger(run);
  assert.equal(ledger.receiptCostUsd, 0);
  assert.equal(ledger.complete, true);
});
