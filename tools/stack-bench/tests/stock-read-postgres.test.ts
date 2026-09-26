import assert from 'node:assert/strict';
import test from 'node:test';
import { getPostgresStock, setPostgresStock } from '../src/stacks/backends/postgres-operations.js';
import type { TextCommandExecutor } from '../src/runtime/command-executor.js';

const lease = { resources: { database: 'bench', container: { name: 'leased-pg', id: 'pg-id' } } };

test('stock reads use the leased database and preserve zero and negative observations', () => {
  for (const quantities of [[0], [-2], [4, -1]]) {
    let sql = '';
    const exec: TextCommandExecutor = (_command, args, options) => {
      if (args[0] === 'inspect') return 'pg-id';
      assert(args.includes('pg-id'));
      assert(args.includes('bench'));
      sql = options.input ?? '';
      return JSON.stringify({ items: 1, warehouses: quantities.length, quantities });
    };
    const result = getPostgresStock({ item: "Kid's Keyboard", lease, exec });
    assert.equal(result.quantity, quantities.reduce((sum, n) => sum + n, 0));
    assert.match(sql, /Kid''s Keyboard/);
    assert.doesNotMatch(sql, /\b(?:UPDATE|INSERT|DELETE)\b/);
  }
});

test('stock reads refuse changed ownership and missing, ambiguous, or invalid data', () => {
  assert.throws(() => getPostgresStock({ item: 'Keyboard', lease, exec: () => 'replacement-id' }),
    /changed after lease/);
  for (const [output, expected] of [
    ['', /no result/],
    ['{}\n{}', /multiple relational/],
    [JSON.stringify({ items: 2, warehouses: 1, quantities: [2] }), /ambiguous/],
    [JSON.stringify({ items: 1, warehouses: 1, quantities: [2, 3] }), /ambiguous/],
    [JSON.stringify({ items: 1, warehouses: 1, quantities: [null] }), /whole number/],
    [JSON.stringify({ items: 1, warehouses: 1, quantities: ['2'] }), /whole number/],
    [JSON.stringify({ items: 1, warehouses: 1, quantities: [1.5] }), /whole number/],
    [JSON.stringify({ items: 1, warehouses: 2, quantities: [Number.MAX_SAFE_INTEGER, 1] }), /whole number/],
    [JSON.stringify({ items: 1, warehouses: 3, quantities: [Number.MAX_SAFE_INTEGER, 2, -2] }), /whole number/],
  ] as const) {
    assert.throws(() => getPostgresStock({ item: 'Keyboard', lease,
      exec: (_command, args) => args[0] === 'inspect' ? 'pg-id' : output }), expected);
  }
  let sql = '';
  assert.equal(getPostgresStock({ item: 'Keyboard', warehouse: "West's", lease,
    exec: (_command, args, options) => {
      if (args[0] === 'inspect') return 'pg-id';
      sql = options.input ?? '';
      return '{"items":1,"warehouses":1,"namedWarehouses":1,"quantities":[2]}';
    } }).quantity, 2);
  assert.match(sql, /West''s/);
});

const stockError = (fields: { missingRow?: string; invalid?: boolean }) => (error: unknown): boolean =>
  error instanceof Error && 'stockInterface' in error && error.stockInterface === true
  && ('missingRow' in error ? error.missingRow : undefined) === fields.missingRow
  && ('stockInterfaceInvalid' in error && error.stockInterfaceInvalid === true) === (fields.invalid === true);

test('stock reads name the missing parent and refuse ambiguous parents without stock', () => {
  for (const [counts, expected] of [
    [{ items: 0, namedWarehouses: 1 }, { missingRow: 'item' }],
    [{ items: 1, namedWarehouses: 0 }, { missingRow: 'warehouse' }],
    [{ items: 2, namedWarehouses: 1 }, { invalid: true }],
    [{ items: 1, namedWarehouses: 2 }, { invalid: true }],
    [{ items: 1, namedWarehouses: 1 }, { missingRow: 'stock' }],
  ] as const) {
    assert.throws(() => getPostgresStock({ item: 'Keyboard', warehouse: 'East', lease,
      exec: (_command, args) => args[0] === 'inspect' ? 'pg-id'
        : JSON.stringify({ ...counts, warehouses: 0, quantities: [] }) }), stockError(expected));
  }
  let sql = '';
  assert.throws(() => getPostgresStock({ item: 'Keyboard', lease, exec: (_command, args, options) => {
    if (args[0] === 'inspect') return 'pg-id';
    sql = options.input ?? '';
    return JSON.stringify({ items: 1, namedWarehouses: 0, warehouses: 0, quantities: [] });
  } }), stockError({ missingRow: 'warehouse' }));
  assert.doesNotMatch(sql, /HAVING/, 'an empty read still reports its parent counts');
});

test('stock writes change one row only under one named item and warehouse', () => {
  const write = (output: string) => {
    let sql = '';
    const run = () => setPostgresStock({ item: "Kid's Keyboard", warehouse: 'East', quantity: 3, lease,
      exec: (_command, args, options) => {
        if (args[0] === 'inspect') return 'pg-id';
        sql = options.input ?? '';
        return output;
      } });
    return { run, sql: () => sql };
  };
  const ok = write('UPDATE 1\n{"items":1,"warehouses":1,"stocks":1}\n');
  assert.deepEqual(ok.run(), { backend: 'postgres', item: "Kid's Keyboard", warehouse: 'East', quantity: 3 });
  assert.match(ok.sql(), /\(SELECT count\(\*\) FROM public\.item WHERE name = 'Kid''s Keyboard'\) = 1/);
  assert.match(ok.sql(), /\(SELECT count\(\*\) FROM public\.warehouse WHERE name = 'East'\) = 1/);
  assert.match(ok.sql(), /linked\.item_id = item\.id AND linked\.warehouse_id = warehouse\.id\) = 1/);
  for (const [counts, expected] of [
    [{ items: 0, warehouses: 1, stocks: 0 }, { missingRow: 'item' }],
    [{ items: 1, warehouses: 0, stocks: 0 }, { missingRow: 'warehouse' }],
    [{ items: 2, warehouses: 1, stocks: 2 }, { invalid: true }],
    [{ items: 2, warehouses: 1, stocks: 0 }, { invalid: true }],
    [{ items: 1, warehouses: 2, stocks: 1 }, { invalid: true }],
    [{ items: 1, warehouses: 1, stocks: 2 }, { invalid: true }],
    [{ items: 1, warehouses: 1, stocks: 0 }, { missingRow: 'stock' }],
  ] as const) {
    assert.throws(write(`UPDATE 0\n${JSON.stringify(counts)}\n`).run, stockError(expected), JSON.stringify(counts));
  }
  for (const output of ['UPDATE 0\n', 'UPDATE 0\n{"items":1}\n']) {
    assert.throws(write(output).run, (error: unknown) => error instanceof Error
      && !('stockInterface' in error) && /invalid result/.test(error.message));
  }
});
