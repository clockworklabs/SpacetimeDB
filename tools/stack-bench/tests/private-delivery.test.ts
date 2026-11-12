import assert from 'node:assert/strict';
import { brotliCompressSync } from 'node:zlib';
import test from 'node:test';
import { deliveredRecords, sameRecord } from '../src/actions/private-delivery.js';

const codec = await import(new URL('../src/stacks/spacetime-wire-codec.js', import.meta.url).href);
const frame = (message: unknown) => {
  const writer = new codec.BinaryWriter(256);
  codec.ServerMessage.serialize(writer, message);
  return Buffer.concat([Buffer.from([1]), brotliCompressSync(writer.getBuffer())]);
};
const rows = (table: string, data: number[][]) => ({
  table,
  rows: { sizeHint: { tag: 'RowOffsets', value: data.reduce<bigint[]>((starts, _row, index) =>
    [...starts, index ? starts.at(-1)! + BigInt(data[index - 1]!.length) : 0n], []) },
  rowsData: Uint8Array.from(data.flat()) },
});
// An alert row as one real app stored it: id, account id, item id, delivered flag, created time.
const alert = [...Array(8).fill(0).map((_, i) => i ? 0 : 7), ...Array(8).fill(0).map((_, i) => i ? 0 : 42),
  ...Array(8).fill(0).map((_, i) => i ? 0 : 3), 0, ...Array(8).fill(9)];

test('a SpacetimeDB row reaches another client whether it arrives as an update or a subscription', async () => {
  const owner = frame({ tag: 'TransactionUpdate', value: { querySets: [{ querySetId: { id: 1 }, tables: [{
    tableName: 'stock_alert', rows: [{ tag: 'PersistentTable', value: {
      inserts: { sizeHint: { tag: 'FixedSize', value: alert.length }, rowsData: Uint8Array.from(alert) },
      deletes: { sizeHint: { tag: 'FixedSize', value: alert.length }, rowsData: new Uint8Array() } } }] }] }] } });
  const other = frame({ tag: 'SubscribeApplied', value: { requestId: 3, querySetId: { id: 1 },
    rows: { tables: [rows('item', [[3, 0, 0, 0, 0, 0, 0, 0, 65, 105, 114]]), rows('stock_alert', [alert])] } } });
  const mine = await deliveredRecords([{ atMs: 1, payload: owner }]);
  const theirs = await deliveredRecords([{ atMs: 2, payload: other }]);
  assert.deepEqual([mine.unreadable, theirs.unreadable], [0, 0]);
  assert.equal(mine.records.length, 1);
  assert.equal(mine.records[0]!.kind, 'table:stock_alert');
  assert(theirs.records.some(record => sameRecord(mine.records[0]!, record)), 'the leaked row is found by its bytes');
  assert(!theirs.records.some(record => record.kind === 'table:item' && sameRecord(mine.records[0]!, record)));
});

test('JSON records match whole objects, not shared text such as a product name', async () => {
  const owner = await deliveredRecords([{ atMs: 1,
    payload: JSON.stringify({ alert: { id: 7, accountId: 42, itemId: 3, delivered: false }, ok: true }) }]);
  const leak = await deliveredRecords([{ atMs: 2,
    payload: `42["alerts",${JSON.stringify([{ delivered: false, itemId: 3, accountId: 42, id: 7 }])}]` }]);
  const publicItem = await deliveredRecords([{ atMs: 2, payload: JSON.stringify({ id: 3, name: 'Air Purifier', price: 189 }) }]);
  const own = owner.records.find(record => record.kind === 'json:accountId,delivered,id,itemId')!;
  assert(own, 'the alert object is a record; the bare status flag is not');
  assert(leak.records.some(record => sameRecord(own, record)), 'key order and Socket.IO framing do not hide it');
  assert(!publicItem.records.some(record => sameRecord(own, record)));
});

test('an unreadable binary delivery is counted, so absence cannot be claimed over it', async () => {
  const { unreadable } = await deliveredRecords([{ atMs: 1, payload: Buffer.from([9, 0xff, 0xfe, 0x00, 0x81]) }]);
  assert.equal(unreadable, 1);
});
