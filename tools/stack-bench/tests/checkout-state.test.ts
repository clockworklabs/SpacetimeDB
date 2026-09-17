import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { checkoutDifferences, orderCheckoutDifferences, cancellationDifferences, purchaseDifferences, checkoutId, checkoutMinor, checkoutStateSchema, verifyCheckoutSchema }
  from '../src/stacks/checkout-state.js';
import type { CheckoutState } from '../src/stacks/checkout-state.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { createDatabaseReadCapability } from '../src/actions/runtime-action-executors.js';
import { getSpacetimeCheckoutState } from '../src/stacks/backends/spacetime-operations.js';

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

test('order-only checkout rejects partial, duplicate, lost and refunded effects without inventing payments', () => {
  const { before, prepared, after } = states();
  prepared.stock = structuredClone(before.stock);
  for (const state of [before, prepared, after]) {
    state.payments = []; state.reservations = []; state.orphanAllocations = 0;
    for (const order of state.orders) order.refundedMinor = 0;
    state.orders.push({ ...structuredClone(after.orders[0]!), id: 'prior', refundedMinor: 0 });
    state.refunds = [{ orderId: 'prior', accountId: 'a', amountMinor: 0 }]; state.orphanRefunds = 0;
  }
  assert.deepEqual(orderCheckoutDifferences(before, prepared, after, 1), []);
  assert.deepEqual(orderCheckoutDifferences(before, prepared, prepared, 1, true), []);
  assert(checkoutDifferences(before, prepared, after, 1).some(row => row.control === 'payments created by one checkout'));
  for (const mutate of [
    (state: CheckoutState) => { state.orders[0]!.lines = []; },
    (state: CheckoutState) => { state.orders.push({ ...structuredClone(state.orders[0]!), id: 'duplicate' }); },
    (state: CheckoutState) => { state.orders = state.orders.filter(order => order.id !== 'prior'); },
    (state: CheckoutState) => { state.orders.find(order => order.id === 'prior')!.refundedMinor = 1999; },
    (state: CheckoutState) => { state.orders[0]!.refundedMinor = 1999; },
    (state: CheckoutState) => { state.stock[0]!.quantity++; },
    (state: CheckoutState) => { state.stock = []; },
    (state: CheckoutState) => { state.orphanAllocations = 1; },
    (state: CheckoutState) => { state.refunds![0]!.amountMinor = 1; },
    (state: CheckoutState) => { state.refunds!.push({ orderId: 'o', accountId: 'a', amountMinor: 1999 }); },
    (state: CheckoutState) => { state.orphanRefunds = 1; },
  ]) {
    const broken = structuredClone(after); mutate(broken);
    assert(orderCheckoutDifferences(before, prepared, broken, 1, true).length);
  }
  const missing = structuredClone(after); delete missing.orders[0]!.refundedMinor;
  assert.throws(() => orderCheckoutDifferences(before, prepared, missing, 1));
  for (const state of [before, prepared, after]) for (const order of state.orders) order.refundedMinor = null;
  assert.deepEqual(orderCheckoutDifferences(before, prepared, after, 1), []);
  const preparedRefund = structuredClone(prepared); preparedRefund.refunds = [];
  assert(orderCheckoutDifferences(before, preparedRefund, after, 1).some(row => row.control === 'refunds unchanged during cart preparation'));
});

test('unsettled server work blocks later checkout and stock comparisons until a new grade', () => {
  const checkoutActivity = { unsettled: false };
  const capability = createDatabaseReadCapability({ expand: value => value, skip: true, checkoutActivity });
  capability.markCheckoutUnsettled();
  const nextAction = createDatabaseReadCapability({ expand: value => value, skip: true, checkoutActivity });
  assert.throws(() => nextAction.getCheckoutState({ account: 'buyer', item: 'Keyboard' }), /transport evidence is incomplete/);
  assert.throws(() => nextAction.getStock({ item: 'Keyboard' }), /transport evidence is incomplete/);
  const fresh = createDatabaseReadCapability({ expand: value => value, skip: true });
  assert.throws(() => fresh.getCheckoutState({ account: 'buyer', item: 'Keyboard' }), /reads are disabled/);
});

