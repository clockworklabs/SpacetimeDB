import { beforeAll, describe, expect, it, vi } from 'vitest';

const lookedUpTables = vi.hoisted(() => [] as string[]);

vi.mock(
  'spacetime:sys@2.0',
  () => ({
    moduleHooks: Symbol('moduleHooks'),
    table_id_from_name: (name: string) => {
      lookedUpTables.push(name);
      return 1;
    },
    index_id_from_name: () => 1,
    row_iter_bsatn_close: () => {},
  }),
  { virtual: true }
);

describe('ctx.as alias proxy', () => {
  let schema: typeof import('../src/server/schema').schema;
  let table: typeof import('../src/lib/table').table;
  let t: typeof import('../src/lib/type_builders').t;
  let moduleHooks: symbol;

  beforeAll(async () => {
    ({ schema } = await import('../src/server/schema'));
    ({ table } = await import('../src/lib/table'));
    ({ t } = await import('../src/lib/type_builders'));
    ({ moduleHooks } = (await import('spacetime:sys@2.0')) as any);
  });

  it('ctx.as.<alias> provides a narrowed ctx with library db and delegating sender', async () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions });
    const authLib = { default: authSchema };

    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });
    const consumer = schema({ players, myauth: authLib });

    let capturedCtx: any;
    const myReducer = consumer.reducer((ctx: any) => {
      capturedCtx = ctx;
    });

    const hooks = (consumer as any)[moduleHooks]({ myReducer });
    hooks.__call_reducer__(0, 0n, 0n, 0n, new DataView(new ArrayBuffer(0)));

    expect(capturedCtx.as).toBeDefined();
    expect(capturedCtx.as.myauth).toBeDefined();
    expect(capturedCtx.as.myauth.db).toBeDefined();
    expect(capturedCtx.as.myauth.db.sessions).toBeDefined();
    expect(capturedCtx.as.myauth.sender).toBe(capturedCtx.sender);
    expect(capturedCtx.as.myauth.timestamp).toBe(capturedCtx.timestamp);
  });

  it('ctx.as.<alias> and table lookups use the accessor key of a named mount', async () => {
    const sessions = table(
      { name: 'sessions' },
      { id: t.u64().primaryKey().autoInc() }
    );
    const authSchema = schema({ sessions });
    const authLib = { default: authSchema };

    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });
    // The canonical namespace (`AuthLib`) is only a database concern: module code
    // reaches the submodule through the key, and the runtime resolves its tables
    // through the accessor alias the host stores for them.
    const consumer = schema({
      players,
      myAuth: { name: 'AuthLib', module: authLib },
    });

    let capturedCtx: any;
    const myReducer = consumer.reducer((ctx: any) => {
      capturedCtx = ctx;
    });

    lookedUpTables.length = 0;
    const hooks = (consumer as any)[moduleHooks]({ myReducer });
    hooks.__call_reducer__(0, 0n, 0n, 0n, new DataView(new ArrayBuffer(0)));

    expect(capturedCtx.as.myAuth).toBeDefined();
    expect(capturedCtx.as.AuthLib).toBeUndefined();
    expect(capturedCtx.as.myAuth.db.sessions).toBeDefined();
    expect(capturedCtx.db.myAuth.sessions).toBeDefined();
    expect(lookedUpTables).toContain('myAuth.sessions');
    expect(lookedUpTables.some(n => n.startsWith('AuthLib.'))).toBe(false);
  });

  it('ctx.as is empty object when there are no submodules', async () => {
    const players = table({ name: 'players' }, { id: t.u32().primaryKey() });
    const consumer = schema({ players });

    let capturedCtx: any;
    const myReducer = consumer.reducer((ctx: any) => {
      capturedCtx = ctx;
    });

    const hooks = (consumer as any)[moduleHooks]({ myReducer });
    hooks.__call_reducer__(0, 0n, 0n, 0n, new DataView(new ArrayBuffer(0)));

    expect(capturedCtx.as).toBeDefined();
    expect(Object.keys(capturedCtx.as)).toHaveLength(0);
  });

  it('ctx.as.<alias>.as carries nested submodule aliases', async () => {
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

    let capturedCtx: any;
    const myReducer = consumer.reducer((ctx: any) => {
      capturedCtx = ctx;
    });

    const hooks = (consumer as any)[moduleHooks]({ myReducer });
    hooks.__call_reducer__(0, 0n, 0n, 0n, new DataView(new ArrayBuffer(0)));

    expect(capturedCtx.as.myauth.as.baz).toBeDefined();
    expect(capturedCtx.as.myauth.as.baz.db.bazTable).toBeDefined();
    expect(capturedCtx.as.myauth.as.baz.sender).toBe(capturedCtx.sender);
  });
});
