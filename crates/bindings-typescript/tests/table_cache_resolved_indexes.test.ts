import { describe, expect, it } from 'vitest';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { TableCacheImpl } from '../src/sdk/table_cache';
import { t } from '../src/lib/type_builders';
import { Range } from '../src/server/range';

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
      {}
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
});
