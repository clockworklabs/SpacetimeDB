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

  it('throws when explicit indexes reuse an accessor', () => {
    expect(() =>
      table(
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
      )
    ).toThrowError("Duplicate index accessor 'dup'");
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
});
