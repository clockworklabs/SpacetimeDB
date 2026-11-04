import { describe, expect, test } from 'vitest';
import { ConnectionId } from '../src/lib/connection_id';
import { Timestamp } from '../src/lib/timestamp';
import { TimeDuration } from '../src/lib/time_duration';
import { Uuid } from '../src/lib/uuid';
import { Identity } from '../src/lib/identity';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { t, type ColumnBuilder } from '../src/lib/type_builders';
import { deepEqual } from '../src/lib/util';
import type { EventContextInterface } from '../src/sdk/event_context';
import { TableCacheImpl, type Operation } from '../src/sdk/table_cache';

const context: EventContextInterface<any> = {
  db: {},
  reducers: {},
  isActive: true,
  event: { id: 'test', tag: 'SubscribeApplied' },
  subscriptionBuilder: () => {
    throw new Error('not used');
  },
  disconnect: () => {},
};

function makeCache(keyColumn: ColumnBuilder<any, any> = t.string().unique()) {
  const schema = tablesToSchema(new ModuleContext(), {
    item: table({ name: 'item' }, { id: t.u32().primaryKey(), key: keyColumn }),
  });
  return new TableCacheImpl<any, string>(schema.tables.item as any);
}

const find = (cache: TableCacheImpl<any, string>, key: unknown) =>
  (cache as any).key.find(key);

const op = (
  type: 'insert' | 'delete',
  id: number,
  key: unknown
): Operation => ({ type, rowId: id, row: { id, key } });

function apply(cache: TableCacheImpl<any, string>, operations: Operation[]) {
  const callbacks = cache.applyOperations(operations, context);
  callbacks.forEach(callback => callback.cb());
  return callbacks;
}

