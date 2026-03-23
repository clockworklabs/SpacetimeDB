import { schema, table, t } from 'spacetimedb/server';

const account = table({
  name: 'account',
}, {
  id: t.u64().primaryKey().autoInc(),
  email: t.string().unique(),
  displayName: t.string(),
});

const spacetimedb = schema({ account });
export default spacetimedb;

export const create_account = spacetimedb.reducer(
  { email: t.string(), displayName: t.string() },
  (ctx, { email, displayName }) => {
    ctx.db.account.insert({
      id: 0n,
      email,
      displayName,
    });
  }
);
