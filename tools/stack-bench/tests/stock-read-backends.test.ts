import assert from 'node:assert/strict';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { getMongoDbStock, setMongoDbStock } from '../src/stacks/backends/mongodb-operations.js';
import { getSpacetimeStock } from '../src/stacks/backends/spacetime-operations.js';
import { getSupabaseStock, setSupabaseStock } from '../src/stacks/backends/supabase-operations.js';
import { supabaseAdapter } from '../src/stacks/backends/supabase-adapter.js';
import { supabaseExec, supabaseLease } from './helpers/supabase-lease.js';

const container = { name: 'leased', id: 'leased-id' };
const lease = { resources: { container, database: 'app' } };
const spacetime = { buildContainer: container, mod: 'app', containerUri: 'http://database:3000' };
const table = (columns: string[], rows: unknown[][]): string =>
  [columns.join(' | '), columns.map(() => '---').join('+'), ...rows.map(row => row.join(' | '))].join('\n');
const interfaceFailure = (error: unknown): boolean => error instanceof Error
  && 'stockInterface' in error && error.stockInterface === true;

class ObjectId {
  readonly _bsontype = 'ObjectId';
  constructor(readonly value: string) {}
  toHexString(): string { return this.value; }
}

type Document = Record<string, unknown>;
function mongoExec(data: Record<string, Document[]>) {
  return (_command: string, args: readonly string[]): string => {
    if (args[0] === 'inspect') return container.id;
    const equal = (a: unknown, b: unknown): boolean => a instanceof ObjectId && b instanceof ObjectId
      ? a.value === b.value : a === b;
    const match = (row: Document, query: Document): boolean => Object.entries(query).every(([key, value]) => {
      if (key === '$or') return (value as Document[]).some(part => match(row, part));
      if (value && typeof value === 'object' && '$in' in value) {
        return (value.$in as unknown[]).some(candidate => equal(row[key], candidate));
      }
      return equal(row[key], value);
    });
    const db = Object.fromEntries(Object.entries(data).map(([name, rows]) => [name, {
      find: (query: Document) => {
        const found = rows.filter(row => match(row, query));
        return { toArray: () => found, limit: (count: number) => ({ toArray: () => found.slice(0, count) }) };
      },
      updateOne: (query: Document, update: { $set: Document }) => {
        const found = rows.find(row => match(row, query));
        if (found) Object.assign(found, update.$set);
        return { matchedCount: found ? 1 : 0 };
      },
    }]));
    let output = '';
    const stopped = { code: 0 };
    try {
      runInNewContext(args.at(-1)!, { db: { ...db, getCollectionNames: () => Object.keys(data) },
        ObjectId, print: (value: string) => { output += value; },
        quit: (code = 0) => { stopped.code = code; throw stopped; } });
    } catch (error) { if (error !== stopped) throw error; }
    // mongosh exits non-zero after quit(1); the child process error carries stdout.
    if (stopped.code) throw Object.assign(new Error('mongosh exited 1'), { stdout: output });
    return output;
  };
}

test('MongoDB stock reads preserve ObjectId/string references, zero and negative values', () => {
  const itemId = '0123456789abcdef01234567';
  const exec = mongoExec({ item: [{ _id: new ObjectId(itemId), name: 'Widget' }],
    warehouse: [{ _id: 1, name: 'East' }, { _id: 2, name: 'West' }],
    stock: [{ item_id: itemId, warehouse_id: 1, quantity: 0 },
      { item_id: new ObjectId(itemId), warehouse_id: 2, quantity: -3 }] });
  assert.equal(getMongoDbStock({ item: 'Widget', warehouse: 'East', lease, exec }).quantity, 0);
  assert.equal(getMongoDbStock({ item: 'Widget', lease, exec }).quantity, -3);
});

test('MongoDB stock reads and writes use only the declared item_id/warehouse_id fields', () => {
  const data = () => ({ item: [{ id: 1, name: 'Widget' }], warehouse: [{ id: 2, name: 'East' }],
    stock: [{ _id: 's', itemId: 1, warehouseId: 2, quantity: 3 }] as Document[] });
  const missingStock = (error: unknown): boolean => interfaceFailure(error)
    && (error as { missingRow?: unknown }).missingRow === 'stock';
  assert.throws(() => getMongoDbStock({ item: 'Widget', warehouse: 'East', lease, exec: mongoExec(data()) }), missingStock);
  assert.throws(() => getMongoDbStock({ item: 'Widget', lease, exec: mongoExec(data()) }), missingStock);
  const undeclared = data();
  assert.throws(() => setMongoDbStock({ item: 'Widget', warehouse: 'East', quantity: 9, lease,
    exec: mongoExec(undeclared) }), missingStock);
  assert.equal(undeclared.stock[0]!.quantity, 3);
  // A declared row without its warehouse_id is not read through a camelCase fallback.
  const partial = { ...data(), stock: [{ item_id: 1, warehouseId: 2, quantity: 3 }] };
  assert.throws(() => getMongoDbStock({ item: 'Widget', lease, exec: mongoExec(partial) }),
    (error: unknown) => interfaceFailure(error) && (error as { stockInterfaceInvalid?: unknown }).stockInterfaceInvalid === true);
});

