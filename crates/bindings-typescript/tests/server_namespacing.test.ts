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
      expect.arrayContaining(['sessions', 'auth__users'])
    );
    expect(app.moduleDef.reducers.map(reducer => reducer.sourceName)).toEqual(
      expect.arrayContaining(['useAuth', 'auth__signIn'])
    );
    expect(app.moduleDef.explicitNames.entries).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          tag: 'Function',
          value: expect.objectContaining({
            sourceName: 'auth__signIn',
            canonicalName: 'auth__sign_in',
          }),
        }),
      ])
    );

    const rootReducerId = app.moduleDef.reducers.findIndex(
      reducer => reducer.sourceName === 'useAuth'
    );
    const mountedReducerId = app.moduleDef.reducers.findIndex(
      reducer => reducer.sourceName === 'auth__signIn'
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
});
