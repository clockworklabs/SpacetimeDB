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

spacetimedb.reducer('insertUser', { id: t.i32(), name: t.string(), age: t.i32(), active: t.bool() },
  (ctx, { id, name, age, active }) => {
    ctx.db.user.insert({ id, name, age, active });
  }
);