test('MongoDB stock writes refuse ambiguous rows and non-numeric quantities as interface failures', () => {
  const valid = () => ({ item: [{ id: 1, name: 'Widget' }], warehouse: [{ id: 2, name: 'East' }],
    stock: [{ _id: 's', item_id: 1, warehouse_id: 2, quantity: 3 }] as Document[] });
  const written = valid();
  assert.deepEqual(setMongoDbStock({ item: 'Widget', warehouse: 'East', quantity: 0, lease, exec: mongoExec(written) }),
    { backend: 'mongodb', item: 'Widget', warehouse: 'East', quantity: 0 });
  assert.equal(written.stock[0]!.quantity, 0);
  const invalid = (error: unknown): boolean => interfaceFailure(error)
    && (error as { stockInterfaceInvalid?: unknown }).stockInterfaceInvalid === true;
  for (const [defect, data] of Object.entries({
    'duplicate item': { ...valid(), item: [{ id: 1, name: 'Widget' }, { id: 3, name: 'Widget' }] },
    'duplicate warehouse': { ...valid(), warehouse: [{ id: 2, name: 'East' }, { id: 4, name: 'East' }] },
    'duplicate stock': { ...valid(), stock: [...valid().stock, { _id: 't', item_id: 1, warehouse_id: 2, quantity: 1 }] },
    'absent quantity': { ...valid(), stock: [{ _id: 's', item_id: 1, warehouse_id: 2 }] },
    'text quantity': { ...valid(), stock: [{ _id: 's', item_id: 1, warehouse_id: 2, quantity: '3' }] },
    'fractional quantity': { ...valid(), stock: [{ _id: 's', item_id: 1, warehouse_id: 2, quantity: 0.5 }] },
  })) {
    const before = JSON.stringify(data.stock);
    assert.throws(() => setMongoDbStock({ item: 'Widget', warehouse: 'East', quantity: 9, lease,
      exec: mongoExec(data) }), invalid, defect);
    assert.equal(JSON.stringify(data.stock), before, `${defect} must not be written`);
  }
});

test('MongoDB stock reads reject missing, duplicate and invalid data', () => {
  const valid = { item: [{ id: 1, name: 'Widget' }], warehouse: [{ id: 2, name: 'East' }],
    stock: [{ item_id: 1, warehouse_id: 2, quantity: 3 }] };
  for (const data of [
    { ...valid, item: [] }, { ...valid, item: [...valid.item, ...valid.item] },
    { ...valid, warehouse: [...valid.warehouse, ...valid.warehouse] },
    { ...valid, stock: [] }, { ...valid, stock: [...valid.stock, ...valid.stock] },
    ...[null, '3', 0.5, Number.MAX_SAFE_INTEGER + 1].map(quantity =>
      ({ ...valid, stock: [{ ...valid.stock[0], quantity }] })),
  ]) assert.throws(() => getMongoDbStock({ item: 'Widget', warehouse: 'East', lease,
    exec: mongoExec(data) }), interfaceFailure);
});

function spacetimeExec(stock: unknown[][], itemRows = [['1']], warehouseRows = [['2'], ['3']]) {
  return (_command: string, args: readonly string[]): string => {
    if (args[0] === 'inspect') return container.id;
    const sql = args.at(-1)!;
    if (sql.startsWith('select id from item')) return table(['id'], itemRows);
    if (sql.startsWith('select id from warehouse')) {
      return table(['id'], sql.includes('where name') ? warehouseRows.slice(0, 1) : warehouseRows);
    }
    assert.match(sql, /^select warehouse_id, quantity from stock/);
    return table(['warehouse_id', 'quantity'], stock);
  };
}

test('SpacetimeDB stock reads parse complete rows and preserve zero and negative values', () => {
  assert.equal(getSpacetimeStock({ item: 'Widget', warehouse: 'East', spacetime,
    exec: spacetimeExec([[2, 0]]) }).quantity, 0);
  assert.equal(getSpacetimeStock({ item: 'Widget', spacetime,
    exec: spacetimeExec([[2, 3], [3, -5]]) }).quantity, -2);
});

test('SpacetimeDB stock reads reject missing, duplicate, malformed and unsafe rows', () => {
  for (const stock of [[], [[2, 1], [2, 2]], [[99, 1]], [[2, 'NaN']], [[2, 0.5]],
    [[2, Number.MAX_SAFE_INTEGER + 1]]]) {
    assert.throws(() => getSpacetimeStock({ item: 'Widget', spacetime,
      exec: spacetimeExec(stock) }), interfaceFailure);
  }
  for (const ids of [[], [['1'], ['2']]]) {
    assert.throws(() => getSpacetimeStock({ item: 'Widget', spacetime,
      exec: spacetimeExec([[2, 1]], ids) }), interfaceFailure);
  }
  assert.throws(() => getSpacetimeStock({ item: 'Widget', spacetime,
    exec: (_command, args) => args[0] === 'inspect' ? container.id : 'id\n999' }), /invalid table header/);
});

