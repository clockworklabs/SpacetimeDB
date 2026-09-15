import { describe, expect, it, vi } from 'vitest';
import { ConnectionId, Identity, Timestamp, Uuid } from '../src';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import type { ReadonlyIndexes } from '../src/lib/indexes';
import {
  table,
  type TableIndexes,
  type UntypedTableDef,
} from '../src/lib/table';
import { t, type TypeBuilder } from '../src/lib/type_builders';
import { TableCacheImpl, type Operation } from '../src/sdk/table_cache';
import { Range } from '../src/server/range';
import { tables } from '../test-app/src/module_bindings';

function cacheFor<T extends UntypedTableDef>(tableDef: T) {
  return new TableCacheImpl<any, string>(tableDef) as TableCacheImpl<
    any,
    string
  > &
    ReadonlyIndexes<T, TableIndexes<T>>;
}

const item = table(
  {
    name: 'item',
    indexes: [
      {
        accessor: 'byTeamLevel',
        algorithm: 'btree',
        columns: ['team', 'level'],
      },
      { accessor: 'byTeamName', algorithm: 'btree', columns: ['team', 'name'] },
    ],
  },
  {
    id: t.u32().primaryKey(),
    name: t.string().unique(),
    team: t.string().index('btree'),
    level: t.u64(),
  }
);
const itemDef = tablesToSchema(new ModuleContext(), { item }).tables.item;
const rows = [
  { id: 1, name: 'one', team: 'red', level: 1n },
  { id: 2, name: 'two', team: 'blue', level: 2n },
  { id: 3, name: 'three', team: 'red', level: 1n },
  { id: 4, name: 'four', team: 'red', level: 3n },
];

function op(type: Operation['type'], row: (typeof rows)[number]): Operation {
  return { type, rowId: row.id, row };
}

function populatedCache() {
  const cache = cacheFor(itemDef);
  cache.applyOperations(
    rows.map(row => op('insert', row)),
    {} as any
  );
  return cache;
}

