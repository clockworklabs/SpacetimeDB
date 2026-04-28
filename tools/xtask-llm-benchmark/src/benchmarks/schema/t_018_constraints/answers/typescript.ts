import { table, schema, t } from 'spacetimedb/server';

const account = table({
  name: 'account',
  indexes: [{ name: 'byName', algorithm: 'btree', columns: ['name'] }],
}, {
  id: t.i32().primaryKey(),
  email: t.string().unique(),
  name: t.string(),
});

const spacetimedb = schema({ account });
export default spacetimedb;

export const seed = spacetimedb.reducer(
  ctx => {
    ctx.db.account.insert({ id: 1, email: "a@example.com", name: "Alice" });
    ctx.db.account.insert({ id: 2, email: "b@example.com", name: "Bob" });
  }
);