test('stock readers reject a replaced container before querying', () => {
  let calls = 0;
  const exec = (_command: string, args: readonly string[]): string => {
    calls += 1;
    assert.equal(args[0], 'inspect');
    return 'different-id';
  };
  assert.throws(() => getMongoDbStock({ item: 'Widget', lease, exec }), /changed after lease/);
  assert.throws(() => getSpacetimeStock({ item: 'Widget', spacetime, exec }), /changed after lease/);
  assert.equal(calls, 2);
});

test('Supabase stock reads run the relational stock SQL as the privileged role', () => {
  const platform = supabaseLease();
  const lease = supabaseAdapter.grading.databaseLease(platform);
  const calls: { args: readonly string[] }[] = [];
  const read = (output: unknown, warehouse?: string) => getSupabaseStock({ item: "Kid's Keyboard", warehouse, lease,
    exec: supabaseExec(platform, sql => { assert.match(sql, /'Kid''s Keyboard'/); return JSON.stringify(output); }, calls) });
  assert.deepEqual(read({ items: 1, warehouses: 2, quantities: [0, -3] }),
    { backend: 'supabase', item: "Kid's Keyboard", quantity: -3 });
  const psql = calls.find(call => call.args.includes('psql'))!.args;
  assert.deepEqual(psql.slice(psql.indexOf('-U'), psql.indexOf('-U') + 4), ['-U', 'supabase_admin', '-d', 'postgres']);
  assert.throws(() => read({ items: 0, namedWarehouses: 1, warehouses: 0, quantities: [] }, 'East'),
    (error: unknown) => interfaceFailure(error) && (error as { missingRow?: unknown }).missingRow === 'item');
  const missing = Object.assign(new Error('psql failed'), { stderr: 'ERROR:  relation "public.stock" does not exist' });
  assert.throws(() => getSupabaseStock({ item: 'Keyboard', lease,
    exec: supabaseExec(platform, () => { throw missing; }) }), interfaceFailure);
});

test('Supabase stock writes change one row under one named item and warehouse and name what is missing', () => {
  const platform = supabaseLease();
  const lease = supabaseAdapter.grading.databaseLease(platform);
  const write = (output: unknown) => {
    let sql = '';
    const run = () => setSupabaseStock({ item: "Kid's Keyboard", warehouse: 'East', quantity: 3, lease,
      exec: supabaseExec(platform, input => { sql = input; return `${JSON.stringify(output)}\n`; }) });
    return { run, sql: () => sql };
  };
  const ok = write({ updated: 1, items: 1, warehouses: 1, stocks: 1 });
  assert.deepEqual(ok.run(), { backend: 'supabase', item: "Kid's Keyboard", warehouse: 'East', quantity: 3 });
  // Quiet psql prints no command tag, so the update reports its rows in its result.
  assert.match(ok.sql(), /RETURNING 1\)\nSELECT json_build_object\('updated', \(SELECT count\(\*\) FROM updated\)/);
  const stockError = (fields: { missingRow?: string; invalid?: boolean }) => (error: unknown): boolean =>
    interfaceFailure(error) && (error as { missingRow?: unknown }).missingRow === fields.missingRow
    && (error as { stockInterfaceInvalid?: unknown }).stockInterfaceInvalid === (fields.invalid === true);
  for (const [counts, expected] of [
    [{ items: 0, warehouses: 1, stocks: 0 }, { missingRow: 'item' }],
    [{ items: 1, warehouses: 0, stocks: 0 }, { missingRow: 'warehouse' }],
    [{ items: 2, warehouses: 1, stocks: 0 }, { invalid: true }],
    [{ items: 1, warehouses: 1, stocks: 2 }, { invalid: true }],
    [{ items: 1, warehouses: 1, stocks: 0 }, { missingRow: 'stock' }],
  ] as const) {
    assert.throws(write({ updated: 0, ...counts }).run, stockError(expected), JSON.stringify(counts));
  }
  for (const output of [{ items: 1, warehouses: 1, stocks: 1 }, 'UPDATE 1']) {
    assert.throws(write(output).run, (error: unknown) => error instanceof Error
      && !('stockInterface' in error) && /invalid result/.test(error.message));
  }
});

test('Supabase observers refuse a replaced platform container', () => {
  const platform = supabaseLease();
  const exec = supabaseExec(platform, () => { throw new Error('must not query'); });
  const replaced: typeof exec = (command, args, options) => args[0] === 'inspect'
    ? JSON.stringify({ Id: 'd'.repeat(64), State: { Running: true } }) : exec(command, args, options);
  assert.throws(() => getSupabaseStock({ item: 'Keyboard', lease: supabaseAdapter.grading.databaseLease(platform),
    exec: replaced }), /changed after lease creation/);
});