describe('dictionary-backed table cache indexes', () => {
  it('finds unique keys and filters all matching non-unique keys without scanning', () => {
    const cache = populatedCache();
    vi.spyOn(cache, 'iter').mockImplementation(() => {
      throw new Error('Equality lookups must not scan the cache');
    });
    expect(cache.id.find(1)).toBe(rows[0]);
    expect(cache.name.find('two')).toBe(rows[1]);
    expect(cache.id.find(100)).toBeNull();
    expect(cache.name.find('missing')).toBeNull();
    expect([...cache.team.filter('red')]).toEqual([rows[0], rows[2], rows[3]]);
    expect([...cache.team.filter('missing')]).toEqual([]);
    expect([...cache.byTeamLevel.filter(['red', 1n])]).toEqual([
      rows[0],
      rows[2],
    ]);
    expect([...cache.byTeamLevel.filter('red')]).toEqual([
      rows[0],
      rows[2],
      rows[3],
    ]);
    expect([...cache.byTeamLevel.filter(['red', 2n])]).toEqual([]);
  });

  it('updates every index before callbacks and returns the latest row object', () => {
    const cache = populatedCache();
    const updated = { ...rows[0], name: 'updated', team: 'blue', level: 9n };
    const check = () => {
      expect(cache.id.find(1)).toBe(updated);
      expect(cache.name.find('one')).toBeNull();
      expect(cache.name.find('updated')).toBe(updated);
      expect([...cache.team.filter('red')]).toEqual([rows[2], rows[3]]);
      expect([...cache.team.filter('blue')]).toEqual([rows[1], updated]);
      expect([...cache.byTeamLevel.filter(['red', 1n])]).toEqual([rows[2]]);
      expect([...cache.byTeamLevel.filter(['blue', 9n])]).toEqual([updated]);
    };
    cache.onUpdate(check);
    const callbacks = cache.applyOperations(
      [op('delete', rows[0]), op('insert', updated)],
      {} as any
    );
    check();
    callbacks.forEach(callback => callback.cb());

    // An update which leaves the indexed keys unchanged must still expose the new object.
    const refreshed = { ...updated };
    cache.applyOperations(
      [op('delete', updated), op('insert', refreshed)],
      {} as any
    );
    expect(cache.id.find(1)).toBe(refreshed);
    expect([...cache.byTeamLevel.filter(['blue', 9n])][0]).toBe(refreshed);
  });

  it('keeps one indexed entry until the last subscription reference is removed', () => {
    const cache = populatedCache();
    const duplicate = { ...rows[0] };
    cache.applyOperations([op('insert', duplicate)], {} as any);
    expect(cache.id.find(1)).toBe(duplicate);
    expect([...cache.byTeamLevel.filter(['red', 1n])]).toEqual([
      duplicate,
      rows[2],
    ]);
    cache.applyOperations([op('delete', duplicate)], {} as any);
    expect(cache.id.find(1)).toBe(duplicate);
    expect([...cache.team.filter('red')]).toHaveLength(3);
    cache.applyOperations([op('delete', duplicate)], {} as any);
    expect(cache.id.find(1)).toBeNull();
    expect(cache.name.find('one')).toBeNull();
    expect([...cache.byTeamLevel.filter(['red', 1n])]).toEqual([rows[2]]);
    cache.applyOperations([op('delete', rows[2])], {} as any);
    expect([...cache.byTeamLevel.filter(['red', 1n])]).toEqual([]);
    cache.applyOperations([op('insert', rows[0])], {} as any);
    expect(cache.id.find(1)).toBe(rows[0]);
    expect([...cache.byTeamLevel.filter(['red', 1n])]).toEqual([rows[0]]);
  });

  it('handles updates which change subscription reference counts', () => {
    const cache = populatedCache();
    const updated = { ...rows[0], team: 'green' };
    cache.applyOperations(
      [op('delete', rows[0]), op('insert', updated), op('insert', updated)],
      {} as any
    );
    expect([...cache.team.filter('green')]).toEqual([updated]);
    cache.applyOperations([op('delete', updated)], {} as any);
    expect(cache.id.find(1)).toBe(updated);
    cache.applyOperations([op('delete', updated)], {} as any);
    expect(cache.id.find(1)).toBeNull();
    expect([...cache.team.filter('green')]).toEqual([]);
  });

  it('handles unique-key swaps and reuse within a batch', () => {
    const cache = populatedCache();
    const first = { ...rows[0], name: rows[1].name };
    const second = { ...rows[1], name: rows[0].name };
    cache.applyOperations(
      [
        op('delete', rows[0]),
        op('delete', rows[1]),
        op('insert', first),
        op('insert', second),
      ],
      {} as any
    );
    expect(cache.name.find('two')).toBe(first);
    expect(cache.name.find('one')).toBe(second);
    const replacement = { ...second, id: 5 };
    cache.applyOperations(
      [op('delete', second), op('insert', replacement)],
      {} as any
    );
    expect(cache.name.find('one')).toBe(replacement);
    expect(cache.id.find(2)).toBeNull();
  });

  it('preserves range bounds and uses equality prefixes to narrow range scans', () => {
    const cache = populatedCache();
    expect([...cache.byTeamLevel.filter(new Range())]).toEqual(rows);
    expect([
      ...cache.byTeamLevel.filter(
        new Range(
          { tag: 'included', value: 'red' },
          { tag: 'included', value: 'red' }
        )
      ),
    ]).toEqual([rows[0], rows[2], rows[3]]);
    vi.spyOn(cache, 'iter').mockImplementation(() => {
      throw new Error(
        'A range with an equality prefix must not scan the cache'
      );
    });
    expect([
      ...cache.byTeamLevel.filter([
        'red',
        new Range(
          { tag: 'included', value: 1n },
          { tag: 'excluded', value: 3n }
        ),
      ]),
    ]).toEqual([rows[0], rows[2]]);
    expect([
      ...cache.byTeamLevel.filter([
        'red',
        new Range(
          { tag: 'excluded', value: 1n },
          { tag: 'included', value: 3n }
        ),
      ]),
    ]).toEqual([rows[3]]);
    expect([...cache.byTeamLevel.filter(['missing', new Range()])]).toEqual([]);
  });

  it.each([false, true])(
    'uses serialized prefix equality for range filters (stored undefined: %s)',
    explicitUndefined => {
      const grouped = table(
        {
          name: 'grouped',
          indexes: [
            {
              accessor: 'byGroupLevel',
              algorithm: 'btree',
              columns: ['group', 'level'],
            },
          ],
        },
        {
          id: t.u32().primaryKey(),
          group: t.object('Group', {
            name: t.string(),
            optional: t.option(t.string()),
          }),
          level: t.u32(),
        }
      );
      const cache = cacheFor(
        tablesToSchema(new ModuleContext(), { grouped }).tables.grouped
      );
      const absent = { name: 'x' };
      const present = { name: 'x', optional: undefined };
      const group = explicitUndefined ? present : absent;
      // JavaScript callers can omit a property typed as `string | undefined`.
      const query = (explicitUndefined ? absent : present) as typeof present;
      const matching = [0, 1, 2, 3].map(level => ({ id: level, group, level }));
      cache.applyOperations(
        [...matching, { id: 4, group: { name: 'other' }, level: 2 }].map(
          row => ({ type: 'insert', rowId: row.id, row })
        ),
        {} as any
      );
      vi.spyOn(cache, 'iter').mockImplementation(() => {
        throw new Error(
          'A range with an equality prefix must not scan the cache'
        );
      });
      expect([...cache.byGroupLevel.filter(query)]).toEqual(matching);
      expect([...cache.byGroupLevel.filter([query, new Range()])]).toEqual(
        matching
      );
      expect([
        ...cache.byGroupLevel.filter([
          query,
          new Range(
            { tag: 'included', value: 1 },
            { tag: 'excluded', value: 3 }
          ),
        ]),
      ]).toEqual([matching[1], matching[2]]);
    }
  );

  it('keeps tuple boundaries distinct for delimiter-containing string keys', () => {
    const cache = cacheFor(itemDef);
    const first = { id: 1, name: 'b,c', team: 'a', level: 1n };
    const second = { id: 2, name: 'c', team: 'a,b', level: 1n };
    cache.applyOperations(
      [op('insert', first), op('insert', second)],
      {} as any
    );
    expect([...cache.byTeamName.filter(['a', 'b,c'])]).toEqual([first]);
    expect([...cache.byTeamName.filter(['a,b', 'c'])]).toEqual([second]);
  });

  it('supports every equality prefix of a three-column index', () => {
    const triple = table(
      {
        name: 'triple',
        indexes: [
          {
            accessor: 'byTeamLevelName',
            algorithm: 'btree',
            columns: ['team', 'level', 'name'],
          },
        ],
      },
      {
        id: t.u32().primaryKey(),
        team: t.string(),
        level: t.u64(),
        name: t.string(),
      }
    );
    const cache = cacheFor(
      tablesToSchema(new ModuleContext(), { triple }).tables.triple
    );
    const index = cache.byTeamLevelName;
    cache.applyOperations(
      rows.map(row => op('insert', row)),
      {} as any
    );
    expect([...index.filter('red')]).toEqual([rows[0], rows[2], rows[3]]);
    expect([...index.filter(['red', 1n])]).toEqual([rows[0], rows[2]]);
    expect([...index.filter(['red', 1n, 'one'])]).toEqual([rows[0]]);
    cache.applyOperations([op('delete', rows[0])], {} as any);
    expect([...index.filter(['red', 1n])]).toEqual([rows[2]]);
    expect([...index.filter(['red', 1n, 'one'])]).toEqual([]);
  });

  it('removes deleted rows from active composite filter iterators', () => {
    const cache = cacheFor(itemDef);
    cache.applyOperations(
      [op('insert', rows[0]), op('insert', rows[2])],
      {} as any
    );
    const iterator = cache.byTeamLevel.filter(['red', 1n]);
    expect(iterator.next().value).toBe(rows[0]);
    cache.applyOperations(
      [op('delete', rows[0]), op('delete', rows[2])],
      {} as any
    );
    expect(iterator.next().done).toBe(true);
  });

  it('maintains indexes on tables without primary keys', () => {
    const log = table(
      { name: 'log' },
      { message: t.string().index('btree'), value: t.u32() }
    );
    const cache = cacheFor(
      tablesToSchema(new ModuleContext(), { log }).tables.log
    );
    const first = { message: 'same', value: 1 };
    const second = { message: 'same', value: 2 };
    const insert = (rowId: string, row: typeof first): Operation => ({
      type: 'insert',
      rowId,
      row,
    });
    cache.applyOperations(
      [insert('a', first), insert('a', { ...first }), insert('b', second)],
      {} as any
    );
    expect([...cache.message.filter('same')]).toEqual([first, second]);
    cache.applyOperations(
      [{ type: 'delete', rowId: 'a', row: first }],
      {} as any
    );
    expect([...cache.message.filter('same')]).toEqual([first, second]);
    cache.applyOperations(
      [{ type: 'delete', rowId: 'a', row: first }],
      {} as any
    );
    expect([...cache.message.filter('same')]).toEqual([second]);
  });

  it('does not retain event rows in indexes', () => {
    const cache = cacheFor({ ...itemDef, isEvent: true });
    const callbacks = cache.applyOperations([op('insert', rows[0])], {} as any);
    expect(callbacks).toHaveLength(1);
    expect(cache.id.find(1)).toBeNull();
    expect([...cache.team.filter('red')]).toEqual([]);
  });

  it('optimizes existing generated bindings, including duplicate index accessors', () => {
    const cache = cacheFor(tables.player.tableDef);
    const player = {
      id: 1,
      userId: Identity.zero(),
      name: 'player',
      location: { x: 1, y: 2 },
    };
    cache.applyOperations(
      [{ type: 'insert', rowId: 1, row: player }],
      {} as any
    );
    vi.spyOn(cache, 'iter').mockImplementation(() => {
      throw new Error('Generated index accessors must not scan');
    });
    expect(cache.id.find(1)).toBe(player);
    cache.applyOperations(
      [{ type: 'delete', rowId: 1, row: player }],
      {} as any
    );
    expect(cache.id.find(1)).toBeNull();
  });
});

