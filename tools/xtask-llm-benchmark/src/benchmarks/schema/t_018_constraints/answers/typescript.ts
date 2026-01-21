import { table, schema, t } from 'spacetimedb/server';

export const Account = table({
  name: 'account',
  indexes: [{ name: 'byName', algorithm: 'btree', columns: ['name'] }],
}, {
  id: t.i32().primaryKey(),
  email: t.string().unique(),
  name: t.string(),
});

const spacetimedb = schema(Account);

spacetimedb.reducer('seed', {},
  ctx => {
    ctx.db.account.insert({ id: 1, email: "a@example.com", name: "Alice" });
    ctx.db.account.insert({ id: 2, email: "b@example.com", name: "Bob" });
  }
);
