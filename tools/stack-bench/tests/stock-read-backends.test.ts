import assert from 'node:assert/strict';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { getMongoDbStock } from '../src/stacks/backends/mongodb-operations.js';
import { getSpacetimeStock } from '../src/stacks/backends/spacetime-operations.js';

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
    }]));
    let output = '';
    const stopped = {};
    try {
      runInNewContext(args.at(-1)!, { db: { ...db, getCollectionNames: () => Object.keys(data) },
        ObjectId, print: (value: string) => { output += value; }, quit: () => { throw stopped; } });
    } catch (error) { if (error !== stopped) throw error; }
    return output;
  };
}

test('MongoDB stock reads preserve ObjectId/string references, zero and negative values', () => {
  const itemId = '0123456789abcdef01234567';
  const exec = mongoExec({ item: [{ _id: new ObjectId(itemId), name: 'Widget' }],
    warehouse: [{ _id: 1, name: 'East' }, { _id: 2, name: 'West' }],
    stock: [{ item_id: itemId, warehouse_id: 1, quantity: 0 },
      { itemId: new ObjectId(itemId), warehouseId: 2, quantity: -3 }] });
  assert.equal(getMongoDbStock({ item: 'Widget', warehouse: 'East', lease, exec }).quantity, 0);
  assert.equal(getMongoDbStock({ item: 'Widget', lease, exec }).quantity, -3);
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