test('order checkout accounts for optional stock holds without accepting unexplained loss or duplicate purchases', () => {
  const { before, prepared, after } = states();
  for (const state of [before, prepared, after]) {
    state.payments = []; state.orphanAllocations = 0;
    for (const order of state.orders) order.refundedMinor = 0;
  }
  assert.deepEqual(orderCheckoutDifferences(before, prepared, after, 1), []);
  assert.deepEqual(orderCheckoutDifferences(before, prepared, prepared, 1, true), []);
  assert(orderCheckoutDifferences(before, prepared, prepared, 1).length, 'confirmed checkout cannot remain just a stock hold');
  const unexplained = structuredClone(prepared); unexplained.reservations = [];
  assert(orderCheckoutDifferences(before, unexplained, after, 1).length, 'stock debit needs an actual hold');
  const overheld = structuredClone(prepared); overheld.reservations[0]!.quantity = 2; overheld.stock[0]!.quantity = 8;
  assert(orderCheckoutDifferences(before, overheld, after, 1).length, 'hold cannot exceed the cart');
  for (const change of [
    (state: CheckoutState) => { state.stock[0]!.quantity--; },
    (state: CheckoutState) => { state.stock[0]!.quantity++; },
    (state: CheckoutState) => { state.orders = []; },
    (state: CheckoutState) => { state.orders.push({ ...structuredClone(state.orders[0]!), id: 'extra' }); },
    (state: CheckoutState) => { state.reservations = structuredClone(prepared.reservations); },
  ]) {
    const broken = structuredClone(after); change(broken);
    assert(orderCheckoutDifferences(before, prepared, broken, 1, true).length);
  }
  // A cart may hold only part of its quantity; final allocations must still account for the whole order.
  prepared.cart[0]!.quantity = 2;
  after.stock[0]!.quantity = 8;
  after.orders[0]!.totalMinor *= 2;
  after.orders[0]!.lines[0]!.quantity = 2;
  after.orders[0]!.lines[0]!.allocations[0]!.quantity = 2;
  assert.deepEqual(orderCheckoutDifferences(before, prepared, after, 2), []);
});

test('checkout reconciliation accepts stock reservation and atomic checkout alternatives', () => {
  const { before, prepared, after } = states();
  assert.deepEqual(checkoutDifferences(before, prepared, after, 1), []);
  prepared.stock = structuredClone(before.stock);
  prepared.reservations = [];
  assert.deepEqual(checkoutDifferences(before, prepared, after, 1), []);
  for (const snapshot of [before, prepared, after]) assert.deepEqual(checkoutStateSchema.parse(snapshot), snapshot);
  const prior = { ...structuredClone(after.orders[0]!), id: 'prior',
    lines: [{ ...structuredClone(after.orders[0]!.lines[0]!), itemId: 'old-a' },
      { ...structuredClone(after.orders[0]!.lines[0]!), itemId: 'old-b' }] };
  before.orders.push(structuredClone(prior)); prepared.orders.push(structuredClone(prior));
  prior.lines.reverse(); after.orders.push(prior);
  assert.deepEqual(checkoutDifferences(before, prepared, after, 1), [], 'nested database row order is not corruption');
});

test('crash recovery permits complete or absent effects only without an acknowledgement', () => {
  const { before, prepared, after } = states();
  assert.deepEqual(checkoutDifferences(before, prepared, after, 1, true), []);
  assert.deepEqual(checkoutDifferences(before, prepared, prepared, 1, true), []);
  assert(checkoutDifferences(before, prepared, prepared, 1).length, 'acknowledged checkout cannot disappear');
  for (const mutate of [
    (state: CheckoutState) => { state.payments = []; },
    (state: CheckoutState) => { state.cart = structuredClone(prepared.cart); },
    (state: CheckoutState) => { state.stock[0]!.quantity++; },
    (state: CheckoutState) => { state.orders.push({ ...structuredClone(state.orders[0]!), id: 'duplicate' }); },
  ]) {
    const partial = structuredClone(after); mutate(partial);
    assert(checkoutDifferences(before, prepared, partial, 1, true).length);
  }
  const invalid = structuredClone(prepared);
  invalid.cart[0]!.quantity++;
  assert(checkoutDifferences(before, invalid, invalid, 1, true).length, 'an invalid setup cannot pass as absent');
});

