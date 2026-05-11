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
});
