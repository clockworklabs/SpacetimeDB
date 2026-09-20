import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { orderOperationDifferences, type CheckoutState, type OrderOperation } from '../src/stacks/checkout-state.js';
import { executeAction } from '../src/actions/action-contract.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { createDatabaseReadCapability } from '../src/actions/runtime-action-executors.js';

const catalog = [{ itemId: 'i', priceMinor: 200 }, { itemId: 'j', priceMinor: 300 }];
test('live history schedules bind every operation to two fresh native snapshots without scored points', () => {
  const scenario = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/diagnostic-mixed-history.json'), 'utf8')));
  assert.equal(scenario.features.length, 3);
  for (const feature of scenario.features) {
    const steps = feature.criteria[0]!.steps;
    assert.equal(feature.criteria[0]!.points, 0);
    const comparisons = steps.filter(step => step.do === 'dbExpectOperation');
    assert.equal(comparisons.length, 30);
    assert.equal(new Set(comparisons.map(step => step.before)).size, 30);
    assert.deepEqual([...new Set(comparisons.map(step => step.operation))].sort(),
      ['buy', 'cancel', 'cart-add', 'cart-update', 'checkout', 'reconnect', 'restock', 'transfer']);
    for (const comparison of comparisons) {
      const at = steps.indexOf(comparison), reconnect = comparison.operation === 'reconnect';
      const offset = reconnect ? 4 : 3;
      assert.deepEqual(steps.slice(at - offset, at - offset + 2).map(step => [step.do, step.as]),
        [['dbRecordCheckout', comparison.before], ['dbRecordCheckout', comparison.otherBefore]]);
      const call = steps[at - (reconnect ? 2 : 1)]!;
      assert.equal(call.do, reconnect ? 'reload' : 'callConcurrently');
      if (!reconnect) { assert.equal(call.action, comparison.operation); assert.equal(call.requests, 1); }
    }
  }
});
function initial(): CheckoutState {
  return { accountId: 'a', itemId: 'i', priceMinor: 200, cart: [], reservations: [], orders: [], payments: [],
    orphanOrderLines: 0, orphanAllocations: 0,
    stock: ['i', 'j'].flatMap(itemId => ['E', 'W'].map(warehouseId => ({ itemId, warehouseId, quantity: 100 }))) };
}

test('checkout snapshot timing uses the request evidence clock across a wall-clock correction', async context => {
  context.mock.method(Date, 'now', () => 0);
  const snapshots = new Map();
  const result = await executeAction(ACTION_REGISTRY, 'dbRecordCheckout', {
    do: 'dbRecordCheckout', account: 'a', item: 'i', as: 'before',
  }, { capabilities: { 'database-read': {
    ...createDatabaseReadCapability({ expand: value => value, checkoutSnapshots: snapshots }),
    getCheckoutState: () => ({ state: initial(), schemaSha256: {} }),
  } } });
  assert.equal(result.status, 'passed');
  assert(snapshots.get('before').recordedAtMs >= result.timing.startedAtMs);
  assert(snapshots.get('before').recordedAtMs <= result.timing.completedAtMs);
});

