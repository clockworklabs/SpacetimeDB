import { schema, table, t } from 'spacetimedb/server';

const userRecord = table(
  { name: 'user_record', public: true },
  { identity: t.identity().primaryKey(), name: t.string() }
);
const spacetimedb = schema({ userRecord });
export default spacetimedb;

export const userRecordFilter = spacetimedb.clientVisibilityFilter.sql(
  'SELECT * FROM user_record WHERE identity = :sender'
);

export const register_self = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => ctx.db.userRecord.insert({ identity: ctx.sender, name })
);
