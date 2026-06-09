import { table, schema, t } from 'spacetimedb/server';

const account = table({
  name: 'account',
  indexes: [{ name: 'byName', algorithm: 'btree', columns: ['name'] }],
}, {
  id: t.u64().primaryKey().autoInc(),
  email: t.string().unique(),
  name: t.string(),
});

const spacetimedb = schema({ account });
export default spacetimedb;

export const seed = spacetimedb.reducer(
  ctx => {
    ctx.db.account.insert({ id: 0n, email: "a@example.com", name: "Alice" });
    ctx.db.account.insert({ id: 0n, email: "b@example.com", name: "Bob" });
  }
);
