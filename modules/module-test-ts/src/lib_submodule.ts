/// <reference path="./environment_sys.d.ts" />
import { schema, table, t, SyncResponse } from 'spacetimedb/server';
import { env_get } from 'spacetime:sys@2.2';

const libData = table(
  { name: 'libData', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    value: t.string(),
  }
);

const libSubmoduleSchema = schema({ libData });
export default libSubmoduleSchema;

export const libInsert = libSubmoduleSchema.reducer(
  { value: t.string() },
  (ctx, { value }) => {
    console.info(`libInsert: ${value}`);
    ctx.db.libData.insert({ id: 0n, value });
  }
);

export const libCount = libSubmoduleSchema.procedure(t.u64(), ctx =>
  ctx.withTx(tx => tx.db.libData.count())
);

export const libHello = libSubmoduleSchema.httpHandler((_ctx, _req) => {
  return new SyncResponse('Hello from lib submodule!');
});

// Ordinary helpers retain their caller's host scope. Exported module callbacks
// below are entered through the lib namespace and must be rejected by the host.
export function readRootEnvironmentHelper(): string | null {
  return env_get('EMPTY');
}
export const envReadReducer = libSubmoduleSchema.reducer(() => {
  env_get('EMPTY');
});
export const envReadProcedure = libSubmoduleSchema.procedure(t.string(), () =>
  env_get('EMPTY') ?? ''
);
export const envReadInTx = libSubmoduleSchema.procedure(t.string(), ctx =>
  ctx.withTx(() => env_get('EMPTY') ?? '')
);
export const envReadView = libSubmoduleSchema.view(
  { public: true }, t.array(t.object('EnvReadRow', { value: t.string() })), () => [{ value: env_get('EMPTY') ?? '' }]
);
export const envReadHandler = libSubmoduleSchema.httpHandler(() =>
  new SyncResponse(env_get('EMPTY') ?? '')
);

// Deliberately forge module-returned SQL through an ordinary query object's
// runtime brand. SDK types are not a security boundary for this host feature.
export function uncheckedEnvironmentQuery(source: object) {
  return Object.assign(Object.create(source), {
    toSql: () => 'SELECT * FROM st_env',
  }) as { key: string; value: string }[];
}
export const envReadSqlView = libSubmoduleSchema.view(
  { public: true },
  t.array(t.object('EnvSqlRow', { key: t.string(), value: t.string() })),
  ctx => uncheckedEnvironmentQuery(ctx.from.libData)
);
