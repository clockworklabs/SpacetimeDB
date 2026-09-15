import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { checkoutDifferences, checkoutId, checkoutMinor, checkoutStateSchema, verifyCheckoutSchema, CheckoutDataError }
  from '../src/stacks/checkout-state.js';
import type { CheckoutState } from '../src/stacks/checkout-state.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { createDatabaseReadCapability } from '../src/actions/runtime-action-executors.js';
import { getSpacetimeCheckoutState } from '../src/stacks/backends/spacetime-operations.js';
import { getMongoDbCheckoutState } from '../src/stacks/backends/mongodb-operations.js';
import { preparedCheckouts } from '../src/evidence/grade-report.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

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
  assert.deepEqual(checkoutStateSchema.parse({ ...states().before, stock: [] }).stock, []);
  assert(checkoutDifferences({ ...states().before, stock: [] }, states().prepared, states().after, 1).length > 0);
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
    const checkoutSnapshots = new Map();
    const capabilities = () => ({ 'database-read': {
      ...createDatabaseReadCapability({ expand: value => value, checkoutSnapshots }),
      getCheckoutState: () => {
        if (failedRead) throw new Error('database unavailable');
        return { account: 'a', item: 'i', state: queue.shift()!, schemaSha256: { schema: 'verified' } };
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

test('migration compares the bound preparation, not new snapshots or newly expanded account names', async () => {
  const original = { account: 'customer-old', item: 'Keyboard', key: 'original',
    schemaSha256: { schema: 'a'.repeat(64) }, state: states().after };
  const step = { do: 'dbRecordCheckout', account: '{user:customer}', item: 'Keyboard', as: 'original' };
  const scenario = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 3,
    name: 'prepare', features: [{ id: 1, name: 'population', actors: ['a'], setup: [],
      criteria: [{ id: '1a', desc: 'record', category: 'production', points: 0, steps: [step] }] }] });
  const base = createDatabaseReadCapability({ expand: value => value });
  const action = await executeAction(ACTION_REGISTRY, step.do, step, { capabilities: {
    'database-read': { ...base, getCheckoutState: () => original },
  } });
  const passed = createCheckEvidence({ status: 'passed', code: 'measured', phase: 'assertion',
    startedAtMs: action.timing.startedAtMs, completedAtMs: action.timing.completedAtMs });
  const report = { selection: null, total: 0, max: 0, features: [{ id: 1, name: 'population',
    score: 0, max: 0, consoleErrors: [], setupEvidence: passed, criteria: [{ id: '1a', desc: 'record',
      points: 0, evidence: { ...passed, actions: [{ actor: null, evidence: action }] } }] }] };
  const prepared = preparedCheckouts(scenario, report);
  assert.equal(prepared[0]!.account, 'customer-old');
  for (const actions of [[], [{ actor: null, evidence: action }, { actor: null, evidence: action }],
    [{ actor: null, evidence: { ...action, observation: { ...original, key: 'wrong' } } }],
    [{ actor: null, evidence: { ...action, observation: { ...original, state: { ...original.state, stock: null } } } }]]) {
    const invalid = structuredClone(report);
    invalid.features[0]!.criteria[0]!.evidence.actions = actions;
    assert.throws(() => preparedCheckouts(scenario, invalid));
  }
  for (const changed of [false, true]) {
    const after = structuredClone(original);
    if (changed) after.state.orders[0]!.totalMinor++;
    const capability = { ...base, preparedCheckouts: prepared,
      getCheckoutState: (input: { account: string; item: string }, options?: { exact: boolean }) => {
        assert.equal(input.account, 'customer-old');
        assert.equal(options?.exact, true);
        return after;
      } };
    capability.checkoutSnapshots.set('original', after);
    const result = await executeAction(ACTION_REGISTRY, 'dbExpectMigrationCheckout',
      { do: 'dbExpectMigrationCheckout' }, { capabilities: { 'database-read': capability } });
    assert.equal(result.status, changed ? 'failed' : 'passed');
    assert.deepEqual(prepared[0], original);
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
  delete results.account;
  assert.throws(read,/account or item is missing/);
});

test('migration native reads distinguish missing business data from failed measurement', async () => {
  const original = { account: 'customer-old', item: 'Keyboard', key: 'original',
    schemaSha256: { schema: 'a'.repeat(64) }, state: states().after };
  const databaseLease = { resources: { container: { id: 'owned', name: 'test' }, database: 'test' } };
  for (const backend of ['postgres', 'mongodb']) {
    for (const [name, result, expected] of [
      ['preserved', JSON.stringify(original.state), 'passed'],
      ['deleted account', JSON.stringify({ ...original.state, accountId: null }), 'failed'],
      ['wrong stored type', JSON.stringify({ ...original.state, priceMinor: 'wrong' }), 'failed'],
      ['missing table', Object.assign(new Error('database query failed'), { stderr: backend === 'postgres'
        ? 'ERROR: relation "orders" does not exist' : 'Error: checkout-interface: checkout collection missing: orders' }), 'failed'],
      ['transport', new Error('connection timed out'), 'harness_failure'],
      ['malformed', '{', 'harness_failure'],
      ['incomplete protocol', '{}', 'harness_failure'],
    ] as const) {
      const capability = createDatabaseReadCapability({ backend, databaseLease,
        app: '/app-with-unrelated-source-changes', preparedCheckouts: [original],
        expand: () => { throw new Error('recorded selectors must not be expanded again'); },
        exec: (_command, args) => {
          if (args[0] === 'inspect') return 'owned';
          if (result instanceof Error) throw result;
          return result;
        } });
      const observed = await executeAction(ACTION_REGISTRY, 'dbExpectMigrationCheckout',
        { do: 'dbExpectMigrationCheckout' }, { capabilities: { 'database-read': capability } });
      assert.equal(observed.status, expected, `${backend}: ${name}`);
    }
  }
});

test('SpacetimeDB migration validates even empty business tables without requiring reference source', () => {
  const columns: Record<string, string[]> = {
    account: ['id'], item: ['id', 'price'], cart_item: ['account_id', 'item_id', 'quantity'],
    stock: ['item_id', 'warehouse_id', 'quantity'], reservation: ['account_id', 'item_id', 'warehouse_id', 'quantity'],
    customer_order: ['id', 'account_id', 'total', 'status'], order_item: ['id', 'order_id', 'item_id', 'quantity', 'unit_price'],
    payment_record: ['id', 'order_id', 'amount', 'status'], order_item_stock: ['order_item_id', 'warehouse_id', 'quantity'],
  };
  const types = Object.values(columns).map(names => ({ Product: { elements: names.map(name => ({
    name: { some: name }, algebraic_type: { [name === 'status' ? 'String' : name === 'quantity' ? 'U32'
      : ['price', 'unit_price', 'total', 'amount'].includes(name) ? 'F64' : 'U64']: [] },
  })) } }));
  const schema = { sections: [{ Typespace: { types } },
    { Tables: Object.keys(columns).map((name, index) => ({ source_name: name, product_type_ref: index })) }] };
  const results = { account: { inserts: [{ id: 1 }], deletes: [] }, item: { inserts: [{ id: 2, price: 3.17 }], deletes: [] } };
  let description: unknown = schema, subscription: unknown = results;
  const read = () => getSpacetimeCheckoutState({ account: 'reader', item: 'unstocked catalog item',
    app: '/app-with-alternate-address-storage', checkoutInterface: 'ecommerce-checkout-v1',
    spacetime: { buildContainer: { id: 'owned', name: 'test' }, mod: 'test', containerUri: 'http://127.0.0.1:3000' },
    exec: (_command, args) => args[0] === 'inspect' ? 'owned'
      : JSON.stringify(args.includes('describe') ? description : subscription) });
  assert.deepEqual(read().state.stock, []);
  const canonicalSchema = structuredClone(schema);
  const sourceTable = canonicalSchema.sections[1]!.Tables![2]!;
  sourceTable.source_name = 'cartItem';
  const explicitNames = { ExplicitNames: { entries: [
    { Table: { source_name: 'cartItem', canonical_name: 'cart_item' } },
  ] } };
  description = { sections: [...canonicalSchema.sections, explicitNames] };
  assert.deepEqual(read().state.stock, []);
  description = { sections: [...canonicalSchema.sections, { ExplicitNames: { entries: [
    { Table: { source_name: 'cartItem' } },
  ] } }] };
  assert.throws(read, error => error instanceof Error && !(error instanceof CheckoutDataError));
  description = schema;
  const emptyOrderType = types[5]!.Product.elements[0]!;
  emptyOrderType.name.some = 'renamed_id';
  assert.throws(read, CheckoutDataError);
  emptyOrderType.name.some = 'id';
  emptyOrderType.algebraic_type = { String: [] };
  assert.throws(read, CheckoutDataError);
  emptyOrderType.algebraic_type = { U64: [] };
  description = { sections: [{ Typespace: { types } }, { Tables: [null] }] };
  assert.throws(read, error => error instanceof Error && !(error instanceof CheckoutDataError));
  description = schema;
  subscription = { ...results, account: { inserts: [{}], deletes: [] } };
  assert.throws(read, error => error instanceof Error && !(error instanceof CheckoutDataError));
});

test('MongoDB fixed snapshot script reports malformed stored lines and allocations as data defects', () => {
  const databaseLease = { resources: { container: { id: 'owned', name: 'test' }, database: 'test' } };
  for (const change of [{ carts: [{ items: [null] }] }, { orders: [{ items: [null] }] },
    { orders: [{ items: [{ allocations: [null] }] }] }]) {
    const data: Record<string, unknown[]> = { users: [{ _id: 'u', username: 'reader' }],
      item: [{ _id: 'i', name: 'Keyboard', price: 3.17 }], carts: [], orders: [], progressionpayments: [], stock: [], ...change };
    const store = Object.fromEntries(Object.entries(data).map(([name, rows]) => [name, { find: () => ({ toArray: () => rows }) }]));
    const db = { getName: () => 'test', getCollectionNames: () => Object.keys(store), getMongo: () => ({
      startSession: () => ({ getDatabase: () => store, startTransaction() {}, commitTransaction() {}, endSession() {} }),
    }) };
    assert.throws(() => getMongoDbCheckoutState({ account: 'reader', item: 'Keyboard', app: '/different-source',
      lease: databaseLease, checkoutInterface: 'ecommerce-checkout-v1', exec: (_command, args) => {
        if (args[0] === 'inspect') return 'owned';
        let output = '';
        try { runInNewContext(args[args.indexOf('--eval') + 1]!, { db, print: (value: string) => { output = value; } }); }
        catch (error) { throw Object.assign(new Error('mongosh failed'), { stderr: String(error) }); }
        return output;
      } }), CheckoutDataError);
  }
});
