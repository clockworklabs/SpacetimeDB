import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { getPostgresStock, setPostgresStock } from '../src/stacks/backends/postgres-operations.js';
import { getMongoDbStock } from '../src/stacks/backends/mongodb-operations.js';
import { writePostgresStock } from '../src/stacks/postgres-sql.js';

const enabled = process.env.STACK_BENCH_STOCK_READ_DOCKER === '1';
const docker = (args: string[], input?: string): string => execFileSync('docker', args,
  { encoding: 'utf8', input, stdio: 'pipe', timeout: 30_000, windowsHide: true }).trim();

for (const backend of ['postgres', 'mongodb'] as const) {
  test(`trusted ${backend} stock reads use real database results`, {
    skip: enabled ? false : 'Set STACK_BENCH_STOCK_READ_DOCKER=1 for two isolated database checks',
    timeout: 90_000,
  }, async () => {
    const name = `stack-bench-stock-read-${backend}-${randomUUID()}`;
    const postgres = backend === 'postgres';
    const id = docker(['run', '--pull=never', '--rm', '-d', '--name', name,
      '--network', 'none', '--cpus', '1', '--memory', '384m',
      ...(postgres ? ['-e', 'POSTGRES_HOST_AUTH_METHOD=trust', '-e', 'POSTGRES_USER=appuser',
        '-e', 'POSTGRES_DB=bench', 'postgres:16-alpine'] : ['mongo:7'])]);
    const lease = { resources: { container: { name, id }, database: 'bench' } };
    const run = (source: string): string => postgres
      ? docker(['exec', '-i', id, 'psql', '-U', 'appuser', '-d', 'bench', '-v', 'ON_ERROR_STOP=1', '-At'], source)
      : docker(['exec', id, 'mongosh', 'bench', '--quiet', '--eval', source]);
    try {
      for (let attempt = 0; ; attempt++) {
        try { run(postgres ? 'SELECT 1;' : 'db.runCommand({ping:1})'); break; }
        catch (error) { if (attempt === 30) throw error; await delay(250); }
      }
      if (postgres) {
        run(`CREATE TABLE item (id integer PRIMARY KEY, name text);
CREATE TABLE warehouse (id integer PRIMARY KEY, name text);
CREATE TABLE stock (item_id integer REFERENCES item, warehouse_id integer REFERENCES warehouse, quantity integer);
CREATE TABLE order_line (item_id integer REFERENCES item, warehouse_id integer REFERENCES warehouse, quantity integer);
INSERT INTO item VALUES (0, 'Kid''s Keyboard');
INSERT INTO warehouse VALUES (1, 'East'), (2, 'West');
INSERT INTO stock VALUES (0, 1, 5), (0, 2, -1);
INSERT INTO order_line VALUES (0, 1, 17);`);
        assert.equal(getPostgresStock({ item: "Kid's Keyboard", lease }).quantity, 4);
        assert.equal(getPostgresStock({ item: "Kid's Keyboard", warehouse: 'West', lease }).quantity, -1);
        setPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', quantity: 0, lease });
        assert.equal(getPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', lease }).quantity, 0);
        assert.equal(run('SELECT quantity FROM order_line;'), '17', 'stock setup must not alter order history');
        assert.throws(() => getPostgresStock({ item: 'Absent', lease }), /no stock data/);
        assert.throws(() => setPostgresStock({ item: 'Absent', warehouse: 'East', quantity: 9, lease }), /required item/);
        run("INSERT INTO item VALUES (1, 'Kid''s Keyboard'); INSERT INTO stock VALUES (1, 1, 4);");
        assert.throws(() => getPostgresStock({ item: "Kid's Keyboard", lease }), /ambiguous/);
        assert.throws(() => setPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', quantity: 9, lease }), /ambiguous/);
        assert.equal(run('SELECT string_agg(quantity::text, \',\' ORDER BY item_id) FROM stock WHERE warehouse_id = 1;'), '0,4',
          'an ambiguous write must change no row');
        run('DELETE FROM stock WHERE item_id = 1;');
        run("DELETE FROM item WHERE id = 1; INSERT INTO warehouse VALUES (3, 'East');");
        assert.throws(() => getPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', lease }), /ambiguous/);
        run('DELETE FROM warehouse WHERE id = 3;');
        run('INSERT INTO stock VALUES (0, 1, 2);');
        assert.throws(() => getPostgresStock({ item: "Kid's Keyboard", lease }), /ambiguous/);
        // The observable interface requires unambiguous links and committed
        // writes, not a particular schema constraint or physical table layout.
        // Failure cases: duplicate names/ids/holdings, dangling links, invalid
        // quantities, read-only views, and acknowledged writes with no effect.
        run('DELETE FROM stock WHERE quantity = 2; ALTER TABLE stock DROP CONSTRAINT stock_warehouse_id_fkey;');
        assert.equal(getPostgresStock({ item: "Kid's Keyboard", lease }).quantity, -1);
        setPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', quantity: 9, lease });
        assert.equal(run('SELECT quantity FROM stock WHERE warehouse_id = 1;'), '9');
        for (const quantity of [-1, 1.5, NaN, Infinity]) {
          assert.throws(() => setPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', quantity, lease }));
        }
        assert.equal(run('SELECT quantity FROM stock WHERE warehouse_id = 1;'), '9');

        run(`ALTER TABLE stock RENAME TO inventory;
ALTER TABLE item RENAME TO products;
ALTER TABLE warehouse RENAME TO locations;
CREATE VIEW item AS SELECT id,name FROM products;
CREATE VIEW warehouse AS SELECT id,name FROM locations;
CREATE VIEW stock AS SELECT item_id,warehouse_id,quantity FROM inventory;`);
        assert.equal(getPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', lease }).quantity, 9);
        setPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', quantity: 7, lease });
        assert.equal(run('SELECT quantity FROM inventory WHERE warehouse_id = 1;'), '7');
        assert.equal(run('SELECT quantity FROM order_line;'), '17');
        const quietWrite = (quantity: number) => writePostgresStock(source =>
          docker(['exec', '-i', id, 'psql', '-U', 'appuser', '-d', 'bench', '-v', 'ON_ERROR_STOP=1', '-qAt'], source),
        { backend: 'supabase', label: 'Supabase', quiet: true }, { item: "Kid's Keyboard", warehouse: 'East', quantity });
        quietWrite(8);
        assert.equal(run('SELECT quantity FROM inventory WHERE warehouse_id = 1;'), '8');
        quietWrite(7);

        const rejectInterface = (error: unknown) => error instanceof Error
          && 'stockInterface' in error && error.stockInterface === true;
        for (const [table, source] of [['item', 'products'], ['warehouse', 'locations']] as const) {
          run(`DROP VIEW ${table}; CREATE VIEW ${table} AS SELECT id,name FROM ${source}
UNION ALL SELECT id,'Different name' FROM ${source} WHERE id=${table === 'item' ? 0 : 1};`);
          assert.throws(() => getPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', lease }), rejectInterface);
          assert.throws(() => setPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', quantity: 99, lease }), rejectInterface);
          assert.equal(run('SELECT quantity FROM inventory WHERE warehouse_id = 1;'), '7');
          run(`DROP VIEW ${table}; CREATE VIEW ${table} AS SELECT id,name FROM ${source};`);
        }
        run('INSERT INTO inventory VALUES (0, 99, 2);');
        assert.throws(() => getPostgresStock({ item: "Kid's Keyboard", lease }), rejectInterface);
        run('DELETE FROM inventory WHERE warehouse_id=99; DROP VIEW stock; CREATE VIEW stock AS SELECT item_id,warehouse_id,quantity::numeric+0.5 AS quantity FROM inventory;');
        assert.throws(() => getPostgresStock({ item: "Kid's Keyboard", lease }), rejectInterface);
        run('DROP VIEW stock; CREATE VIEW stock AS SELECT item_id,warehouse_id,sum(quantity)::integer AS quantity FROM inventory GROUP BY item_id,warehouse_id;');
        assert.throws(() => setPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', quantity: 99, lease }), rejectInterface);
        assert.throws(() => quietWrite(99), rejectInterface);
        assert.equal(run('SELECT quantity FROM inventory WHERE warehouse_id = 1;'), '7');
        run(`DROP VIEW stock; CREATE VIEW stock AS SELECT item_id,warehouse_id,quantity FROM inventory;
CREATE FUNCTION ignore_stock_write() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NEW; END $$;
CREATE TRIGGER ignore_stock_write INSTEAD OF UPDATE ON stock FOR EACH ROW EXECUTE FUNCTION ignore_stock_write();`);
        assert.throws(() => setPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', quantity: 99, lease }), rejectInterface);
        assert.throws(() => quietWrite(99), rejectInterface);
        assert.equal(run('SELECT quantity FROM inventory WHERE warehouse_id = 1;'), '7');
        run('DROP TRIGGER ignore_stock_write ON stock; DROP VIEW stock;');
        assert.throws(() => getPostgresStock({ item: "Kid's Keyboard", lease }), rejectInterface);
        assert.throws(() => setPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', quantity: 1, lease }), rejectInterface);
        run('CREATE VIEW stock AS SELECT item_id,warehouse_id,quantity FROM inventory;');
        const unavailable = (_command: string, args: readonly string[]): string => {
          if (args[0] === 'inspect') return id;
          throw new Error('database connection refused');
        };
        assert.throws(() => getPostgresStock({ item: "Kid's Keyboard", lease, exec: unavailable }),
          error => error instanceof Error && !('stockInterface' in error));
      } else {
        run(`const itemId = ObjectId('0123456789abcdef01234567');
db.item.insertOne({_id:itemId, name:"Kid's Keyboard"});
db.warehouse.insertMany([{id:0,name:'East'},{id:2,name:'West'}]);
db.stock.insertMany([{item_id:itemId.toHexString(),warehouse_id:0,quantity:5},
 {item_id:itemId,warehouse_id:2,quantity:-1}]);`);
        assert.equal(getMongoDbStock({ item: "Kid's Keyboard", lease }).quantity, 4);
        assert.equal(getMongoDbStock({ item: "Kid's Keyboard", warehouse: 'West', lease }).quantity, -1);
        run('db.stock.updateOne({warehouse_id:0},{$set:{quantity:0}})');
        assert.equal(getMongoDbStock({ item: "Kid's Keyboard", warehouse: 'East', lease }).quantity, 0);
        assert.throws(() => getMongoDbStock({ item: 'Absent', lease }), /missing/);
        run("db.stock.insertOne({item_id:'0123456789abcdef01234567',warehouse_id:0,quantity:2})");
        assert.throws(() => getMongoDbStock({ item: "Kid's Keyboard", lease }), /duplicated/);
      }
    } finally { docker(['rm', '-f', id]); }
  });
}
