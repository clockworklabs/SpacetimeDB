import { describe, expect, it } from 'vitest';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';
import { schema } from '../src/server/schema';

const buildSession = table(
  {
    name: 'build_session',
    indexes: [
      {
        accessor: 'byOrganizationAndState',
        algorithm: 'btree',
        columns: ['organizationId', 'state'] as const,
      },
      {
        accessor: 'byState',
        name: 'custom_build_session_state_idx',
        algorithm: 'hash',
        columns: ['state'] as const,
      },
    ] as const,
  },
  {
    id: t.u64().primaryKey(),
    externalId: t.string().unique(),
    organizationId: t.string().index('btree'),
    requestHash: t.string().index('hash'),
    slot: t.u64().index('direct'),
    state: t.string(),
  }
);

const withoutCanonicalName = table(
  {},
  {
    id: t.u64().primaryKey(),
  }
);

describe('canonical generated index naming', () => {
  it('uses the canonical table name for every generated index source name', () => {
    const module = schema({ buildSession, withoutCanonicalName });
    const buildSessionDef = module.schemaType.tables.buildSession;
    const generatedNames = buildSessionDef.tableDef.indexes.map(
      index => index.sourceName
    );

    expect(buildSessionDef.accessorName).toBe('buildSession');
    expect(buildSessionDef.sourceName).toBe('build_session');
    expect(generatedNames).toEqual([
      'build_session_id_idx_btree',
      'build_session_externalId_idx_btree',
      'build_session_organizationId_idx_btree',
      'build_session_requestHash_idx_hash',
      'build_session_slot_idx_direct',
      'build_session_organizationId_state_idx_btree',
      'build_session_state_idx_hash',
    ]);
    expect(
      generatedNames.every(name => name?.startsWith('build_session_'))
    ).toBe(true);

    expect(module.moduleDef.explicitNames.entries).toContainEqual({
      tag: 'Index',
      value: {
        sourceName: 'build_session_state_idx_hash',
        canonicalName: 'custom_build_session_state_idx',
      },
    });
    expect(module.moduleDef.explicitNames.entries).toContainEqual({
      tag: 'Table',
      value: {
        sourceName: 'buildSession',
        canonicalName: 'build_session',
      },
    });

    expect(
      module.schemaType.tables.withoutCanonicalName.tableDef.indexes.map(
        index => index.sourceName
      )
    ).toEqual(['withoutCanonicalName_id_idx_btree']);
  });

  it('constructs identical index metadata repeatedly', () => {
    const first = schema({ buildSession });
    const second = schema({ buildSession });

    expect(second.schemaType.tables.buildSession.tableDef.indexes).toEqual(
      first.schemaType.tables.buildSession.tableDef.indexes
    );
    expect(second.moduleDef.explicitNames.entries).toEqual(
      first.moduleDef.explicitNames.entries
    );
  });
});
