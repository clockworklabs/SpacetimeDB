import { table, schema, t } from 'spacetimedb/server';

export const User = table({
  name: 'user',
}, {
  id: t.i32().primaryKey(),
  name: t.string(),
  age: t.i32(),
  active: t.bool(),
});

const spacetimedb = schema(User);

spacetimedb.init(ctx => {
  ctx.db.user.insert({ id: 1, name: "Alice", age: 30, active: true });
  ctx.db.user.insert({ id: 2, name: "Bob", age: 22, active: false });
});
