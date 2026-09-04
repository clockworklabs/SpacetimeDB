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

  it('treats null and undefined as option none in btree cache filters', () => {
    const account = table(
      {
        name: 'account',
        indexes: [
          {
            accessor: 'linkedId',
            algorithm: 'btree',
            columns: ['linkedId'] as const,
          },
        ] as const,
      },
      {
        id: t.u32(),
        linkedId: t.option(t.u32()).index('btree'),
        uniqueLinkedId: t.option(t.u32()).unique(),
      }
    );

    const schemaDef = tablesToSchema(new ModuleContext(), { account });
    const accountDef = schemaDef.tables.account;
    const tableCache = new TableCacheImpl<any, string>(accountDef as any);

    const rows = [
      { id: 1, linkedId: undefined, uniqueLinkedId: undefined },
      { id: 2, linkedId: null, uniqueLinkedId: 7 },
      { id: 3, linkedId: 5, uniqueLinkedId: 8 },
      { id: 4, linkedId: 9, uniqueLinkedId: 9 },
    ];

    const callbacks = tableCache.applyOperations(
      rows.map(row => ({
        type: 'insert' as const,
        rowId: row.id,
        row,
      })),
      {}
    );
    callbacks.forEach(cb => cb.cb());

    const linkedId = (tableCache as any).linkedId;
    const uniqueLinkedId = (tableCache as any).uniqueLinkedId;

    expect(uniqueLinkedId.find(null)?.id).toEqual(1);
    expect(Array.from(linkedId.filter(null)).map(row => row.id)).toEqual([
      1, 2,
    ]);
    expect(Array.from(linkedId.filter(5)).map(row => row.id)).toEqual([3]);
    expect(
      Array.from(
        linkedId.filter(
          new Range(
            { tag: 'included', value: null },
            { tag: 'included', value: 5 }
          )
        )
      ).map(row => row.id)
    ).toEqual([1, 2, 3]);
  });
});
