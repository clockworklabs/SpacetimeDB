import { beforeEach, describe, expect, it } from 'vitest';
import { moduleHooks, __resetMockSys } from 'spacetime:sys@2.0';
import { schema, table } from '../src/server/index';
import t from '../src/lib/type_builders';

describe('server schema namespacing', () => {
  beforeEach(() => {
    __resetMockSys();
  });

  it('mounts library tables and reducers under an alias', () => {
    const seen = {
      rootMountedDb: false,
      rootMountedAs: false,
      mountedScopedDb: false,
    };

    const auth = schema({
      users: table({}, { id: t.u32() }),
    });

    const signIn = auth.reducer({ name: 'sign_in' }, ctx => {
      seen.mountedScopedDb =
        'users' in ctx.db && !('auth' in (ctx.db as object));
      ctx.db.users.count();
    });

    const authNs = {
      default: auth,
      signIn,
      helper(
        ctx: typeof auth.schemaType extends infer _
          ? Parameters<typeof signIn>[0]
          : never
      ) {
        ctx.db.users.count();
      },
      ignored: 'not-a-spacetime-export',
    };

    const app = schema({
      sessions: table({}, { id: t.u32() }),
      auth: authNs,
    });

    const useAuth = app.reducer(ctx => {
      seen.rootMountedDb = 'users' in ctx.db.auth;
      seen.rootMountedAs = 'users' in ctx.as.auth.db;
      ctx.db.auth.users.count();
      ctx.as.auth.db.users.count();
      authNs.helper(ctx.as.auth);
    });

    const hooks = app[moduleHooks]({ default: app, useAuth });

    expect(app.moduleDef.tables.map(table => table.sourceName)).toEqual(
      expect.arrayContaining(['sessions', '__ns__auth__users'])
    );
    expect(app.moduleDef.reducers.map(reducer => reducer.sourceName)).toEqual(
      expect.arrayContaining(['useAuth', '__ns__auth__signIn'])
    );
    expect(app.moduleDef.explicitNames.entries).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          tag: 'Function',
          value: expect.objectContaining({
            sourceName: '__ns__auth__signIn',
            canonicalName: '__ns__auth__sign_in',
          }),
        }),
      ])
    );

    const rootReducerId = app.moduleDef.reducers.findIndex(
      reducer => reducer.sourceName === 'useAuth'
    );
    const mountedReducerId = app.moduleDef.reducers.findIndex(
      reducer => reducer.sourceName === '__ns__auth__signIn'
    );

    hooks.__call_reducer__(
      rootReducerId,
      0n,
      0n,
      0n,
      new DataView(new ArrayBuffer(0))
    );
    hooks.__call_reducer__(
      mountedReducerId,
      0n,
      0n,
      0n,
      new DataView(new ArrayBuffer(0))
    );

    expect(seen.rootMountedDb).toBe(true);
    expect(seen.rootMountedAs).toBe(true);
    expect(seen.mountedScopedDb).toBe(true);
  });

  it('rejects default-import style mounts', () => {
    const auth = schema({
      users: table({}, { id: t.u32() }),
    });

    expect(() =>
      schema({
        auth: auth as never,
      })
    ).toThrow(/module namespace import/i);
  });

  it('uses the reserved internal prefix for nested mounted names', () => {
    const profile = schema({
      settings: table({}, { id: t.u32() }),
    });

    const auth = schema({
      users: table({}, { id: t.u32() }),
      profile: {
        default: profile,
      },
    });

    const app = schema({
      auth: {
        default: auth,
      },
    });

    expect(app.moduleDef.tables.map(table => table.sourceName)).toEqual(
      expect.arrayContaining([
        '__ns__auth__users',
        '__ns__auth__profile__settings',
      ])
    );
  });

  it('rejects reserved internal prefix in user-defined names', () => {
    const users = table({}, { id: t.u32() });
    const app = schema({
      users,
    });

    const reducer = app.reducer(ctx => {
      ctx.db.users.count();
    });
    const namedReducer = app.reducer({ name: '__ns__create_user' }, ctx => {
      ctx.db.users.count();
    });
    const procedure = app.procedure(t.string(), () => '');
    const namedProcedure = app.procedure(
      { name: '__ns__get_message' },
      t.string(),
      () => ''
    );
    const view = app.anonymousView(
      { name: 'listUsers', public: true },
      t.array(users.rowType),
      () => []
    );
    const namedView = app.anonymousView(
      { name: '__ns__list_users', public: true },
      t.array(users.rowType),
      () => []
    );

    expect(() =>
      schema({
        __ns__users: table({}, { id: t.u32() }),
      })
    ).toThrow(/reserved for internal mounted-library names/i);

    expect(() =>
      schema({
        users: table({ name: '__ns__users' }, { id: t.u32() }),
      })
    ).toThrow(/reserved for internal mounted-library names/i);

    expect(() =>
      schema({
        __ns__auth: {
          default: app,
        },
      })
    ).toThrow(/reserved for internal mounted-library names/i);

    expect(() =>
      app[moduleHooks]({ default: app, __ns__createUser: reducer })
    ).toThrow(/reserved for internal mounted-library names/i);

    expect(() =>
      app[moduleHooks]({ default: app, createUser: namedReducer })
    ).toThrow(/reserved for internal mounted-library names/i);

    expect(() =>
      app[moduleHooks]({ default: app, __ns__getMessage: procedure })
    ).toThrow(/reserved for internal mounted-library names/i);

    expect(() =>
      app[moduleHooks]({ default: app, getMessage: namedProcedure })
    ).toThrow(/reserved for internal mounted-library names/i);

    expect(() =>
      app[moduleHooks]({ default: app, __ns__listUsers: view })
    ).toThrow(/reserved for internal mounted-library names/i);

    expect(() =>
      app[moduleHooks]({ default: app, listUsers: namedView })
    ).toThrow(/reserved for internal mounted-library names/i);
  });
});