describe('unique cache indexes', () => {
  test.each([100, 10000])(
    'lookup work does not grow with %i cached rows',
    size => {
      const cache = makeCache();
      let reads = 0;
      cache.applyOperations(
        Array.from({ length: size }, (_, id) => ({
          type: 'insert' as const,
          rowId: id,
          row: {
            id,
            get key() {
              reads++;
              return `key-${id}`;
            },
          },
        })),
        context
      );
      reads = 0;
      expect(find(cache, `key-${size - 1}`)?.id).toBe(size - 1);
      expect(find(cache, 'missing')).toBeNull();
      expect(reads).toBeLessThanOrEqual(2);
    }
  );

  test('follows replacements, overlapping references and final deletion', () => {
    const cache = makeCache();
    apply(cache, [op('insert', 1, 'old'), op('insert', 1, 'old')]);
    const replacement = op('insert', 1, 'replacement');
    apply(cache, [replacement]);
    expect(find(cache, 'old')).toBeNull();
    expect(find(cache, 'replacement')).toBe(replacement.row);

    const update = op('insert', 1, 'new');
    cache.onUpdate(() => {
      expect(find(cache, 'replacement')).toBeNull();
      expect(find(cache, 'new')).toBe(update.row);
    });
    apply(cache, [op('delete', 1, 'replacement'), update]);
    // Even a partial delete replaces the stored object in the existing cache.
    const partial = op('delete', 1, 'new');
    apply(cache, [partial]);
    expect(find(cache, 'new')).toBe(partial.row);
    apply(cache, [partial]);
    expect(find(cache, 'new')).toBe(partial.row);
    cache.onDelete(() => expect(find(cache, 'new')).toBeNull());
    apply(cache, [partial]);
    expect(find(cache, 'new')).toBeNull();
  });

  test('reconnect refreshes identical row objects and subscription counts', () => {
    const cache = makeCache();
    apply(cache, [op('insert', 1, 'same'), op('insert', 1, 'same')]);
    const replacement = op('insert', 1, 'same');
    expect(
      cache.applyOperations(
        [...cache.snapshotDeleteOperations(), replacement],
        context,
        { skipIdenticalUpdates: true }
      )
    ).toEqual([]);
    expect(find(cache, 'same')).toBe(replacement.row);
    apply(cache, [op('delete', 1, 'same')]);
    expect(find(cache, 'same')).toBeNull();
  });

  test.each([
    ['number', t.u32().unique(), 42, 42],
    ['bigint', t.u256().unique(), 42n, 42n],
    ['boolean', t.bool().unique(), true, true],
    ['string', t.string().unique(), 'hello', 'hello'],
    ['identity', t.identity().unique(), new Identity(42n), new Identity(42n)],
    [
      'connection',
      t.connectionId().unique(),
      new ConnectionId(42n),
      new ConnectionId(42n),
    ],
    [
      'timestamp',
      t.timestamp().unique(),
      new Timestamp(42n),
      new Timestamp(42n),
    ],
    [
      'duration',
      t.timeDuration().unique(),
      new TimeDuration(42n),
      new TimeDuration(42n),
    ],
    ['uuid', t.uuid().unique(), new Uuid(42n), new Uuid(42n)],
  ] as const)(
    'looks up %s keys from resolved schema metadata',
    (_name, column, stored, query) => {
      const cache = makeCache(column);
      const insert = op('insert', 1, stored);
      apply(cache, [insert]);
      expect(find(cache, query)).toBe(insert.row);
      expect(find(cache, [query])).toBe(insert.row);
      expect((cache as any).id.find(1)).toBe(insert.row);
      apply(cache, [op('delete', 1, stored)]);
      expect(find(cache, query)).toBeNull();
      expect((cache as any).id.find(1)).toBeNull();
    }
  );

  test('does not cache event-table rows', () => {
    const schema = tablesToSchema(new ModuleContext(), {
      event: table(
        { name: 'event', event: true },
        { key: t.string().unique() }
      ),
    });
    const cache = new TableCacheImpl<any, string>(schema.tables.event as any);
    cache.onInsert(() => expect(find(cache, 'event')).toBeNull());
    apply(cache, [op('insert', 1, 'event')]);
    expect(find(cache, 'event')).toBeNull();
  });

  // Exercise the runtime equality contract beyond the statically typed query API.
  test('maintains unique indexes without a primary key', () => {
    const schema = tablesToSchema(new ModuleContext(), {
      item: table({ name: 'item' }, { key: t.string().unique() }),
    });
    const cache = new TableCacheImpl<any, string>(schema.tables.item as any);
    const insert = op('insert', 1, 'value');
    apply(cache, [insert, insert]);
    apply(cache, [op('delete', 1, 'value')]);
    expect(find(cache, 'value')).not.toBeNull();
    apply(cache, [op('delete', 1, 'value')]);
    expect(find(cache, 'value')).toBeNull();
  });

  test('preserves tuple normalization for compound runtime metadata', () => {
    const schema = tablesToSchema(new ModuleContext(), {
      item: table(
        { name: 'item' },
        { id: t.u32().primaryKey(), key: t.string() }
      ),
    });
    // The runtime supports tuple keys, although the current TS schema builder
    // restricts unique constraints to one column.
    const definition = {
      ...schema.tables.item,
      resolvedIndexes: [
        { name: 'compound', algorithm: 'btree', columns: ['key', 'id'] },
      ],
      constraints: [{ constraint: 'unique', columns: ['key', 'id'] }],
    };
    const cache = new TableCacheImpl<any, string>(definition as any);
    const index = (cache as any).compound;
    const insert = op('insert', 1, 'value');
    apply(cache, [insert]);
    expect(index.find(['value', 1])).toBe(insert.row);
    expect(index.find(['value'])).toBeNull();
    expect(index.find([1, 'value'])).toBeNull();
    expect(index.find(['value', 1, 2])).toBeNull();
    apply(cache, [op('delete', 1, 'value'), op('insert', 1, 'changed')]);
    expect(index.find(['value', 1])).toBeNull();
    expect(index.find(['changed', 1])?.key).toBe('changed');
  });

  test('matches deep equality rather than object identity or string coercion', () => {
    const cache = makeCache();
    const keys = [
      '1',
      1,
      1n,
      true,
      null,
      undefined,
      0,
      NaN,
      new Identity(123n),
      { a: 1n, b: { value: 'x' } },
      new Uint8Array([1, 2]),
      { optional: undefined },
      [1, 2],
      new Array(3),
    ];
    apply(
      cache,
      keys.map((key, id) => op('insert', id, key))
    );
    const queries = [
      ...keys,
      -0,
      new Identity(123n),
      { __identity__: 123n },
      { b: { value: 'x' }, a: 1n },
      new Uint8Array([1, 2]),
      { 0: 1, 1: 2 },
      'missing',
      { a: 1n },
      {},
      [[]],
      [[1, 2]],
      { optional: undefined },
    ];
    for (const query of queries) {
      const expected =
        Array.from(cache.iter()).find(row =>
          deepEqual([row.key], Array.isArray(query) ? query : [query])
        ) ?? null;
      expect(find(cache, query)).toBe(expected);
    }
  });

  test('keeps table insertion order when duplicate keys are replaced', () => {
    const cache = makeCache();
    apply(cache, [op('insert', 1, 'same'), op('insert', 2, 'same')]);
    apply(cache, [op('delete', 1, 'same'), op('insert', 1, 'same')]);
    expect(find(cache, 'same')?.id).toBe(1);
    apply(cache, [op('delete', 1, 'same')]);
    expect(find(cache, 'same')?.id).toBe(2);
  });
});