test('purchase histories reconcile owners, money, old rows and each warehouse even when total stock is unchanged', () => {
  const { before, after } = states();
  before.orders.push({ ...structuredClone(after.orders[0]!), id: 'old', accountId: 'old' });
  before.payments.push({ ...after.payments[0]!, id: 'old', orderId: 'old' });
  after.orders.push(...structuredClone(before.orders)); after.payments.push(...before.payments);
  for (const state of [before, after]) state.stock.push({ warehouseId: 'west', quantity: 5 });
  after.stock[1]!.quantity += 2;
  const accepted = new Map([['a', 1], ['b', 0]]), restocked = new Map([['west', 2]]);
  assert.deepEqual(purchaseDifferences(before, after, accepted, restocked), []);
  after.orders.reverse(); after.payments.reverse(); after.stock.reverse();
  assert.deepEqual(purchaseDifferences(before, after, accepted, restocked), []);
  const defects: Record<string, (state: CheckoutState) => void> = {
    duplicate: state => state.orders.push({ ...structuredClone(state.orders.find(row => row.id === 'o')!), id: 'extra' }),
    wrongBuyer: state => { state.orders.find(row => row.id === 'o')!.accountId = 'b'; },
    missingPayment: state => { state.payments = state.payments.filter(row => row.orderId !== 'o'); },
    wrongPrice: state => { state.orders.find(row => row.id === 'o')!.lines[0]!.priceMinor++; },
    wrongAmount: state => { state.payments.find(row => row.orderId === 'o')!.amountMinor++; },
    lostRestock: state => { state.stock.find(row => row.warehouseId === 'west')!.quantity -= 2; },
    lostPurchase: state => { state.stock.find(row => row.warehouseId === 'w')!.quantity++; },
    wrongWarehouse: state => { state.stock[0]!.quantity--; state.stock[1]!.quantity++; },
    corruptHistory: state => { state.orders.find(row => row.id === 'old')!.totalMinor = 0; },
    retainedCart: state => { state.cart = [{itemId:'i',quantity:1}]; },
    oversell: state => { state.stock[0]!.quantity = -1; },
    noOp: state => { Object.assign(state, structuredClone(before)); },
  };
  for (const [name, defect] of Object.entries(defects)) {
    const value = structuredClone(after); defect(value);
    assert(purchaseDifferences(before, value, accepted, restocked).length, name);
  }
  assert.throws(() => purchaseDifferences(before, after, accepted, new Map([['missing',1]])), /invalid restock/);
});

test('purchase action retains failed business evidence and keeps unknown outcomes unmeasured', async () => {
  for (const mode of ['valid','no-op','reject-all','unknown','missing','reader-error','schema-change']) {
    const {before,after}=states();
    const snapshots=new Map([['before',{state:before,account:'a',item:'i',schemaSha256:{schema:'same'}}]]);
    const result=await executeAction(ACTION_REGISTRY,'dbExpectPurchases',{
      do:'dbExpectPurchases',before:{buyer:'before'},purchases:1,
    },{capabilities:{
      'database-read':{...createDatabaseReadCapability({expand:value=>value,checkoutSnapshots:snapshots}),getCheckoutState:()=>{
        if(mode==='reader-error')throw new Error('unavailable');
        return {state:['no-op','reject-all'].includes(mode)?before:after,schemaSha256:{schema:mode==='schema-change'?'changed':'same'}};
      }},
      'named-actions':{lastCalls:{get:()=>mode==='missing'?null:{action:'buy',fired:1,ms:1,outcomes:[{
        action:'buy',values:{itemId:'i'},name:'buyer',ok:!['reject-all','unknown'].includes(mode),status:mode==='unknown'?0:mode==='reject-all'?409:200,text:'',
      }]}}},
    }});
    assert.equal(result.status,mode==='valid'?'passed':['unknown','missing'].includes(mode)?'inconclusive':['reader-error','schema-change'].includes(mode)?'harness_failure':'failed',mode);
    if(['no-op','reject-all'].includes(mode))assert(result.observation);
  }
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
  assert.throws(() => checkoutStateSchema.parse({ ...states().before, stock: null }));
  assert.throws(() => checkoutStateSchema.parse({ ...states().before, accountId: null }));
  const { before, prepared, after } = states();
  before.stock.push({ warehouseId: 'large', quantity: Number.MAX_SAFE_INTEGER });
  assert.throws(() => checkoutDifferences(before, prepared, after, 1));
});

