import { describe, expect, it } from 'vitest';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { TableCacheImpl } from '../src/sdk/table_cache';
import { t } from '../src/lib/type_builders';

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
});
