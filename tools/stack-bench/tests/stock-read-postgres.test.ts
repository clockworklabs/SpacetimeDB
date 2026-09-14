import assert from 'node:assert/strict';
import test from 'node:test';
import { getPostgresStock } from '../src/stacks/backends/postgres-operations.js';
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
    assert.match(sql, /json_agg\(stock/);
  }
});

test('stock reads refuse changed ownership and missing, ambiguous, or invalid data', () => {
  assert.throws(() => getPostgresStock({ item: 'Keyboard', lease, exec: () => 'replacement-id' }),
    /changed after lease/);
  for (const [output, expected] of [
    ['', /no stock data/],
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