describe('index key value equality', () => {
  it.each([
    ['zero', t.u32(), () => 0],
    ['false', t.bool(), () => false],
    ['empty string', t.string(), () => ''],
    ['large integer', t.u128(), () => (1n << 100n) + 1n],
    ['enum', t.enum('Status', ['Ready', 'Waiting']), () => ({ tag: 'Ready' })],
    ['identity', t.identity(), () => new Identity(123n)],
    ['connection ID', t.connectionId(), () => new ConnectionId(123n)],
    ['timestamp', t.timestamp(), () => new Timestamp(123n)],
    ['UUID', t.uuid(), () => new Uuid(123n)],
    [
      'product',
      t.object('Pair', { a: t.string(), b: t.u64() }),
      () => ({ b: 123n, a: 'value' }),
    ],
    ['option', t.option(t.u64()), () => 123n],
    ['absent option', t.option(t.u64()), () => undefined],
    ['byte array', t.array(t.u8()), () => new Uint8Array([1, 2, 3])],
  ] as [string, TypeBuilder<any, any>, () => any][])(
    'compares %s keys by value',
    (_name, type, value) => {
      const keyed = table(
        {
          name: 'keyed',
          indexes: [
            {
              accessor: 'uniqueKey',
              algorithm: 'btree',
              columns: ['uniqueKey'],
            },
            {
              accessor: 'sharedKey',
              algorithm: 'btree',
              columns: ['sharedKey'],
            },
          ],
          constraints: [
            {
              name: 'unique_key',
              constraint: 'unique',
              columns: ['uniqueKey'],
            },
          ],
        },
        {
          id: t.u32().primaryKey(),
          uniqueKey: type,
          sharedKey: type,
        }
      );
      const cache = cacheFor(
        tablesToSchema(new ModuleContext(), { keyed }).tables.keyed
      );
      const row = { id: 1, uniqueKey: value(), sharedKey: value() };
      // Explicit constraints determine uniqueness at runtime for these types.
      const uniqueIndex = (cache as any).uniqueKey;
      cache.applyOperations([{ type: 'insert', rowId: 1, row }], {} as any);
      expect(uniqueIndex.find(value())).toBe(row);
      expect([...cache.sharedKey.filter(value())]).toEqual([row]);
      cache.applyOperations(
        [
          {
            type: 'delete',
            rowId: 1,
            row: { ...row, uniqueKey: value(), sharedKey: value() },
          },
        ],
        {} as any
      );
      expect(uniqueIndex.find(value())).toBeNull();
      expect([...cache.sharedKey.filter(value())]).toEqual([]);
    }
  );
});