test('empty stock is measurable after checkout but cannot establish a setup snapshot', async () => {
  const { before, prepared, after } = states();
  after.stock = [];
  assert.deepEqual(checkoutStateSchema.parse(after), after);
  assert(checkoutDifferences(before, prepared, after, 1).some(row => row.control === 'stored stock consumed by one checkout'));
  before.stock = []; prepared.stock = []; prepared.reservations = [];
  assert(checkoutDifferences(before, prepared, prepared, 1, true).some(row => row.control === 'initial stock warehouses'));
  const checkoutSnapshots = new Map();
  const result = await executeAction(ACTION_REGISTRY, 'dbRecordCheckout', {
    do: 'dbRecordCheckout', account: 'buyer', item: 'item', as: 'before',
  }, { capabilities: { 'database-read': {
    ...createDatabaseReadCapability({ expand: value => value, checkoutSnapshots }),
    getCheckoutState: () => ({ state: before, schemaSha256: { schema: 'same' } }),
  } } });
  assert.equal(result.status, 'inconclusive');
  assert.equal(checkoutSnapshots.size, 0);
  // A mapped warehouse with zero remaining units is still a valid observation.
  const zero = states();
  zero.before.stock[0]!.quantity = 1;
  zero.prepared.stock[0]!.quantity = 0; zero.after.stock[0]!.quantity = 0;
  assert.deepEqual(checkoutDifferences(zero.before, zero.prepared, zero.after, 1), []);
});

test('cancellation restores each allocation once and preserves other orders and payment history', () => {
  const before = states().after;
  before.stock.push({ warehouseId: 'west', quantity: 6 });
  before.orders[0]!.lines[0]!.allocations.push({ warehouseId: 'west', quantity: 2 });
  before.orders[0]!.lines[0]!.quantity = 3;
  before.orders[0]!.totalMinor *= 3;
  before.payments[0]!.amountMinor *= 3;
  before.orders.push({ ...structuredClone(before.orders[0]!), id: 'other', accountId: 'other' });
  const after = structuredClone(before);
  after.orders[0]!.status = 'cancelled';
  after.stock[0]!.quantity++;
  after.stock[1]!.quantity += 2;
  assert.deepEqual(cancellationDifferences(before, after), []);
  after.stock.reverse();
  after.orders.reverse();
  after.orders.forEach(order => order.lines.forEach(line => line.allocations.reverse()));
  assert.deepEqual(cancellationDifferences(before, after), []);
  const defects: Record<string, (state: CheckoutState) => void> = {
    duplicateStock: state => { state.stock[0]!.quantity += 2; },
    missingStock: state => { state.stock = structuredClone(before.stock); },
    wrongWarehouse: state => { state.stock[0]!.quantity--; state.stock[1]!.quantity++; },
    missingHistory: state => { state.orders = []; },
    wrongOrder: state => { state.orders.find(row => row.id === 'other')!.status = 'cancelled'; },
    duplicatePayment: state => { state.payments.push({ ...state.payments[0]!, id: 'extra' }); },
    changedTotal: state => { state.orders.find(row => row.id === 'o')!.totalMinor = 0; },
    noOp: state => { state.orders.find(row => row.id === 'o')!.status = 'pending'; },
  };
  for (const [name, defect] of Object.entries(defects)) {
    const value = structuredClone(after); defect(value);
    assert(cancellationDifferences(before, value).length > 0, name);
  }
  assert(cancellationDifferences(before, before).length > 0, 'reject all');
  assert(cancellationDifferences(states().before, after).length, 'a missing purchased order is a failed application precondition');
});

