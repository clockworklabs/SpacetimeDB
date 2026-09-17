import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { ORDER_DATA_COLUMNS, readOrderDataSnapshot } from '../src/stacks/order-data.js';
import { orderCheckoutDifferences, orderPurchaseDifferences, orderCancellationDifferences } from '../src/stacks/checkout-state.js';
import { getPostgresCheckoutState } from '../src/stacks/backends/postgres-operations.js';
import { getMongoDbCheckoutState } from '../src/stacks/backends/mongodb-operations.js';
import { getSpacetimeCheckoutState } from '../src/stacks/backends/spacetime-operations.js';
import { createDatabaseReadCapability } from '../src/actions/runtime-action-executors.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';

function data(): Record<keyof typeof ORDER_DATA_COLUMNS, Record<string, unknown>[]> {
  return {
    order_account: [{ id: '1', username: 'buyer' }], item: [{ id: '2', name: 'Keyboard', price: '19.99' }],
    warehouse: [{ id: '3' }],
    stock: [{ item_id: '2', warehouse_id: '3', quantity: 10 }],
    order_cart: [], order_header: [], order_line: [], order_allocation: [],
  };
}

test('order data preserves empty state, orphan effects and exact identifiers without a reference source', () => {
  const raw = data();
  const read = () => readOrderDataSnapshot(raw, 'buyer', 'Keyboard');
  assert.equal(read().state.priceMinor, 1999);
  assert.deepEqual(read().state.orders, []);
  raw.order_header.push({ id: '9007199254740993', account_id: '1', total: '19.99', refunded: 0, status: 'pending' });
  raw.order_line.push({ id: '5', order_id: '9007199254740993', item_id: '2', quantity: 1, unit_price: '19.99' });
  raw.order_allocation.push({ order_line_id: '5', warehouse_id: '3', quantity: 1 });
  assert.equal(read().state.orders[0]!.id, '9007199254740993');
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
    assert.equal(args.filter(arg => arg.startsWith('SELECT *')).length, 8);
    return JSON.stringify(Object.fromEntries(Object.entries(raw).filter(([, rows]) => rows.length)
      .map(([table, inserts]) => [table, { inserts, deletes: [] }]))).replace('"9007199254740993"', '9007199254740993');
  };
  const capability = createDatabaseReadCapability({ backend: 'spacetime', app: '/not-a-reference', expand: value => value, exec,
    spacetime: { buildContainer: { id: 'owned', name: 'owned' }, mod: 'app', uri: 'http://localhost:3000', containerUri: 'http://localhost:3000' } });
  const result = await executeAction(ACTION_REGISTRY, 'dbRecordCheckout', {
    do: 'dbRecordCheckout', storage: 'order-data', account: 'buyer', item: 'Keyboard', as: 'before',
  }, { capabilities: { 'database-read': capability } });
  assert.equal(result.status, 'passed');
  const snapshot = capability.checkoutSnapshots.get('before')!;
  assert.equal(snapshot.storage, 'order-data');
  assert.equal(snapshot.state.orders[0]!.id, '9007199254740993');
  assert.deepEqual(capability.getCheckoutState(snapshot).state, snapshot.state);
  raw.item[0]!.price = 0.001;
  const invalid = await executeAction(ACTION_REGISTRY, 'dbRecordCheckout', {
    do: 'dbRecordCheckout', storage: 'order-data', account: 'buyer', item: 'Keyboard', as: 'bad',
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
    do: 'dbRecordCheckout', storage: 'order-data', account: 'buyer', item: 'Keyboard', as: 'before',
  }, { capabilities: { 'database-read': capability } });
  assert.notEqual(result.status, 'passed');
  assert.notEqual(result.status, 'failed');
});

test('order-only purchase and cancellation reject no-op, wrong allocation and wrong refund effects', () => {
  const raw = data(), before = readOrderDataSnapshot(raw, 'buyer', 'Keyboard').state;
  raw.stock[0]!.quantity = 9;
  raw.order_header.push({ id: '4', account_id: '1', total: 19.99, refunded: 0, status: 'pending' });
  raw.order_line.push({ id: '5', order_id: '4', item_id: '2', quantity: 1, unit_price: 19.99 });
  raw.order_allocation.push({ order_line_id: '5', warehouse_id: '3', quantity: 1 });
  const after = readOrderDataSnapshot(raw, 'buyer', 'Keyboard').state;
  assert.deepEqual(orderPurchaseDifferences(before, after, new Map([['1', 1]]), new Map()), []);
  raw.warehouse = [];
  assert.throws(() => readOrderDataSnapshot(raw, 'buyer', 'Keyboard'), /warehouse link is missing/);
  raw.warehouse = [{ id: '3' }];
  assert(orderPurchaseDifferences(before, before, new Map([['1', 1]]), new Map()).length);
  raw.order_header[0]!.status = 'cancelled';
  raw.order_header[0]!.refunded = 19.99;
  raw.stock[0]!.quantity = 10;
  const cancelled = readOrderDataSnapshot(raw, 'buyer', 'Keyboard').state;
  assert.deepEqual(orderCancellationDifferences(after, cancelled), []);
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
        CREATE TABLE order_header(id bigint, account_id bigint, total numeric, refunded numeric, status text);
        CREATE TABLE order_line(id bigint, order_id bigint, item_id bigint, quantity integer, unit_price numeric);
        CREATE TABLE order_allocation(order_line_id bigint, warehouse_id bigint, quantity integer);
        INSERT INTO order_account VALUES(1,'buyer'); INSERT INTO item VALUES(2,'Keyboard',19.99);
        INSERT INTO stock VALUES(2,3,10); INSERT INTO warehouse VALUES(3);`
        : `for (const [table, rows] of Object.entries(${JSON.stringify(data())})) {
            db.createCollection(table); if (rows.length) db.getCollection(table).insertMany(rows);
          }`);
      const read = () => (pg ? getPostgresCheckoutState : getMongoDbCheckoutState)({ account: 'buyer', item: 'Keyboard',
        storage: 'order-data', app: '/not-a-reference', lease: { resources: { container: { id, name }, database: 'bench' } } });
      const before = read().state;
      run(pg ? 'INSERT INTO order_cart VALUES(1,2,1);'
        : "db.order_cart.insertOne({account_id:'1',item_id:'2',quantity:1})");
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
      assert.throws(read, error => error instanceof Error && 'orderDataInterface' in error);
    } finally { docker(['rm', '-f', id]); }
  });
}
