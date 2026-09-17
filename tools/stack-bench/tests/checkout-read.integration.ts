import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import ts from 'typescript';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { getPostgresCheckoutState } from '../src/stacks/backends/postgres-operations.js';
import { getMongoDbCheckoutState } from '../src/stacks/backends/mongodb-operations.js';
import { checkoutDifferences, orderCheckoutDifferences, orderCancellationDifferences } from '../src/stacks/checkout-state.js';

const enabled = process.env.STACK_BENCH_CHECKOUT_READ_DOCKER === '1';
const docker = (args: string[], input?: string): string => execFileSync('docker', args,
  { encoding: 'utf8', input, stdio: 'pipe', timeout: 30000, windowsHide: true }).trim();

for (const backend of ['postgres', 'mongodb'] as const) {
  test(`${backend} checkout reader reconciles real stored rows and rejects unreadable tables`, {
    skip: enabled ? false : 'Set STACK_BENCH_CHECKOUT_READ_DOCKER=1 for isolated database reader tests', timeout: 120000,
  }, async () => {
    const postgres = backend === 'postgres';
    const name = `stack-bench-checkout-read-${backend}-${randomUUID()}`;
    const id = docker(['run', '--pull=never', '--rm', '-d', '--name', name, '--network', 'none',
      '--cpus', '1', '--memory', '512m', ...(postgres ? ['-e', 'POSTGRES_HOST_AUTH_METHOD=trust',
        '-e', 'POSTGRES_USER=appuser', '-e', 'POSTGRES_DB=bench', 'postgres:16-alpine']
        : ['mongo:7', '--replSet', 'rs0', '--bind_ip_all'])]);
    const lease = { resources: { container: { name, id }, database: 'bench' } };
    const run = (script: string) => postgres
      ? docker(['exec', '-i', id, 'psql', '-U', 'appuser', '-d', 'bench', '-v', 'ON_ERROR_STOP=1', '-At'], script)
      : docker(['exec', id, 'mongosh', 'bench', '--quiet', '--eval', script]);
    try {
      for (let attempt = 0; ; attempt++) {
        try { run(postgres ? 'SELECT 1;' : 'db.runCommand({ping:1})'); break; }
        catch (error) { if (attempt === 30) throw error; await delay(250); }
      }
      if (!postgres) {
        run("rs.initiate({_id:'rs0',members:[{_id:0,host:'127.0.0.1:27017'}]})");
        for (let attempt = 0; ; attempt++) {
          if (run('print(db.hello().isWritablePrimary)') === 'true') break;
          if (attempt === 30) throw new Error('MongoDB test replica did not become writable');
          await delay(250);
        }
      }
      run(postgres ? `
        CREATE TABLE account(id integer, username text);
        CREATE TABLE item(id integer, name text, price numeric);
        CREATE TABLE warehouse(id integer);
        CREATE TABLE stock(item_id integer, warehouse_id integer, quantity integer);
        CREATE TABLE cart(id integer, account_id integer);
        CREATE TABLE cart_item(id integer, cart_id integer, item_id integer, quantity integer);
        CREATE TABLE cart_reservation_allocation(cart_item_id integer, warehouse_id integer, quantity integer);
        CREATE TABLE orders(id integer, account_id integer, total numeric, status text, payment_amount numeric, payment_status text, refund_total numeric DEFAULT 0);
        CREATE TABLE order_item(id integer DEFAULT 8, order_id integer, item_id integer, quantity integer, price numeric, warehouse_id integer);
        INSERT INTO account VALUES(1,'reader'); INSERT INTO item VALUES(2,'Keyboard',19.99);
        INSERT INTO warehouse VALUES(3); INSERT INTO stock VALUES(2,3,10); INSERT INTO cart VALUES(4,1);`
        : `for (const name of ['users','item','warehouse','stock','carts','orders','progressionpayments']) db.createCollection(name);
          db.users.insertOne({_id:'1',username:'reader'}); db.item.insertOne({_id:'2',name:'Keyboard',price:19.99});
          db.warehouse.insertOne({_id:'3'});
          db.stock.insertOne({item_id:'2',warehouse_id:'3',quantity:10}); db.carts.insertOne({userId:'1',items:[]});`);
      // Run the reference's actual view definitions against the native database.
      // Only the driver is replaced with the container's CLI; no view behavior is mocked.
      const source = readFileSync(join(STACK_BENCH_ROOT, 'reference-apps/ecommerce', backend, 'server/src/order-data.ts'), 'utf8');
      const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022,
        module: ts.ModuleKind.CommonJS } }).outputText;
      const initialize = new Function('exports', `${code}; return exports.initializeOrderData;`)({});
      const command = (value: unknown) => JSON.parse(run(`const r=db.runCommand(${JSON.stringify(value)}); if(!r.ok) throw Error(JSON.stringify(r)); print(JSON.stringify(r));`));
      const connection = postgres ? { query: async (sql: string) => run(sql) } : { db: {
        listCollections: () => ({ toArray: async () => command({ listCollections: 1, nameOnly: false }).cursor.firstBatch }),
        dropCollection: async (name: string) => command({ drop: name }),
        createCollection: async (name: string, options: object) => command({ create: name, ...options }),
      } };
      await initialize(connection);
      await initialize(connection); // Restart must keep the views readable without changing app data.
      if (!postgres) {
        run("db.order_cart.drop(); db.createView('order_cart','carts',[{$match:{impossible:true}}]);");
        await initialize(connection);
        run("db.order_account.drop(); db.createCollection('order_account'); db.order_account.insertOne({keep:true});");
        await assert.rejects(initialize(connection), /refusing to replace stored data/);
        assert.equal(run('print(db.order_account.countDocuments({keep:true}))'), '1');
        run('db.order_account.drop()');
        await initialize(connection);
      }
      const readOrders = () => (postgres ? getPostgresCheckoutState : getMongoDbCheckoutState)({
        account: 'reader', item: 'Keyboard', app: '/not-a-reference', lease, storage: { kind: 'order-data', cart: true, warehouses: true },
      }).state;
      assert.deepEqual(readOrders().orders, []);
      const read = () => (postgres ? getPostgresCheckoutState : getMongoDbCheckoutState)({
        account: 'reader', item: 'Keyboard', app: join(STACK_BENCH_ROOT, 'reference-apps/ecommerce', backend), lease,
      });
      const before = read();
      const orderBefore = readOrders();
      run(postgres ? `INSERT INTO cart_item VALUES(5,4,2,1); INSERT INTO cart_reservation_allocation VALUES(5,3,1);
        UPDATE stock SET quantity=9;` : `db.carts.updateOne({userId:'1'},{$set:{items:[{itemId:'2',quantity:1,reservedWarehouseIds:['3']}]}});
        db.stock.updateOne({item_id:'2'},{$set:{quantity:9}});`);
      const prepared = read();
      const orderPrepared = readOrders();
      assert.deepEqual(orderPrepared.reservations, [{ itemId: '2', warehouseId: '3', quantity: 1 }]);
      assert.deepEqual(readOrders().cart, [{ itemId: '2', quantity: 1 }]);
      run(postgres ? `INSERT INTO orders VALUES(6,1,19.99,'pending',19.99,'paid',0);
        INSERT INTO order_item VALUES(8,6,2,1,19.99,3); DELETE FROM cart_item; DELETE FROM cart_reservation_allocation;`
        : `db.orders.insertOne({_id:'6',userId:'1',total:19.99,refundTotal:0,status:'pending',items:[{itemId:'2',quantity:1,price:19.99,allocations:[{warehouseId:'3',quantity:1}]}]});
          db.progressionpayments.insertOne({_id:'7',orderId:'6',amount:19.99,status:'paid'});
          db.carts.updateOne({userId:'1'},{$set:{items:[]}});`);
      const after = read();
      assert.deepEqual(checkoutDifferences(before.state, prepared.state, after.state, 1), []);
      assert.equal(after.state.orders[0]!.totalMinor, 1999);
      const pending = readOrders();
      assert.deepEqual(orderCheckoutDifferences(orderBefore, orderPrepared, pending, 1), []);
      assert.deepEqual(pending.orders, after.state.orders.map(order => ({ ...order, refundedMinor: 0 })));
      run(postgres ? "UPDATE orders SET status='cancelled'; UPDATE stock SET quantity=10;"
        : "db.orders.updateOne({},{$set:{status:'cancelled'}}); db.stock.updateOne({},{$set:{quantity:10}});");
      assert.deepEqual(orderCancellationDifferences(pending, readOrders()), []);
      run(postgres ? "UPDATE orders SET status='pending'; UPDATE stock SET quantity=9;"
        : "db.orders.updateOne({},{$set:{status:'pending'}}); db.stock.updateOne({},{$set:{quantity:9}});");
      run(postgres ? 'UPDATE orders SET payment_amount=0;' : 'db.progressionpayments.updateOne({},{$set:{amount:0}});');
      assert(checkoutDifferences(before.state, prepared.state, read().state, 1).some(row => row.control.includes('payment')));
      run(postgres ? 'UPDATE stock SET warehouse_id=NULL;' : 'db.stock.updateOne({},{$unset:{warehouse_id:1}});');
      assert.throws(read, 'missing identifiers must not become string placeholders');
      run(postgres ? 'UPDATE stock SET warehouse_id=3;' : "db.stock.updateOne({},{$set:{warehouse_id:'3'}});");
      run(postgres ? 'DROP TABLE order_item CASCADE;' : 'db.orders.drop();');
      assert.throws(read);
    } finally { docker(['rm', '-f', id]); }
  });
}
