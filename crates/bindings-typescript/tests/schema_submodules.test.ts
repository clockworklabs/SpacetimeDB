import { beforeAll, describe, expect, it, vi } from 'vitest';

vi.mock(
  'spacetime:sys@2.0',
  () => ({
    moduleHooks: Symbol('moduleHooks'),
  }),
  { virtual: true }
);

vi.mock('../src/server/runtime', () => ({
  makeHooks: () => ({}),
  callProcedure: () => new Uint8Array(),
  callUserFunction: (fn: (...args: any[]) => any, ...args: any[]) =>
    fn(...args),
  ReducerCtxImpl: class {},
  sys: {
    row_iter_bsatn_close: () => {},
  },
}));

describe('schema submodules', () => {
  let schema: typeof import('../src/server/schema').schema;
  let table: typeof import('../src/lib/table').table;
  let t: typeof import('../src/lib/type_builders').t;

  beforeAll(async () => {
    ({ schema } = await import('../src/server/schema'));
    ({ table } = await import('../src/lib/table'));
    ({ t } = await import('../src/lib/type_builders'));
  });

  it('emits submodule module defs and resolves submodule schedules', () => {
    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });

    const sessionCleanupTick = table(
      {
        name: 'session_cleanup_tick',
        scheduled: (): any => cleanExpiredSessions,
      },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
      }
    );

    const sessions = table(
      { name: 'sessions' },
      {
        id: t.u64().primaryKey().autoInc(),
      }
    );

    const authSchema = schema({
      sessions,
      sessionCleanupTick,
    });

    const cleanExpiredSessions = authSchema.reducer(() => {});
    const authLib = {
      default: authSchema,
      cleanExpiredSessions,
    };

    const consumer = schema({
      players,
      myauth: authLib,
    });

    const raw = consumer.buildRawModuleDefV10({});
    const submodules = raw.sections.find(
      section => section.tag === 'Submodules'
    )?.value;

    expect(submodules).toHaveLength(1);
    expect(submodules?.[0]?.namespace).toBe('myauth');

    const submoduleSections = submodules?.[0]?.module.sections ?? [];
    const submoduleReducers = submoduleSections.find(
      section => section.tag === 'Reducers'
    )?.value;
    const submoduleSchedules = submoduleSections.find(
      section => section.tag === 'Schedules'
    )?.value;

    expect(submoduleReducers).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ sourceName: 'cleanExpiredSessions' }),
      ])
    );
    expect(submoduleSchedules).toEqual([
      expect.objectContaining({
        tableName: 'sessionCleanupTick',
        functionName: 'cleanExpiredSessions',
      }),
    ]);
  });

  it('sends the accessor key as the source namespace and lets the host derive the canonical name', () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions });
    const authLib = { default: authSchema };

    const consumer = schema({ myAuth: authLib });
    const raw = consumer.buildRawModuleDefV10({});

    // The key is the accessor namespace. It is sent verbatim as the source name,
    // and the host applies the case conversion policy (`myAuth` -> `my_auth`),
    // so no explicit namespace mapping is emitted here.
    const submodules = raw.sections.find(s => s.tag === 'Submodules')?.value;
    expect(submodules?.map(s => s.namespace)).toEqual(['myAuth']);
    const explicitNames =
      raw.sections.find(s => s.tag === 'ExplicitNames')?.value.entries ?? [];
    expect(explicitNames.filter(e => e.tag === 'Namespace')).toEqual([]);

    // The runtime keeps addressing the submodule by its accessor name.
    expect(consumer.submoduleDispatchInfos[0].namespace).toBe('myAuth');
  });

  it('pins the canonical namespace with a `{ name, module }` mount', () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions });
    const cleanExpiredSessions = authSchema.reducer(() => {});
    const authLib = { default: authSchema, cleanExpiredSessions };

    const consumer = schema({
      myAuth: { name: 'myAuth', module: authLib },
    });
    const raw = consumer.buildRawModuleDefV10({});

    const submodules = raw.sections.find(s => s.tag === 'Submodules')?.value;
    expect(submodules?.map(s => s.namespace)).toEqual(['myAuth']);

    const explicitNames =
      raw.sections.find(s => s.tag === 'ExplicitNames')?.value.entries ?? [];
    expect(explicitNames.filter(e => e.tag === 'Namespace')).toEqual([
      {
        tag: 'Namespace',
        value: { sourceName: 'myAuth', canonicalName: 'myAuth' },
      },
    ]);

    // The mounted module's exports are still registered through the wrapper.
    const info = consumer.submoduleDispatchInfos[0];
    expect(info.namespace).toBe('myAuth');
    expect(info.reducerDefs.map(r => r.sourceName)).toEqual([
      'cleanExpiredSessions',
    ]);
    expect(info.tables[0].accessorName).toBe('sessions');
  });

  it('accepts a `{ module }` mount without a name', () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions });
    const authLib = { default: authSchema };

    const consumer = schema({ myAuth: { module: authLib } });
    const raw = consumer.buildRawModuleDefV10({});

    const submodules = raw.sections.find(s => s.tag === 'Submodules')?.value;
    expect(submodules?.map(s => s.namespace)).toEqual(['myAuth']);
    const explicitNames =
      raw.sections.find(s => s.tag === 'ExplicitNames')?.value.entries ?? [];
    expect(explicitNames.filter(e => e.tag === 'Namespace')).toEqual([]);
  });

  it('rejects a named mount whose module is a default import', () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions });

    expect(() =>
      schema({
        myAuth: { name: 'myAuth', module: authSchema as any },
      })
    ).toThrow(/looks like a default import/);
  });

  it('rejects a named mount with an empty name', () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions });
    const authLib = { default: authSchema };

    expect(() =>
      schema({
        myAuth: { name: '', module: authLib },
      })
    ).toThrow(/invalid `name`/);
  });

  it('rejects default-import style submodules with a clear error', () => {
    const sessions = table(
      { name: 'sessions' },
      {
        id: t.u64().primaryKey().autoInc(),
      }
    );

    const authSchema = schema({ sessions });

    expect(() =>
      schema({
        myauth: authSchema as any,
      })
    ).toThrow(/looks like a default import/);
  });

  it('populates submoduleDispatchInfos with reducer fns and table metadata', () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );

    const authSchema = schema({ sessions });
    const cleanExpiredSessions = authSchema.reducer(() => {});
    const authLib = { default: authSchema, cleanExpiredSessions };

    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });
    const consumer = schema({ players, myauth: authLib });

    const infos = consumer.submoduleDispatchInfos;
    expect(infos).toHaveLength(1);

    const info = infos[0];
    expect(info.reducerFns).toHaveLength(1);
    expect(info.reducerDefs).toHaveLength(1);
    expect(info.reducerDefs[0].sourceName).toBe('cleanExpiredSessions');
    expect(info.tables).toHaveLength(1);
    expect(info.tables[0].accessorName).toBe('sessions');
    expect(info.subDispatches).toHaveLength(0);
  });

  it('flattens nested submodule dispatches depth-first', () => {
    // baz library: 1 reducer
    const bazTable = table({ name: 'baz_items' }, { id: t.u32().primaryKey() });
    const bazSchema = schema({ bazTable });
    const bazReducer = bazSchema.reducer(() => {});
    const bazLib = { default: bazSchema, bazReducer };

    // auth library: 1 own reducer, uses baz as a submodule
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions, baz: bazLib });
    const authReducer = authSchema.reducer(() => {});
    const authLib = { default: authSchema, authReducer };

    // consumer: 1 own reducer, uses auth as a submodule
    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });
    const consumer = schema({ players, myauth: authLib });
    const consumerReducer = consumer.reducer(() => {});

    // Verify depth-first structure:
    // consumer.submoduleDispatchInfos[0] = myauth (authReducer)
    // consumer.submoduleDispatchInfos[0].subDispatches[0] = myauth.baz (bazReducer)
    const infos = consumer.submoduleDispatchInfos;
    expect(infos).toHaveLength(1);

    const authInfo = infos[0];
    expect(authInfo.reducerFns).toHaveLength(1);
    expect(authInfo.reducerDefs[0].sourceName).toBe('authReducer');
    expect(authInfo.subDispatches).toHaveLength(1);

    const bazInfo = authInfo.subDispatches[0];
    expect(bazInfo.reducerFns).toHaveLength(1);
    expect(bazInfo.reducerDefs[0].sourceName).toBe('bazReducer');
    expect(bazInfo.subDispatches).toHaveLength(0);

    // Unused variable check
    void consumerReducer;
  });

  it('submoduleDispatchInfos carry namespace and nested namespace dispatches propagate', () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions });
    const authLib = { default: authSchema };

    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });
    const consumer = schema({ players, myauth: authLib });

    const infos = consumer.submoduleDispatchInfos;
    expect(infos).toHaveLength(1);
    expect(infos[0].namespace).toBe('myauth');
    expect(infos[0].tables[0].accessorName).toBe('sessions');
  });

  it('nested submodules carry their own namespace on subDispatches', () => {
    const bazTable = table({ name: 'baz_items' }, { id: t.u32().primaryKey() });
    const bazSchema = schema({ bazTable });
    const bazLib = { default: bazSchema };

    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions, baz: bazLib });
    const authLib = { default: authSchema };

    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });
    const consumer = schema({ players, myauth: authLib });

    const authInfo = consumer.submoduleDispatchInfos[0];
    expect(authInfo.namespace).toBe('myauth');
    expect(authInfo.subDispatches).toHaveLength(1);
    expect(authInfo.subDispatches[0].namespace).toBe('baz');
    expect(authInfo.subDispatches[0].tables[0].accessorName).toBe('bazTable');
  });
});
