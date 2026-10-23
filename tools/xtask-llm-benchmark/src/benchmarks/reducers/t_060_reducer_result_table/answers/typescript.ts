import { schema, table, t } from 'spacetimedb/server';

const commandResult = table(
  { name: 'command_result', public: true },
  { requestId: t.string().primaryKey(), success: t.bool(), message: t.string() }
);
const spacetimedb = schema({ commandResult });
export default spacetimedb;

export const run_command = spacetimedb.reducer(
  { requestId: t.string(), value: t.i32() },
  (ctx, { requestId, value }) => ctx.db.commandResult.insert({
    requestId,
    success: true,
    message: `value=${value}`,
  })
);