test('three 30-operation histories preserve cumulative orders, ownership, stock and single cancellation refunds', () => {
  for (const seed of [17, 43, 91]) {
    let state = initial(), steps = 0;
    const history: OrderOperation[] = [];
    const advance = (operation: OrderOperation, effect: (state: CheckoutState) => void) => {
      const before = structuredClone(state), after = structuredClone(state); effect(after);
      history.push(operation);
      assert.deepEqual(orderOperationDifferences(before, after, operation, catalog), [], JSON.stringify({ seed, history }));
      if (JSON.stringify(before) !== JSON.stringify(after)) {
        assert(orderOperationDifferences(before, before, operation, catalog).length, 'ignored completed writes must fail');
      }
      const corrupt = structuredClone(after); corrupt.stock[0]!.quantity++;
      assert(orderOperationDifferences(before, corrupt, operation, catalog).length, 'unexplained stock must fail at this step');
      if (after.orders.length > before.orders.length) for (const field of ['owner', 'price', 'duplicate'] as const) {
        const broken = structuredClone(after), order = broken.orders.at(-1)!;
        if (field === 'owner') order.accountId = 'wrong';
        if (field === 'price') order.totalMinor++;
        if (field === 'duplicate') broken.orders.push({ ...order, id: 'duplicate' });
        assert(orderOperationDifferences(before, broken, operation, catalog).length, field);
      }
      state = after; steps++;
    };
    for (let round = 0; round < 3; round++) {
      state.accountId = round % 2 ? 'b' : 'a';
      const quantity = 2 + (seed + round) % 4, transfer = 1 + (seed * 3 + round) % 7;
      const owner = state.accountId, id = `${seed}-${round}`;
      const buy = (itemId: 'i' | 'j', suffix: string) => advance({ kind: 'buy', itemId }, next => {
        next.stock[itemId === 'i' ? 0 : 2]!.quantity--;
        next.orders.push({ id: id + suffix, accountId: owner, status: 'pending', refundedMinor: 0,
          totalMinor: itemId === 'i' ? 200 : 300,
          lines: [{ itemId, quantity: 1, priceMinor: itemId === 'i' ? 200 : 300,
            allocations: [{ warehouseId: 'E', quantity: 1 }] }] });
      });
      buy('i', '-buy');
      advance({ kind: 'cart', itemId: 'j', quantity }, next => {
        next.cart = [{ itemId: 'j', quantity }]; next.stock[2]!.quantity -= quantity;
        next.reservations = [{ itemId: 'j', warehouseId: 'E', quantity }];
      });
      advance({ kind: 'transfer', itemId: 'j', fromWarehouseId: 'W', toWarehouseId: 'E', quantity: transfer }, next => {
        next.stock[2]!.quantity += transfer; next.stock[3]!.quantity -= transfer;
      });
      advance({ kind: 'restock', itemId: 'i', warehouseId: 'W', quantity: 7 }, next => { next.stock[1]!.quantity += 7; });
      advance({ kind: 'cart', itemId: 'j', quantity: quantity + 1 }, next => {
        next.cart[0]!.quantity++; next.stock[2]!.quantity--; next.reservations[0]!.quantity++;
      });
      advance({ kind: 'checkout' }, next => {
        next.orders.push({ id, accountId: owner, status: 'pending', refundedMinor: 0, totalMinor: 300 * (quantity + 1),
          lines: [{ itemId: 'j', quantity: quantity + 1, priceMinor: 300, allocations: [{ warehouseId: 'E', quantity: quantity + 1 }] }] });
        next.cart = []; next.reservations = [];
      });
      advance({ kind: 'cancel', orderId: id }, next => {
        const order = next.orders.find(order => order.id === id)!;
        order.status = 'cancelled'; order.refundedMinor = 300 * (quantity + 1); next.stock[2]!.quantity += quantity + 1;
      });
      advance({ kind: 'cancel', orderId: id }, () => {});
      advance({ kind: 'reconnect' }, () => {});
      buy('j', '-last');
    }
    assert.equal(steps, 30); assert.equal(state.orders.length, 9);
    assert.equal(state.orders.filter(order => order.status === 'cancelled').length, 3);
    assert.equal(state.stock[0]!.quantity, 97); assert.equal(state.stock[1]!.quantity, 121);
    assert.equal(state.stock[2]!.quantity + state.stock[3]!.quantity, 197);
  }
});

test('history accepts unreserved carts and rejects invalid or missing observations', () => {
  const before = initial(), after = initial(); after.cart = [{ itemId: 'j', quantity: 3 }];
  assert.deepEqual(orderOperationDifferences(before, after, { kind: 'cart', itemId: 'j', quantity: 3 }, catalog), []);
  for (const change of [
    (state: CheckoutState) => { state.reservations = [{ itemId: 'j', warehouseId: 'unknown', quantity: 1 }]; },
    (state: CheckoutState) => { state.reservations = [{ itemId: 'j', warehouseId: 'E', quantity: 4 }]; state.stock[2]!.quantity -= 4; },
    (state: CheckoutState) => { state.cart.push({ itemId: 'j', quantity: 3 }); },
  ]) {
    const broken = structuredClone(after); change(broken);
    assert(orderOperationDifferences(before, broken, { kind: 'cart', itemId: 'j', quantity: 3 }, catalog).length);
  }
  assert.throws(() => orderOperationDifferences(before, undefined as unknown as CheckoutState, { kind: 'reconnect' }, catalog));
  assert.throws(() => orderOperationDifferences(before, after, { kind: 'cart', itemId: 'j', quantity: -1 }, catalog));
});

