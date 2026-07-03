import { schema, table, t, SyncResponse, Router } from 'spacetimedb/server';

const lib_data = table(
  { name: 'lib_data', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    value: t.string(),
  }
);

const libSubmoduleSchema = schema({ lib_data });
export default libSubmoduleSchema;

export const lib_insert = libSubmoduleSchema.reducer(
  { value: t.string() },
  (ctx, { value }) => {
    console.info(`lib_insert: ${value}`);
    ctx.db.lib_data.insert({ id: 0n, value });
  }
);

export const lib_count = libSubmoduleSchema.procedure(
  t.u64(),
  (ctx) => ctx.withTx(tx => tx.db.lib_data.count())
);

export const lib_hello = libSubmoduleSchema.httpHandler((_ctx, _req) => {
  return new SyncResponse('Hello from lib submodule!');
});
