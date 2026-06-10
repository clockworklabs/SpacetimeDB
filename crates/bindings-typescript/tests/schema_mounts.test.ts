import { beforeAll, describe, expect, it, vi } from 'vitest';

vi.mock(
  'spacetime:sys@2.0',
  () => ({
    moduleHooks: Symbol('moduleHooks'),
  }),
  { virtual: true }
);

vi.mock('spacetime:sys@2.1', () => ({}), { virtual: true });

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

describe('schema mounts', () => {
  let schema: typeof import('../src/server/schema').schema;
  let table: typeof import('../src/lib/table').table;
  let t: typeof import('../src/lib/type_builders').t;

  beforeAll(async () => {
    ({ schema } = await import('../src/server/schema'));
    ({ table } = await import('../src/lib/table'));
    ({ t } = await import('../src/lib/type_builders'));
  });

  it('emits mounted submodule module defs and resolves mounted schedules', () => {
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
    const mounts = raw.sections.find(
      section => section.tag === 'Mounts'
    )?.value;

    expect(mounts).toHaveLength(1);
    expect(mounts?.[0]?.namespace).toBe('myauth');

    const mountedSections = mounts?.[0]?.module.sections ?? [];
    const mountedReducers = mountedSections.find(
      section => section.tag === 'Reducers'
    )?.value;
    const mountedSchedules = mountedSections.find(
      section => section.tag === 'Schedules'
    )?.value;

    expect(mountedReducers).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ sourceName: 'cleanExpiredSessions' }),
      ])
    );
    expect(mountedSchedules).toEqual([
      expect.objectContaining({
        tableName: 'sessionCleanupTick',
        functionName: 'cleanExpiredSessions',
      }),
    ]);
  });

  it('rejects default-import style mounts with a clear error', () => {
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

  it('populates mountedDispatchInfos with reducer fns and table metadata', () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );

    const authSchema = schema({ sessions });
    const cleanExpiredSessions = authSchema.reducer(() => {});
    const authLib = { default: authSchema, cleanExpiredSessions };

    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });
    const consumer = schema({ players, myauth: authLib });

    const infos = consumer.mountedDispatchInfos;
    expect(infos).toHaveLength(1);

    const info = infos[0];
    expect(info.reducerFns).toHaveLength(1);
    expect(info.reducerDefs).toHaveLength(1);
    expect(info.reducerDefs[0].sourceName).toBe('cleanExpiredSessions');
    expect(info.tables).toHaveLength(1);
    expect(info.tables[0].accessorName).toBe('sessions');
    expect(info.subDispatches).toHaveLength(0);
  });

  it('flattens nested mount dispatches depth-first', () => {
    // baz library: 1 reducer
    const bazTable = table({ name: 'baz_items' }, { id: t.u32().primaryKey() });
    const bazSchema = schema({ bazTable });
    const bazReducer = bazSchema.reducer(() => {});
    const bazLib = { default: bazSchema, bazReducer };

    // auth library: 1 own reducer, mounts baz
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions, baz: bazLib });
    const authReducer = authSchema.reducer(() => {});
    const authLib = { default: authSchema, authReducer };

    // consumer: 1 own reducer, mounts auth
    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });
    const consumer = schema({ players, myauth: authLib });
    const consumerReducer = consumer.reducer(() => {});

    // Verify depth-first structure:
    // consumer.mountedDispatchInfos[0] = myauth (authReducer)
    // consumer.mountedDispatchInfos[0].subDispatches[0] = myauth.baz (bazReducer)
    const infos = consumer.mountedDispatchInfos;
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

  it('mountedDispatchInfos carry namespace and nested namespace dispatches propagate', () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions });
    const authLib = { default: authSchema };

    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });
    const consumer = schema({ players, myauth: authLib });

    const infos = consumer.mountedDispatchInfos;
    expect(infos).toHaveLength(1);
    expect(infos[0].namespace).toBe('myauth');
    expect(infos[0].tables[0].accessorName).toBe('sessions');
  });

  it('nested mounts carry their own namespace on subDispatches', () => {
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

    const authInfo = consumer.mountedDispatchInfos[0];
    expect(authInfo.namespace).toBe('myauth');
    expect(authInfo.subDispatches).toHaveLength(1);
    expect(authInfo.subDispatches[0].namespace).toBe('baz');
    expect(authInfo.subDispatches[0].tables[0].accessorName).toBe('bazTable');
  });
});
