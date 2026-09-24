import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { ORDER_DATA_COLUMNS, readOrderDataSnapshot } from '../src/stacks/order-data.js';
import { orderCheckoutDifferences, orderPurchaseDifferences, orderCancellationDifferences, checkoutCrashDifferences } from '../src/stacks/checkout-state.js';
import { getPostgresCheckoutState } from '../src/stacks/backends/postgres-operations.js';
import { getMongoDbCheckoutState } from '../src/stacks/backends/mongodb-operations.js';
import { getSpacetimeCheckoutState } from '../src/stacks/backends/spacetime-operations.js';
import { createDatabaseReadCapability } from '../src/actions/runtime-action-executors.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { runInNewContext } from 'node:vm';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { loadTrack } from '../src/composition/tracks.js';
import { requireRecipeRelease } from '../src/composition/recipe-release.js';
import { createBoundRecipeTaskRequest, resolveBoundRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { parseGradeArgs } from '../grader/grade.js';

const fullStorage = { kind: 'order-data' as const, cart: true, warehouses: true };

test('crash stock scope follows the validated delivered task, including retained upgrade contracts', async () => {
  const binding = requireRecipeRelease(loadTrack('ecommerce'), 3, 'ecommerce.progression-catalog');
  const path = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/progression-checkout-crash.json');
  const scenario = compileScenarioDefinition(JSON.parse(readFileSync(path, 'utf8')), { source: path });
  const step = scenario.features[0]!.setup.find(step => step.do === 'dbRecordCheckout')!;
  assert.deepEqual(step.storage, { kind: 'order-data', cart: true, warehouses: 'if-requested' });
  const warehouseContract = 'ecommerce.feature.warehouse-admin.hooks';
  const checkKeys = ['ecommerce.spec.state-durability.checkout-crash-integrity.910a'];
  for (const mode of ['checkout', 'warehouses', 'retained', 'missing-task'] as const) {
    const request = createBoundRecipeTaskRequest(binding, {
      featureIds: ['ecommerce.feature.checkout', ...(mode === 'warehouses' ? ['ecommerce.feature.warehouse-admin'] : [])],
      taskMode: mode === 'retained' ? 'upgrade' : 'fresh',
      priorContractIds: mode === 'retained' ? [warehouseContract] : [],
      expectedSpecifications: ['ecommerce.spec.state-durability'], checkKeys,
    }).request;
    const parsed = parseGradeArgs(['node', 'grade', '--url', 'http://localhost:1', '--spec', path,
      '--recipe-task-json', JSON.stringify(request)]);
    const task = resolveBoundRecipeTaskRequest(binding, parsed.recipeTask!).task;
    const warehouses = mode === 'warehouses' || mode === 'retained';
    assert.equal(task.contractIds.includes(warehouseContract), warehouses);
    let reads = 0;
    const capability = createDatabaseReadCapability({ backend: 'spacetime', app: '/app',
      expand: value => value.startsWith('{user:') ? 'buyer' : value,
      contractIds: mode === 'missing-task' ? undefined : task.contractIds,
      spacetime: { buildContainer: { id: 'owned', name: 'owned' }, mod: 'app',
        uri: 'http://localhost:3000', containerUri: 'http://localhost:3000' },
      exec: (_command, args) => {
        reads++;
        if (args[0] === 'inspect') return 'owned';
        const tables = args.filter(arg => arg.startsWith('SELECT * FROM ')).map(arg => arg.slice('SELECT * FROM '.length));
        assert.equal(tables.length, warehouses ? 9 : 5);
        assert.equal(tables.includes('stock'), warehouses);
        const raw = data();
        return JSON.stringify(Object.fromEntries(tables.map(table => [table, { inserts: raw[table as keyof typeof raw], deletes: [] }])));
      } });
    const result = await executeAction(ACTION_REGISTRY, step.do, step, { capabilities: { 'database-read': capability } });
    if (mode === 'missing-task') {
      assert.equal(result.status, 'harness_failure');
      assert.equal(reads, 0);
      assert.equal(capability.checkoutSnapshots.size, 0);
    } else {
      assert.equal(result.status, 'passed', mode);
      const snapshot = [...capability.checkoutSnapshots.values()][0]!;
      assert.deepEqual(snapshot.storage, { kind: 'order-data', cart: true, warehouses });
      assert.deepEqual((await capability.getCheckoutState(snapshot)).state, snapshot.state);
    }
  }
});

function data(): Record<keyof typeof ORDER_DATA_COLUMNS, Record<string, unknown>[]> {
  return {
    order_account: [{ id: '1', username: 'buyer' }], item: [{ id: '2', name: 'Keyboard', price: '19.99' }],
    warehouse: [{ id: '3' }],
    stock: [{ item_id: '2', warehouse_id: '3', quantity: 10 }],
    order_cart: [], order_reservation: [], order_header: [], order_line: [], order_allocation: [],
  };
}

test('multi-item checkout reconciles every cart line, stored price and selected warehouse effect, including refused writes', async () => {
  for (const lazy of [false, true]) for (const warehouses of [false, true]) for (const reserved of warehouses ? [false, true] : [false]) for (const mode of ['accepted', 'refused', 'missing-line', 'wrong-price', 'wrong-total', 'swapped-quantity',
    'extra-line', 'retained-cart', 'duplicate-order', 'refused-cart-change', 'committed-refusal', 'reject-all',
    ...(warehouses ? ['missing-stock-effect', 'swapped-stock-effects', 'unrelated-stock-change', 'missing-stock-row', 'duplicate-stock-row', 'wrong-allocation'] : [])]) {
    const raw = data();
    raw.item.push({ id: '4', name: 'Lamp', price: '3.25' }, { id: '5', name: 'Untouched', price: '2.00' });
    raw.stock.push({ item_id: '4', warehouse_id: '3', quantity: 10 }, { item_id: '5', warehouse_id: '3', quantity: 10 });
    const storage = { kind: 'order-data' as const, cart: true, warehouses };
    const read = () => ({ ...readOrderDataSnapshot(raw, 'buyer', 'Keyboard', storage), account: 'buyer', item: 'Keyboard' });
    const accounts = raw.order_account;
    if (lazy) raw.order_account = [];
    const before = read();
    raw.order_account = accounts;
    raw.order_cart = [{ account_id: '1', item_id: '2', quantity: 1 }, { account_id: '1', item_id: '4', quantity: 2 }];
    if (reserved) {
      raw.stock[0]!.quantity = 9; raw.stock[1]!.quantity = 8;
      raw.order_reservation = [{ account_id: '1', item_id: '2', warehouse_id: '3', quantity: 1 },
        { account_id: '1', item_id: '4', warehouse_id: '3', quantity: 2 }];
    }
    const prepared = read();
    const refused = ['refused', 'refused-cart-change', 'committed-refusal'].includes(mode);
    if (!refused && mode !== 'reject-all' || mode === 'committed-refusal') {
      raw.order_cart = [];
      raw.order_reservation = [];
      raw.order_header = [{ id: 'new', account_id: '1', total: 26.49, refunded: 0, status: 'pending' }];
      raw.order_line = [{ id: 'a', order_id: 'new', item_id: '2', quantity: 1, unit_price: 19.99 },
        { id: 'b', order_id: 'new', item_id: '4', quantity: 2, unit_price: 3.25 }];
      raw.stock[0]!.quantity = 9; raw.stock[1]!.quantity = 8;
      raw.order_allocation = [{ order_line_id: 'a', warehouse_id: '3', quantity: 1 },
        { order_line_id: 'b', warehouse_id: '3', quantity: 2 }];
    }
    if (mode === 'missing-line') raw.order_line.pop();
    if (mode === 'wrong-price') raw.order_line[1]!.unit_price = 0.01;
    if (mode === 'wrong-total') raw.order_header[0]!.total = 0.01;
    if (mode === 'swapped-quantity') { raw.order_line[0]!.quantity = 2; raw.order_line[1]!.quantity = 1; }
    if (mode === 'extra-line') raw.order_line.push({ ...raw.order_line[0]!, id: 'extra' });
    if (mode === 'retained-cart') raw.order_cart = [{ account_id: '1', item_id: '4', quantity: 2 }];
    if (mode === 'duplicate-order') raw.order_header.push({ ...raw.order_header[0]!, id: 'extra' });
    if (mode === 'refused-cart-change') raw.order_cart.pop();
    if (mode === 'missing-stock-effect') raw.stock[1]!.quantity = 10;
    if (mode === 'swapped-stock-effects') { raw.stock[0]!.quantity = 8; raw.stock[1]!.quantity = 9; }
    if (mode === 'unrelated-stock-change') raw.stock[2]!.quantity = 9;
    if (mode === 'missing-stock-row') raw.stock.pop();
    if (mode === 'duplicate-stock-row') raw.stock.push({ ...raw.stock[1]! });
    if (mode === 'wrong-allocation') raw.order_allocation[1]!.quantity = 1;
    const result = await executeAction(ACTION_REGISTRY, 'dbExpectCheckout', {
      do: 'dbExpectCheckout', before: 'before', prepared: 'prepared',
      quantity: [{ item: 'Keyboard', quantity: 1 }, { item: 'Lamp', quantity: 2 }],
      ...(mode !== 'reject-all' ? { actor: 'buyer' } : {}),
    }, { capabilities: {
      actors: { get: () => ({ actionCall: { action: 'checkout', accepted: !refused, status: refused ? 400 : 200 } }) },
      'database-read': { ...createDatabaseReadCapability({ expand: value => value,
        checkoutSnapshots: new Map([['before', before], ['prepared', prepared]]) }), getCheckoutState: read },
    } });
    assert.equal(result.status, ['accepted', 'refused'].includes(mode) ? 'passed' : 'failed', `${warehouses}:${reserved}:${mode}`);
    if (warehouses && ['accepted', 'refused', 'missing-stock-effect', 'swapped-stock-effects', 'missing-line'].includes(mode)) {
      const after = read().state;
      const quantities = [{ itemId: '2', priceMinor: 1999, quantity: 1 }, { itemId: '4', priceMinor: 325, quantity: 2 }];
      for (const confirmed of [false, true]) {
        const verdict = checkoutCrashDifferences(before.state, after, confirmed,
          (state, allow) => orderCheckoutDifferences(before.state, prepared.state, state, quantities, allow, true));
        assert.equal(verdict.atomicity.length === 0, ['accepted', 'refused'].includes(mode), `crash atomicity ${reserved}:${mode}`);
        assert.equal(verdict.durability.length > 0, confirmed && mode === 'refused', `crash durability ${reserved}:${mode}`);
      }
    }
  }
});

test('compiled duplicate checkout reconciles real order effects without requesting warehouse records', async () => {
  const binding = requireRecipeRelease(loadTrack('ecommerce'), 3, 'ecommerce.progression-catalog');
  const request = createBoundRecipeTaskRequest(binding, {
    featureIds: ['ecommerce.feature.checkout'], taskMode: 'fresh',
    expectedSpecifications: ['ecommerce.spec.concurrency-safety'],
    checkKeys: ['ecommerce.spec.concurrency-safety.duplicate-checkout.203b'],
  });
  assert(!request.selection.features?.includes('ecommerce.feature.warehouse-admin'));
  assert.match(request.task.contractText, /item\(id, name, price\)/);
  assert.match(request.task.contractText, /order_cart\(account_id, item_id, quantity\)/);
  assert(!request.task.requirementText.includes('Warehouse administration'));
  const path = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/01-duplicate-checkout.json');
  const scenario = compileScenarioDefinition(JSON.parse(readFileSync(path, 'utf8')), { source: path });
  const criterion = scenario.features[0]!.criteria.find(row => row.id === '203b')!;
  const steps = criterion.steps.filter(step => ['dbRecordCheckout', 'dbExpectCheckout'].includes(step.do)
    && [step.as, step.before].some(key => key === 'checkout-before' || key === 'checkout-prepared'));
  for (const step of criterion.steps.filter(step => step.do === 'dbRecordCheckout')) {
    assert.equal((step.storage as { warehouses: boolean }).warehouses, false);
  }
  const loss = criterion.steps.findIndex(step => step.do === 'loseCheckoutResponse');
  const retry = criterion.steps.findIndex(step => step.do === 'callAction');
  assert(loss > 0 && retry > loss);
  assert(criterion.steps.slice(loss + 1, retry).some(step => step.do === 'reload'));
  assert(criterion.steps.slice(retry + 1).some(step => step.do === 'expectNumber'
    && step.relativeTo === 'reply-stock-before' && step.plus === -1));
  assert.equal(steps.length, 3);
  const raceIndex = criterion.steps.findIndex(step => step.do === 'callConcurrently');
  assert(criterion.steps.indexOf(steps[1]!) < raceIndex && criterion.steps.indexOf(steps[2]!) > raceIndex);
  for (const defect of ['none', 'placed', 'completed', 'no-op', 'duplicate', 'wrong-owner', 'wrong-price', 'retained-cart', 'lost-prior']) {
    const initial = data();
    for (const key of ['warehouse', 'stock', 'order_reservation', 'order_allocation'] as const) delete (initial as Partial<typeof initial>)[key];
    initial.order_header = [{ id: 'old', account_id: '1', total: 0, refunded: 0, status: 'cancelled' }];
    const prepared = structuredClone(initial); prepared.order_cart = [{ account_id: '1', item_id: '2', quantity: 1 }];
    const after = structuredClone(initial);
    after.order_header.push({ id: 'new', account_id: '1', total: 19.99, refunded: 0, status: 'pending' });
    if (['placed', 'completed'].includes(defect)) after.order_header[1]!.status = defect;
    after.order_line.push({ id: 'line', order_id: 'new', item_id: '2', quantity: 1, unit_price: 19.99 });
    if (defect === 'no-op') Object.assign(after, prepared);
    if (defect === 'duplicate') after.order_header.push({ ...after.order_header[1]!, id: 'extra' });
    if (defect === 'wrong-owner') {
      after.order_account.push({ id: 'other', username: 'other' }); after.order_header[1]!.account_id = 'other';
    }
    if (defect === 'wrong-price') after.order_line[0]!.unit_price = 0;
    if (defect === 'retained-cart') after.order_cart = prepared.order_cart;
    if (defect === 'lost-prior') after.order_header.shift();
    const queue = [initial, prepared, after];
    const capability = createDatabaseReadCapability({ backend: 'spacetime', app: '/minimal-checkout', expand: value => value === '{user:twin}' ? 'buyer' : value,
      spacetime: { buildContainer: { id: 'owned', name: 'owned' }, mod: 'app', uri: 'http://localhost:3000', containerUri: 'http://localhost:3000' },
      exec: (_command, args) => {
        if (args[0] === 'inspect') return 'owned';
        const tables = args.filter(arg => arg.startsWith('SELECT * FROM ')).map(arg => arg.slice('SELECT * FROM '.length));
        assert.deepEqual(tables.sort(), ['item', 'order_account', 'order_cart', 'order_header', 'order_line']);
        const raw = queue.shift()!;
        return JSON.stringify(Object.fromEntries(tables.map(table => [table, { inserts: raw[table as keyof typeof raw], deletes: [] }])));
      } });
    for (const [index, step] of steps.entries()) {
      const result = await executeAction(ACTION_REGISTRY, step.do, step, { capabilities: { 'database-read': capability, actors: { get: () => undefined } } });
      assert.equal(result.status, index < 2 || ['none', 'placed', 'completed'].includes(defect) ? 'passed' : 'failed', defect);
    }
  }
});

test('order read scope requires selected data and never infers features from missing tables', () => {
  for (const cart of [false, true]) for (const warehouses of [false, true]) {
    const storage = { kind: 'order-data' as const, cart, warehouses }, raw = data();
    if (!cart) delete (raw as Partial<typeof raw>).order_cart;
    if (!cart || !warehouses) delete (raw as Partial<typeof raw>).order_reservation;
    if (!warehouses) for (const key of ['warehouse', 'stock', 'order_allocation'] as const) delete (raw as Partial<typeof raw>)[key];
    const snapshot = readOrderDataSnapshot(raw, 'buyer', 'Keyboard', storage);
    assert.deepEqual(snapshot.storage, storage);
    assert.equal(snapshot.state.stock.length, warehouses ? 1 : 0);
    if (!cart || !warehouses) assert.throws(() => readOrderDataSnapshot(raw, 'buyer', 'Keyboard', fullStorage));
    delete (raw as Partial<typeof raw>).order_line;
    assert.throws(() => readOrderDataSnapshot(raw, 'buyer', 'Keyboard', storage), /missing fields/);
  }
});

test('order data preserves empty state, orphan effects and exact identifiers without a reference source', () => {
  const raw = data();
  const read = () => readOrderDataSnapshot(raw, 'buyer', 'Keyboard', fullStorage);
  assert.equal(read().state.priceMinor, 1999);
  assert.deepEqual(read().state.orders, []);
  raw.order_account = [];
  assert.deepEqual(read().state.cart, [], 'a customer with no shopping records has an empty cart');
  assert.deepEqual(read().state.orders, []);
  raw.order_account = [{ id: '1', username: 'buyer' }, { id: '9', username: 'buyer' }];
  assert.throws(read, /ambiguous/, 'two native identities must not merge into one customer');
  raw.order_account.pop();
  raw.order_header.push({ id: '9007199254740993', account_id: '1', total: '19.99', refunded: 0, status: 'pending' });
  raw.order_line.push({ id: '5', order_id: '9007199254740993', item_id: '2', quantity: 1, unit_price: '19.99' });
  raw.order_allocation.push({ order_line_id: '5', warehouse_id: '3', quantity: 1 });
  assert.equal(read().state.orders[0]!.id, '9007199254740993');
  raw.order_account = [];
  assert.throws(read, /link is missing/, 'an orphan order is not an empty account');
  raw.order_account = [{ id: '1', username: 'buyer' }];
  raw.order_line[0]!.order_id = 'absent';
  assert.equal(read().state.orphanOrderLines, 1);
  raw.order_line = [];
  assert.equal(read().state.orphanAllocations, 1);
  raw.stock = [];
  assert.deepEqual(read().state.stock, [], 'lost stock must reach reconciliation');
  raw.order_header[0]!.total = '1.001';
  assert.throws(read, /invalid values/);
  raw.order_header[0]!.total = 19.99;
  raw.order_header.push({ ...raw.order_header[0]! });
  assert.throws(read, /duplicate ids/);
});

test('order data uses one SpacetimeDB subscription and preserves the explicit reader in later snapshots', async () => {
  const raw = data();
  raw.order_header.push({ id: '9007199254740993', account_id: '1', total: 19.99, refunded: 0, status: 'pending' });
  const exec = (_: string, args: readonly string[]) => {
    if (args[0] === 'inspect') return 'owned';
    assert(args.includes('subscribe'));
    assert.equal(args.filter(arg => arg.startsWith('SELECT *')).length, 9);
    return JSON.stringify(Object.fromEntries(Object.entries(raw).filter(([, rows]) => rows.length)
      .map(([table, inserts]) => [table, { inserts, deletes: [] }]))).replace('"9007199254740993"', '9007199254740993');
  };
  const capability = createDatabaseReadCapability({ backend: 'spacetime', app: '/not-a-reference', expand: value => value, exec,
    spacetime: { buildContainer: { id: 'owned', name: 'owned' }, mod: 'app', uri: 'http://localhost:3000', containerUri: 'http://localhost:3000' } });
  const result = await executeAction(ACTION_REGISTRY, 'dbRecordCheckout', {
    do: 'dbRecordCheckout', storage: { kind: 'order-data', cart: true, warehouses: true }, account: 'buyer', item: 'Keyboard', as: 'before',
  }, { capabilities: { 'database-read': capability } });
  assert.equal(result.status, 'passed');
  const snapshot = capability.checkoutSnapshots.get('before')!;
  assert.deepEqual(snapshot.storage, fullStorage);
  assert.equal(snapshot.state.orders[0]!.id, '9007199254740993');
  assert.deepEqual((await capability.getCheckoutState(snapshot)).state, snapshot.state);
  raw.item[0]!.price = 0.001;
  const invalid = await executeAction(ACTION_REGISTRY, 'dbRecordCheckout', {
    do: 'dbRecordCheckout', storage: { kind: 'order-data', cart: true, warehouses: true }, account: 'buyer', item: 'Keyboard', as: 'bad',
  }, { capabilities: { 'database-read': capability } });
  assert.equal(invalid.status, 'failed', 'bad declared data is an app failure');
  assert.throws(() => getSpacetimeCheckoutState({ account: 'buyer', item: 'Keyboard', app: '/not-a-reference' }),
    /ENOENT/, 'old diagnostic readers still require verified reference source');
});

test('order data preserves infrastructure failures instead of blaming the app', async () => {
  const capability = createDatabaseReadCapability({ backend: 'postgres', app: '/any-app', expand: value => value,
    databaseLease: { resources: { container: { id: 'owned', name: 'owned' }, database: 'app' } },
    exec: (_command, args) => { if (args[0] === 'inspect') return 'owned'; throw new Error('connection refused'); } });
  const result = await executeAction(ACTION_REGISTRY, 'dbRecordCheckout', {
    do: 'dbRecordCheckout', storage: { kind: 'order-data', cart: true, warehouses: true }, account: 'buyer', item: 'Keyboard', as: 'before',
  }, { capabilities: { 'database-read': capability } });
  assert.notEqual(result.status, 'passed');
  assert.notEqual(result.status, 'failed');
});

test('MongoDB order data reads a not-yet-created collection as empty and keeps real read failures', () => {
  const lease = { resources: { container: { id: 'owned', name: 'owned' }, database: 'bench' } };
  const mongo = (collections: Record<string, Record<string, unknown>[]>, failure?: string) =>
    (_command: string, args: readonly string[]) => {
      if (args[0] === 'inspect') return 'owned';
      let output = '';
      const collection = (name: string) => ({ find: () => ({ toArray: () => {
        if (name === failure) throw new Error(`MongoServerError: read of ${name} failed`);
        return collections[name]!;
      } }) });
      const session = { getDatabase: () => ({ getCollection: collection }), startTransaction() {}, commitTransaction() {},
        endSession() {} };
      runInNewContext(args.at(-1)!, { db: { getName: () => 'bench', getCollectionNames: () => Object.keys(collections),
        getMongo: () => ({ startSession: () => session }) }, print: (value: string) => { output += value; } });
      return output;
    };
  const read = (collections: Record<string, Record<string, unknown>[]>, failure?: string) => getMongoDbCheckoutState({
    account: 'buyer', item: 'Keyboard', app: '/not-a-reference', lease, storage: fullStorage, exec: mongo(collections, failure) });
  const catalog = { item: data().item, warehouse: data().warehouse, stock: data().stock };
  const empty = read(catalog).state;
  assert.deepEqual([empty.cart, empty.reservations, empty.orders], [[], [], []]);
  assert.equal(empty.stock[0]!.quantity, 10);
  assert.deepEqual(read({ item: data().item }).state.stock, [], 'no stock collection yet is no stock rows');
  assert.throws(() => read(catalog, 'stock'), (error: unknown) => error instanceof Error
    && /read of stock failed/.test(error.message) && !('orderDataInterface' in error));
});

test('order-only purchase and cancellation reject no-op, wrong allocation and wrong refund effects', () => {
  const storage = { kind: 'order-data' as const, cart: false, warehouses: true };
  const raw = data();
  raw.item.push({ id: 'other', name: 'Untouched', price: 1 });
  raw.stock.push({ item_id: 'other', warehouse_id: '3', quantity: 7 });
  const before = readOrderDataSnapshot(raw, 'buyer', 'Keyboard', storage).state;
  raw.stock[0]!.quantity = 9;
  raw.order_header.push({ id: '4', account_id: '1', total: 19.99, refunded: 0, status: 'pending' });
  raw.order_line.push({ id: '5', order_id: '4', item_id: '2', quantity: 1, unit_price: 19.99 });
  raw.order_allocation.push({ order_line_id: '5', warehouse_id: '3', quantity: 1 });
  const after = readOrderDataSnapshot(raw, 'buyer', 'Keyboard', storage).state;
  assert.deepEqual(orderPurchaseDifferences(before, after, new Map([['buyer', 1]]), new Map()), []);
  raw.warehouse = [];
  assert.throws(() => readOrderDataSnapshot(raw, 'buyer', 'Keyboard', fullStorage), /warehouse link is missing/);
  raw.warehouse = [{ id: '3' }];
  assert(orderPurchaseDifferences(before, before, new Map([['buyer', 1]]), new Map()).length);
  raw.order_header[0]!.status = 'cancelled';
  raw.order_header[0]!.refunded = 19.99;
  raw.stock[0]!.quantity = 10;
  const cancelled = readOrderDataSnapshot(raw, 'buyer', 'Keyboard', storage).state;
  assert.deepEqual(orderCancellationDifferences(after, cancelled), []);
  const damaged = structuredClone(cancelled); damaged.stock[1]!.quantity++;
  assert(orderCancellationDifferences(after, damaged).length, 'cancellation must not restore another product');
  const wrongPurchase = structuredClone(after); wrongPurchase.stock[1]!.quantity--;
  assert(orderPurchaseDifferences(before, wrongPurchase, new Map([['buyer', 1]]), new Map()).length, 'purchase must preserve other products');
  for (const defect of ['missing-order', 'missing-lines', 'missing-allocation']) {
    const broken = structuredClone(after);
    if (defect === 'missing-order') broken.orders = [];
    if (defect === 'missing-lines') broken.orders[0]!.lines = [];
    if (defect === 'missing-allocation') broken.orders[0]!.lines[0]!.allocations = [];
    assert(orderCancellationDifferences(broken, cancelled).length, `${defect} is an app failure, not a harness exception`);
  }
  assert(orderCancellationDifferences(after, after).length);
  cancelled.orders[0]!.refundedMinor = 0;
  assert(orderCancellationDifferences(after, cancelled).some(row => row.control === 'cancellation orders'));
  cancelled.orders[0]!.refundedMinor = 1999;
  cancelled.stock[0]!.quantity = 11;
  assert(orderCancellationDifferences(after, cancelled).some(row => row.control === 'cancellation stock'));
});

const docker = (args: string[], input?: string) => execFileSync('docker', args,
  { encoding: 'utf8', input, stdio: 'pipe', timeout: 30000, windowsHide: true }).trim();
for (const backend of ['postgres', 'mongodb'] as const) {
  test(`${backend} native order interface reads live data, detects lost effects and rejects missing interfaces`, {
    skip: process.env.STACK_BENCH_CHECKOUT_READ_DOCKER !== '1', timeout: 120000,
  }, async () => {
    const pg = backend === 'postgres', name = `stack-bench-order-data-${backend}-${randomUUID()}`;
    const id = docker(['run', '--pull=never', '--rm', '-d', '--name', name, '--network', 'none',
      '--cpus', '1', '--memory', '512m', ...(pg ? ['-e', 'POSTGRES_HOST_AUTH_METHOD=trust',
        '-e', 'POSTGRES_USER=appuser', '-e', 'POSTGRES_DB=bench', 'postgres:16-alpine']
        : ['mongo:7', '--replSet', 'rs0', '--bind_ip_all'])]);
    const run = (script: string) => pg
      ? docker(['exec', '-i', id, 'psql', '-U', 'appuser', '-d', 'bench', '-v', 'ON_ERROR_STOP=1', '-At'], script)
      : docker(['exec', id, 'mongosh', 'bench', '--quiet', '--eval', script]);
    try {
      for (let attempt = 0; ; attempt++) {
        try { run(pg ? 'SELECT 1;' : 'db.runCommand({ping:1})'); break; }
        catch (error) { if (attempt === 30) throw error; await delay(250); }
      }
      if (!pg) {
        run("rs.initiate({_id:'rs0',members:[{_id:0,host:'127.0.0.1:27017'}]})");
        for (let attempt = 0; ; attempt++) {
          if (run('print(db.hello().isWritablePrimary)') === 'true') break;
          if (attempt === 30) throw new Error('test replica did not become writable');
          await delay(250);
        }
      }
      run(pg ? `
        CREATE TABLE order_account(id bigint, username text);
        CREATE TABLE item(id bigint, name text, price numeric);
        CREATE TABLE warehouse(id bigint);
        CREATE TABLE stock(item_id bigint, warehouse_id bigint, quantity integer);
        CREATE TABLE order_cart(account_id bigint, item_id bigint, quantity integer);
        CREATE TABLE order_reservation(account_id bigint, item_id bigint, warehouse_id bigint, quantity integer);
        CREATE TABLE order_header(id bigint, account_id bigint, total numeric, refunded numeric, status text);
        CREATE TABLE order_line(id bigint, order_id bigint, item_id bigint, quantity integer, unit_price numeric);
        CREATE TABLE order_allocation(order_line_id bigint, warehouse_id bigint, quantity integer);
        INSERT INTO order_account VALUES(1,'buyer'); INSERT INTO item VALUES(2,'Keyboard',19.99);
        INSERT INTO stock VALUES(2,3,10); INSERT INTO warehouse VALUES(3);`
        : `for (const [table, rows] of Object.entries(${JSON.stringify(data())})) {
            db.createCollection(table); if (rows.length) db.getCollection(table).insertMany(rows);
          }`);
      const read = (storage = fullStorage) => (pg ? getPostgresCheckoutState : getMongoDbCheckoutState)({ account: 'buyer', item: 'Keyboard',
        storage, app: '/not-a-reference', lease: { resources: { container: { id, name }, database: 'bench' } } });
      run(pg ? 'DELETE FROM order_account;' : 'db.order_account.deleteMany({})');
      const before = read().state;
      run(pg ? "INSERT INTO order_account VALUES(1,'buyer'); INSERT INTO order_cart VALUES(1,2,1);"
        : "db.order_account.insertOne({id:'1',username:'buyer'}); db.order_cart.insertOne({account_id:'1',item_id:'2',quantity:1})");
      const prepared = read().state;
      run(pg ? `INSERT INTO order_header VALUES(9007199254740993,1,19.99,0,'pending');
          INSERT INTO order_line VALUES(5,9007199254740993,2,1,19.99);
          INSERT INTO order_allocation VALUES(5,3,1); UPDATE stock SET quantity=9; DELETE FROM order_cart;`
        : `db.order_header.insertOne({id:Long('9007199254740993'),account_id:'1',total:Decimal128('19.99'),refunded:0,status:'pending'});
          db.order_line.insertOne({id:'5',order_id:Long('9007199254740993'),item_id:'2',quantity:1,unit_price:19.99});
          db.order_allocation.insertOne({order_line_id:'5',warehouse_id:'3',quantity:1});
          db.stock.updateOne({},{$set:{quantity:9}}); db.order_cart.deleteMany({});`);
      const after = read().state;
      assert.equal(after.orders[0]!.id, '9007199254740993');
      assert.deepEqual(orderCheckoutDifferences(before, prepared, after, 1), []);
      run(pg ? 'ALTER TABLE order_header RENAME TO stored_orders; CREATE VIEW order_header AS SELECT * FROM stored_orders;'
        : "db.order_header.renameCollection('stored_orders'); db.createView('order_header','stored_orders',[]);");
      assert.deepEqual(read().state, after, 'native views may expose the application records without duplicate storage');
      run(pg ? 'DELETE FROM order_line;' : 'db.order_line.deleteMany({})');
      assert.equal(read().state.orphanAllocations, 1);
      assert(orderCheckoutDifferences(before, prepared, read().state, 1).length > 0);
      run(pg ? 'DROP TABLE order_line;' : 'db.order_line.drop()');
      // MongoDB creates a collection on first insert, so an absent collection has no rows.
      if (pg) assert.throws(read, error => error instanceof Error && 'orderDataInterface' in error);
      else assert.equal(read().state.orphanAllocations, 1);
      run(pg ? `CREATE TABLE order_line(id bigint, order_id bigint, item_id bigint, quantity integer, unit_price numeric);
          DROP TABLE stock, warehouse, order_allocation, order_reservation, order_cart;`
        : `db.createCollection('order_line'); for (const table of ['stock','warehouse','order_allocation','order_reservation','order_cart']) db[table].drop();`);
      const minimal = { kind: 'order-data' as const, cart: false, warehouses: false };
      assert.deepEqual(read(minimal).state.stock, []);
      if (pg) {
        assert.throws(() => read({ ...minimal, cart: true }), error => error instanceof Error && 'orderDataInterface' in error);
        assert.throws(() => read({ ...minimal, warehouses: true }), error => error instanceof Error && 'orderDataInterface' in error);
      } else {
        assert.deepEqual(read({ ...minimal, cart: true }).state.cart, []);
        assert.deepEqual(read({ ...minimal, warehouses: true }).state.stock, []);
      }
      run(pg ? 'CREATE TABLE order_cart(account_id bigint, item_id bigint, quantity integer);' : "db.createCollection('order_cart')");
      assert.deepEqual(read({ ...minimal, cart: true }).state.cart, []);
    } finally { docker(['rm', '-f', id]); }
  });
}
