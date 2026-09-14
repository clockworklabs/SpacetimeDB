import { schema, table, t, SyncResponse } from 'spacetimedb/server';

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
