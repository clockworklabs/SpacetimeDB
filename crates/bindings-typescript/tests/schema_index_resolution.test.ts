import { describe, expect, it } from 'vitest';
import { ModuleContext, tablesToSchema } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';

describe('schema index resolution', () => {
  it('keeps declarative index options separate from resolved runtime indexes', () => {
    /**
     * Why this test exists:
     * We intentionally model two different index representations:
     * 1) `indexes`: declarative table-level options as authored by the user
     * 2) `resolvedIndexes`: runtime-ready index metadata derived from RawTableDef
     *
     * The rewrite is correct only if:
     * - `indexes` still preserves the original declaration shape for type inference.
     * - `resolvedIndexes` includes every runtime index (field-level + table-level).
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
        // Field-level index declared on metadata.
        displayName: t.string().index('hash'),
        team: t.string(),
        level: t.u32(),
      }
    );

    const schemaDef = tablesToSchema(new ModuleContext(), { player });
    const playerDef = schemaDef.tables.player;

    /**
     * `indexes` should remain the declarative shape from `table({ indexes: ... })`.
     * This drives type-level behavior, so it must not be replaced with resolved
     * runtime metadata.
     */
    expect(playerDef.indexes).toEqual([
      {
        accessor: 'byTeamAndLevel',
        algorithm: 'btree',
        columns: ['team', 'level'],
      },
    ]);

    /**
     * `resolvedIndexes` should contain:
     * - the field-level index derived from `displayName.index('hash')`, and
     * - the explicit table-level index `byTeamAndLevel`.
     *
     * Runtime consumers (e.g. TableCache) use this field because it is the
     * normalized shape with resolved algorithms, names, and column lists.
     */
    expect(playerDef.resolvedIndexes).toEqual(
      expect.arrayContaining([
        {
          name: 'displayName',
          unique: false,
          algorithm: 'hash',
          columns: ['displayName'],
        },
        {
          name: 'byTeamAndLevel',
          unique: false,
          algorithm: 'btree',
          columns: ['team', 'level'],
        },
      ])
    );
  });
});
