import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { checkoutDifferences, checkoutId, checkoutMinor, checkoutStateSchema, verifyCheckoutSchema }
  from '../src/stacks/checkout-state.js';
import type { CheckoutState } from '../src/stacks/checkout-state.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';

function states(): { before: CheckoutState; prepared: CheckoutState; after: CheckoutState } {
  const before: CheckoutState = { accountId: 'a', itemId: 'i', priceMinor: 1999, cart: [],
    stock: [{ warehouseId: 'w', quantity: 10 }], reservations: [], orders: [], payments: [], orphanOrderLines: 0 };
  const prepared = structuredClone(before);
  prepared.cart = [{ itemId: 'i', quantity: 1 }];
  prepared.stock[0]!.quantity = 9;
  prepared.reservations = [{ itemId: 'i', warehouseId: 'w', quantity: 1 }];
  const after = structuredClone(before);
  after.stock[0]!.quantity = 9;
  after.orders = [{ id: 'o', accountId: 'a', totalMinor: 1999, status: 'pending',
    lines: [{ itemId: 'i', quantity: 1, priceMinor: 1999, allocations: [{ warehouseId: 'w', quantity: 1 }] }] }];
  after.payments = [{ id: 'p', orderId: 'o', amountMinor: 1999, status: 'paid' }];
  return { before, prepared, after };
}

test('checkout reconciliation accepts stock reservation and atomic checkout alternatives', () => {
  const { before, prepared, after } = states();
  assert.deepEqual(checkoutDifferences(before, prepared, after, 1), []);
  prepared.stock = structuredClone(before.stock);
  prepared.reservations = [];
  assert.deepEqual(checkoutDifferences(before, prepared, after, 1), []);
  for (const snapshot of [before, prepared, after]) assert.deepEqual(checkoutStateSchema.parse(snapshot), snapshot);
});

test('one order can split a product across warehouse lines', () => {
  const { before, prepared, after } = states();
  before.stock.push({ warehouseId: 'west', quantity: 5 });
  prepared.stock.push({ warehouseId: 'west', quantity: 4 });
  after.stock.push({ warehouseId: 'west', quantity: 4 });
  prepared.cart[0]!.quantity = 2;
  prepared.reservations.push({ itemId: 'i', warehouseId: 'west', quantity: 1 });
  after.orders[0]!.lines.push({ itemId: 'i', quantity: 1, priceMinor: 1999, allocations: [{ warehouseId: 'west', quantity: 1 }] });
  after.orders[0]!.totalMinor *= 2;
  after.payments[0]!.amountMinor *= 2;
  assert.deepEqual(checkoutDifferences(before, prepared, after, 2), []);
});

test('checkout reconciliation detects duplicate, partial, wrong-owner and reject-all effects', () => {
  const defects: Record<string, (state: ReturnType<typeof states>) => void> = {
    duplicate: ({ after }) => { after.orders.push({ ...structuredClone(after.orders[0]!), id: 'o2' }); },
    duplicatePayment: ({ after }) => { after.payments.push({ ...after.payments[0]!, id: 'p2' }); },
    wrongOwner: ({ after }) => { after.orders[0]!.accountId = 'someone-else'; },
    wrongQuantity: ({ after }) => { after.orders[0]!.lines[0]!.quantity = 2; },
    wrongPrice: ({ after }) => { after.orders[0]!.lines[0]!.priceMinor = 1; },
    wrongTotal: ({ after }) => { after.orders[0]!.totalMinor = 1; },
    wrongPayment: ({ after }) => { after.payments[0]!.orderId = 'another-order'; },
    missingPayment: ({ after }) => { after.payments = []; },
    cartRetained: ({ after, prepared }) => { after.cart = prepared.cart; },
    reservationRetained: ({ after, prepared }) => { after.reservations = prepared.reservations; },
    wrongAllocation: ({ after }) => { after.orders[0]!.lines[0]!.allocations[0]!.warehouseId = 'wrong'; },
    missingStockEffect: ({ after, before }) => { after.stock = before.stock; },
    duplicateStockEffect: ({ after }) => { after.stock[0]!.quantity--; },
    orphanLine: ({ after }) => { after.orphanOrderLines = 1; },
    rejectAll: state => { state.after = structuredClone(state.prepared); },
    erasedHistory: ({ before, prepared }) => {
      const old = { id: 'old', accountId: 'other', totalMinor: 0, status: 'pending', lines: [] };
      before.orders.push(old); prepared.orders.push(old);
    },
  };
  for (const [name, defect] of Object.entries(defects)) {
    const snapshots = states();
    defect(snapshots);
    assert(checkoutDifferences(snapshots.before, snapshots.prepared, snapshots.after, 1).length > 0, name);
  }
});

test('unreadable or inexact checkout data cannot become empty or rounded success', () => {
  assert.equal(checkoutMinor(19.99), 1999);
  assert.equal(checkoutMinor('0.29'), 29);
  for (const value of [null, '', NaN, Infinity, 1.001, Number.MAX_SAFE_INTEGER]) assert.throws(() => checkoutMinor(value));
  assert.equal(checkoutId('18446744073709551615'), '18446744073709551615');
  assert.throws(() => checkoutId(Number('18446744073709551615')));
  assert.throws(() => checkoutStateSchema.parse({ ...states().before, stock: [] }));
  assert.throws(() => checkoutStateSchema.parse({ ...states().before, accountId: null }));
  const { before, prepared, after } = states();
  before.stock.push({ warehouseId: 'large', quantity: Number.MAX_SAFE_INTEGER });
  assert.throws(() => checkoutDifferences(before, prepared, after, 1));
});

test('checkout reader rejects an unverified source mapping before a database read', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-checkout-schema-'));
  try {
    const file = 'server/src/schema.ts';
    const source = join(STACK_BENCH_ROOT, 'reference-apps/ecommerce/postgres');
    cpSync(join(source, 'server/src'), join(root, 'server/src'), { recursive: true });
    assert.match(verifyCheckoutSchema('postgres', root, [file])[file]!, /^[a-f0-9]{64}$/);
    writeFileSync(join(root, file), 'different schema');
    assert.throws(() => verifyCheckoutSchema('postgres', root, [file]), /no verified mapping/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('checkout actions retain a failed reconciliation and treat reader failures as unmeasured', async () => {
  for (const failedRead of [false, true]) {
    const snapshots = states();
    const queue = [snapshots.before, snapshots.prepared, snapshots.prepared];
    const capabilities = { 'database-read': {
      checkoutSnapshots: new Map(),
      getCheckoutState: () => {
        if (failedRead) throw new Error('database unavailable');
        return { state: queue.shift()!, schemaSha256: { schema: 'verified' } };
      },
    } };
    for (const as of ['before', 'prepared']) {
      const result = await executeAction(ACTION_REGISTRY, 'dbRecordCheckout', {
        do: 'dbRecordCheckout', account: 'a', item: 'i', as,
      }, { capabilities });
      assert.equal(result.status, failedRead ? 'harness_failure' : 'passed');
    }
    const result = await executeAction(ACTION_REGISTRY, 'dbExpectCheckout', {
      do: 'dbExpectCheckout', before: 'before', prepared: 'prepared', quantity: 1,
    }, { capabilities });
    assert.equal(result.status, failedRead ? 'inconclusive' : 'failed');
    if (!failedRead) assert(result.observation);
  }
});
