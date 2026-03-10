import { describe, expect, it } from 'vitest';
import { ModuleContext } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';

describe('table index accessors', () => {
  it('throws when an explicit index is missing accessor', () => {
    expect(() =>
      table(
        {
          name: 'person',
          indexes: [
            {
              name: 'id_idx',
              algorithm: 'btree',
              columns: ['id'] as const,
            } as any,
          ] as const,
        },
        {
          id: t.identity(),
        }
      )
    ).toThrowError("must define a non-empty 'accessor'");
  });

  it('allows duplicate explicit index accessors in raw table definitions', () => {
    /**
     * table() does not reject duplicate explicit accessor names at definition
     * time. Runtime table view construction handles duplicate accessors by
     * merging methods onto a single accessor object.
     */
    const tableSchema = table(
      {
        name: 'person',
        indexes: [
          {
            accessor: 'dup',
            algorithm: 'btree',
            columns: ['id'] as const,
          },
          {
            accessor: 'dup',
            algorithm: 'hash',
            columns: ['email'] as const,
          },
        ] as const,
      },
      {
        id: t.identity(),
        email: t.string(),
      }
    );

    const rawTableDef = tableSchema.tableDef(new ModuleContext(), 'person');
    expect(rawTableDef.indexes).toHaveLength(2);
    expect(rawTableDef.indexes.map(index => index.accessorName)).toEqual([
      'dup',
      'dup',
    ]);
  });

  it('accepts an explicit accessor for table-level indexes', () => {
    const tableSchema = table(
      {
        name: 'person',
        indexes: [
          {
            accessor: 'byName',
            algorithm: 'btree',
            columns: ['name'] as const,
          },
        ] as const,
      },
      {
        name: t.string(),
      }
    );

    const rawTableDef = tableSchema.tableDef(new ModuleContext(), 'person');
    expect(rawTableDef.indexes).toHaveLength(1);
    expect(rawTableDef.indexes[0].accessorName).toBe('byName');
  });

  it('derives accessor from the field name for field-level indexes', () => {
    const tableSchema = table(
      {
        name: 'person',
      },
      {
        displayName: t.string().index('btree'),
      }
    );

    const rawTableDef = tableSchema.tableDef(new ModuleContext(), 'person');
    expect(rawTableDef.indexes).toHaveLength(1);
    expect(rawTableDef.indexes[0].accessorName).toBe('displayName');
  });

  it('keeps both implicit and explicit index entries when accessors match', () => {
    /**
     * Generated client bindings can include an explicit table-level index entry
     * for the same logical index that is already inferred from field metadata
     * (e.g. primaryKey implies an implicit index). This should not fail as a
     * duplicate accessor when the definitions are equivalent.
     */
    const tableSchema = table(
      {
        name: 'player',
        indexes: [
          {
            accessor: 'id',
            name: 'player_id_idx_btree',
            algorithm: 'btree',
            columns: ['id'] as const,
          },
        ] as const,
      },
      {
        id: t.u32().primaryKey(),
      }
    );

    const ctx = new ModuleContext();
    const rawTableDef = tableSchema.tableDef(ctx, 'player');

    // Raw schema keeps both entries; runtime accessor construction merges by
    // accessor name.
    expect(rawTableDef.indexes).toHaveLength(2);
    expect(rawTableDef.indexes.map(index => index.accessorName)).toEqual([
      'id',
      'id',
    ]);

    // The explicit canonical name should still be preserved in explicit names.
    expect(ctx.moduleDef.explicitNames.entries).toEqual([
      {
        tag: 'Index',
        value: {
          sourceName: 'player_id_idx_btree',
          canonicalName: 'player_id_idx_btree',
        },
      },
    ]);
  });
});