test('cancellation action retains mismatches and cannot pass absent or unreadable evidence', async () => {
  for (const mode of ['valid', 'mismatch', 'missing-order', 'missing', 'reader-error', 'schema-change']) {
    const before = states().after;
    const after = structuredClone(before);
    after.orders[0]!.status = 'cancelled'; after.stock[0]!.quantity++;
    if (mode === 'missing-order') before.orders = [];
    const checkoutSnapshots = new Map(mode === 'missing' ? [] : [['before', {
      state: before, account: 'a', item: 'i', schemaSha256: { schema: 'verified' },
    }]]);
    const result = await executeAction(ACTION_REGISTRY, 'dbExpectCancellation', {
      do: 'dbExpectCancellation', before: 'before',
    }, { capabilities: { 'database-read': {
      ...createDatabaseReadCapability({ expand: value => value, checkoutSnapshots }),
      getCheckoutState: () => {
        if (mode === 'reader-error') throw new Error('database unavailable');
        return { state: mode === 'mismatch' ? before : after,
          schemaSha256: { schema: mode === 'schema-change' ? 'changed' : 'verified' } };
      },
    } } });
    assert.equal(result.status, mode === 'valid' ? 'passed' : ['mismatch', 'missing-order'].includes(mode) ? 'failed'
      : mode === 'missing' ? 'inconclusive' : 'harness_failure', mode);
    if (mode === 'mismatch') assert(result.observation);
  }
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
    const checkoutSnapshots = new Map();
    const capabilities = () => ({ 'database-read': {
      ...createDatabaseReadCapability({ expand: value => value, checkoutSnapshots }),
      getCheckoutState: () => {
        if (failedRead) throw new Error('database unavailable');
        return { state: queue.shift()!, schemaSha256: { schema: 'verified' } };
      },
    } });
    for (const as of ['before', 'prepared']) {
      const result = await executeAction(ACTION_REGISTRY, 'dbRecordCheckout', {
        do: 'dbRecordCheckout', account: 'a', item: 'i', as,
      }, { capabilities: capabilities() });
      assert.equal(result.status, failedRead ? 'harness_failure' : 'passed');
    }
    const result = await executeAction(ACTION_REGISTRY, 'dbExpectCheckout', {
      do: 'dbExpectCheckout', before: 'before', prepared: 'prepared', quantity: 1,
    }, { capabilities: capabilities() });
    assert.equal(result.status, failedRead ? 'inconclusive' : 'failed');
    if (!failedRead) assert(result.observation);
  }
});

test('SpacetimeDB checkout reads one bounded subscription snapshot and rejects malformed rows', () => {
  const results: Record<string, { inserts: Record<string, unknown>[]; deletes: unknown[] }> = {
    account:{inserts:[{id:1}],deletes:[]}, item:{inserts:[{id:2,price:19.99}],deletes:[]},
    stock:{inserts:[{item_id:2,warehouse_id:3,quantity:10}],deletes:[]},
    cart_item:{inserts:[{account_id:1,item_id:2,quantity:1}],deletes:[]},
  };
  const read = () => getSpacetimeCheckoutState({ account:'reader', item:'Keyboard',
    app:join(STACK_BENCH_ROOT,'reference-apps/ecommerce/spacetime'),
    spacetime:{buildContainer:{id:'owned',name:'test'},mod:'test',containerUri:'http://127.0.0.1:3000'},
    exec:(_command,args) => {
      if (args[0]==='inspect') return 'owned';
      assert(args.includes('subscribe'));
      assert.equal(args[args.indexOf('--num-updates')+1],'0');
      assert.equal(args[args.indexOf('--timeout')+1],'30');
      assert.equal(args.filter(value=>value.startsWith('SELECT *')).length,9);
      return JSON.stringify(results);
    },
  });
  assert.equal(read().state.priceMinor,1999);
  assert.equal(read().state.cart[0]!.quantity,1);
  results.cart_item!.inserts=[{accountId:1,itemId:2,quantity:1}];
  assert.throws(read,/invalid row shape/);
  delete results.cart_item;
  assert.deepEqual(read().state.cart,[]);
  delete results.stock;
  assert.deepEqual(read().state.stock,[], 'a successful empty subscription must reach reconciliation');
  delete results.account;
  assert.throws(read,/account or item is missing/);
});