test('history action binds complete fresh requests and keeps both customer snapshots in failed evidence', async () => {
  for (const operation of [['buy'], 'unknown']) {
    const invalid = await executeAction(ACTION_REGISTRY, 'dbExpectOperation', {
      do: 'dbExpectOperation', before: 'before', otherBefore: 'other', actor: 'buyer', operation,
    }, { capabilities: {} });
    assert.equal(invalid.status, 'harness_failure'); assert.equal(invalid.code, 'invalid_input');
  }
  for (const mode of ['valid', 'reconnect', 'ignored', 'other-cart', 'other-hold', 'divergent-reads', 'catalog-change', 'incomplete', 'stale', 'refused', 'wrong-action', 'missing', 'reader-error']) {
    const before = initial(), other = initial(); other.accountId = 'b';
    const after = initial(), otherAfter = structuredClone(other);
    if (!['ignored', 'refused', 'reconnect'].includes(mode)) after.cart = [{ itemId: 'j', quantity: 1 }];
    if (mode === 'other-cart') otherAfter.cart = [{ itemId: 'j', quantity: 1 }];
    if (mode === 'other-hold') otherAfter.reservations = [{ itemId: 'j', warehouseId: 'E', quantity: 1 }];
    if (mode === 'divergent-reads') otherAfter.stock[0]!.quantity++;
    const snapshot = (state: CheckoutState) => ({ state, scope: 'orders' as const,
      storage: { kind: 'order-data' as const, cart: true, warehouses: true }, recordedAtMs: 10,
      account: state.accountId, item: 'i', schemaSha256: { contract: 'fixed' },
      catalog: catalog.map(row => ({ ...row, name: row.itemId })) });
    const snapshots = new Map([['before', snapshot(before)], ['other', snapshot(other)]]);
    if (mode === 'missing') snapshots.delete('before');
    const result = await executeAction(ACTION_REGISTRY, 'dbExpectOperation', {
      do: 'dbExpectOperation', before: 'before', otherBefore: 'other', actor: 'buyer', operation: mode === 'reconnect' ? 'reconnect' : 'cart-add',
    }, { capabilities: {
      'database-read': {
        ...createDatabaseReadCapability({ expand: value => value, checkoutSnapshots: snapshots }),
        getCheckoutState: (input: { account: string }) => {
          if (mode === 'reader-error') throw new Error('observer unavailable');
          const value = snapshot(input.account === 'a' ? after : otherAfter);
          if (mode === 'catalog-change') value.catalog[0]!.priceMinor++;
          return value;
        },
      },
      'named-actions': { lastCalls: { get: () => ({ fired: 1, action: 'cart-add', ms: 1, outcomes: [{
        name: 'buyer', action: mode === 'wrong-action' ? 'buy' : 'cart-add', values: { itemId: 'j' },
        ok: mode !== 'refused', status: mode === 'refused' ? 409 : 200, text: '{}', complete: mode !== 'incomplete',
        startedAtMs: mode === 'stale' ? 5 : 11, completedAtMs: 12,
      }] }) } },
    } });
    assert.equal(result.status, ['valid', 'reconnect'].includes(mode) ? 'passed' : ['incomplete', 'stale', 'missing', 'divergent-reads'].includes(mode) ? 'inconclusive'
      : ['wrong-action', 'reader-error'].includes(mode) ? 'harness_failure' : 'failed', mode);
    if (['ignored', 'other-cart', 'other-hold', 'refused'].includes(mode)) {
      assert(result.observation && typeof result.observation === 'object'); assert('otherAfter' in result.observation);
    }
  }
});
