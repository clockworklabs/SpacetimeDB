import { describe, expect, it, vi } from 'vitest';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { TableCacheImpl } from '../src/sdk/table_cache';
import { t } from '../src/lib/type_builders';
import { Range } from '../src/server/range';
import { setGlobalLogLevel, getGlobalLogLevel } from '../src/sdk/logger';

describe('table cache resolved indexes', () => {
  it('builds index accessors from resolvedIndexes (field-level + table-level)', () => {
    /**
     * Why this test exists:
     * `TableCacheImpl` previously consumed `table.indexes` and re-cast it into a
     * runtime shape. After the refactor, runtime index construction must come
     * from `table.resolvedIndexes` instead.
     *
     * This test validates the observable contract:
     * - field-level indexes still materialize as cache accessors
     * - explicit table-level indexes still materialize as cache accessors
     * - both accessors execute correctly against cached rows
     */
    const player = table(
      {
        name: 'player',
        indexes: [
          {
            accessor: 'byTeamAndLevel',
            algorithm: 'btree',
            columns: ['team', 'level'] as const,
          },
        ] as const,
      },
      {
        // Field-level index.
        email: t.string().index('btree'),
        team: t.string(),
        level: t.u32(),
      }
    );

    const schemaDef = tablesToSchema(new ModuleContext(), { player });
    const playerDef = schemaDef.tables.player;
    const tableCache = new TableCacheImpl<any, string>(playerDef as any);

    const rows = [
      { email: 'a@example.com', team: 'red', level: 1 },
      { email: 'b@example.com', team: 'blue', level: 2 },
      { email: 'c@example.com', team: 'red', level: 3 },
    ];

    const callbacks = tableCache.applyOperations(
      rows.map(row => ({
        type: 'insert' as const,
        // This table has no primary key, so any stable row id is acceptable for this test.
        rowId: row.email,
        row,
      })),
      {
        db: {},
        reducers: {},
        isActive: true,
        subscriptionBuilder: () => {
          throw new Error('Not used by this test');
        },
        disconnect: () => {},
        event: { id: 'test', tag: 'SubscribeApplied' },
      }
    );
    callbacks.forEach(cb => cb.cb());

    const emailIndex = (tableCache as any).email;
    const byTeamAndLevel = (tableCache as any).byTeamAndLevel;

    // The field-level accessor must exist and support point lookup semantics.
    expect(typeof emailIndex?.filter).toBe('function');
    expect(Array.from(emailIndex.filter('a@example.com'))).toEqual([rows[0]]);

    // The explicit table-level accessor must exist and support tuple filtering.
    expect(typeof byTeamAndLevel?.filter).toBe('function');
    expect(Array.from(byTeamAndLevel.filter(['red', 1]))).toEqual([rows[0]]);
  });

  it('btree filter supports prefixes and ranges and tracks row changes', () => {
    const player = table(
      {
        name: 'player',
        indexes: [
          {
            accessor: 'byTeamAndLevel',
            algorithm: 'btree',
            columns: ['team', 'level'] as const,
          },
        ] as const,
      },
      {
        id: t.u32(),
        team: t.string(),
        level: t.u32(),
      }
    );

    const schemaDef = tablesToSchema(new ModuleContext(), { player });
    const tableCache = new TableCacheImpl<any, string>(
      schemaDef.tables.player as any
    );
    const byTeamAndLevel = (tableCache as any).byTeamAndLevel;
    const apply = (type: 'insert' | 'delete', rows: any[]) =>
      tableCache.applyOperations(
        rows.map(row => ({ type, rowId: row.id, row })),
        {} as any
      );
    const filter = (range: any) => Array.from(byTeamAndLevel.filter(range));

    const rows = [
      { id: 1, team: 'red', level: 1 },
      { id: 2, team: 'blue', level: 2 },
      { id: 3, team: 'red', level: 3 },
      { id: 4, team: 'red', level: 5 },
    ];
    apply('insert', rows);

    expect(filter(['red', 3])).toEqual([rows[2]]);
    expect(filter(['green', 3])).toEqual([]);
    expect(filter('red')).toEqual([rows[0], rows[2], rows[3]]);
    expect(
      filter([
        'red',
        new Range({ tag: 'excluded', value: 1 }, { tag: 'included', value: 5 }),
      ])
    ).toEqual([rows[2], rows[3]]);
    expect(
      filter(
        new Range(
          { tag: 'included', value: 'blue' },
          { tag: 'excluded', value: 'red' }
        )
      )
    ).toEqual([rows[1]]);

    const moved = { id: 3, team: 'blue', level: 3 };
    apply('delete', [rows[2]]);
    apply('insert', [moved]);
    apply('delete', [rows[3]]);

    expect(filter(['red', 3])).toEqual([]);
    expect(filter('red')).toEqual([rows[0]]);
    expect(filter('blue')).toEqual([rows[1], moved]);
  });

  // Indexes are built on their first lookup.
  const makeUser = () => {
    const user = table(
      {
        name: 'user',
        indexes: [
          {
            accessor: 'byTeamAndLevel',
            algorithm: 'btree',
            columns: ['team', 'level'] as const,
          },
        ] as const,
      },
      {
        id: t.u32().primaryKey(),
        email: t.string().unique(),
        team: t.string().index('btree'),
        level: t.u32(),
      }
    );
    const def = tablesToSchema(new ModuleContext(), { user }).tables.user;
    const cache = new TableCacheImpl<any, string>(def as any);
    const tx = (ops: [type: 'insert' | 'delete', row: any][]) =>
      cache.applyOperations(
        ops.map(([type, row]) => ({ type, rowId: row.id, row })),
        {} as any
      );
    return { cache: cache as any, tx };
  };

  it('builds an index on its first lookup only, and logs the build', () => {
    const { cache, tx } = makeUser();
    const level = getGlobalLogLevel();
    setGlobalLogLevel('debug');
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    try {
      const builds = () =>
        log.mock.calls.filter(c => String(c[0]).includes('built index')).length;
      tx(
        [1, 2, 3].map(i => [
          'insert',
          { id: i, email: `e${i}`, team: 't', level: i },
        ])
      );
      expect(builds()).toBe(0);

      expect(cache.email.find('e2')).toEqual({
        id: 2,
        email: 'e2',
        team: 't',
        level: 2,
      });
      expect(builds()).toBe(1);
      expect(String(log.mock.calls.at(-1)![0])).toContain('built index user.');
      cache.email.find('e3');
      expect(builds()).toBe(1);

      // A range on the first column scans the rows; it needs no index.
      Array.from(
        cache.team.filter(
          new Range({ tag: 'included', value: 'a' }, { tag: 'unbounded' })
        )
      );
      expect(builds()).toBe(1);
      Array.from(cache.team.filter('t'));
      expect(builds()).toBe(2);
    } finally {
      log.mockRestore();
      setGlobalLogLevel(level);
    }
  });

  it('find and filter return the same rows as a scan, before and after changes', () => {
    const { cache, tx } = makeUser();
    let seed = 7;
    const rand = (n: number) => (seed = (seed * 48271) % 2147483647) % n;
    const live = new Map<number, any>();
    const randomRow = (id: number) => ({
      id,
      email: `e${rand(60)}`,
      team: `t${rand(4)}`,
      level: rand(5),
    });
    const scan = (pred: (r: any) => boolean) =>
      [...live.values()].filter(pred).sort((x, y) => x.id - y.id);
    const sorted = (rows: Iterable<any>) =>
      [...rows].sort((x, y) => x.id - y.id);
    const check = () => {
      for (let e = 0; e < 60; e++) {
        expect(cache.email.find(`e${e}`)).toEqual(
          scan(r => r.email === `e${e}`)[0] ?? null
        );
      }
      for (let tm = 0; tm < 4; tm++) {
        expect(sorted(cache.team.filter(`t${tm}`))).toEqual(
          scan(r => r.team === `t${tm}`)
        );
        for (let l = 0; l < 5; l++) {
          expect(sorted(cache.byTeamAndLevel.filter([`t${tm}`, l]))).toEqual(
            scan(r => r.team === `t${tm}` && r.level === l)
          );
        }
        const range = new Range(
          { tag: 'excluded', value: 1 },
          { tag: 'included', value: 3 }
        );
        expect(sorted(cache.byTeamAndLevel.filter([`t${tm}`, range]))).toEqual(
          scan(r => r.team === `t${tm}` && r.level > 1 && r.level <= 3)
        );
      }
    };

    // Unique emails: each step deletes or updates rows and inserts new ones.
    const used = new Set<string>();
    const fresh = (id: number) => {
      let r = randomRow(id);
      while (used.has(r.email)) r = randomRow(id);
      used.add(r.email);
      return r;
    };
    const ops: [any, any][] = [];
    for (let id = 0; id < 30; id++) {
      const r = fresh(id);
      live.set(id, r);
      ops.push(['insert', r]);
    }
    tx(ops);
    check();
    let nextId = 30;
    for (let step = 0; step < 20; step++) {
      const ops: [any, any][] = [];
      for (const [id, r] of [...live]) {
        const k = rand(4);
        if (k === 0) {
          ops.push(['delete', r]);
          live.delete(id);
          used.delete(r.email);
        } else if (k === 1) {
          const n = { ...fresh(id) };
          used.delete(r.email);
          ops.push(['delete', r], ['insert', n]);
          live.set(id, n);
        }
      }
      for (let i = 0; i < 3 && used.size < 60; i++) {
        const n = fresh(nextId);
        live.set(nextId++, n);
        ops.push(['insert', n]);
      }
      tx(ops);
      check();
    }
  });
});
