import { table, schema, t } from 'spacetimedb/server';

export const User = table({
  name: 'user',
}, {
  id: t.i32().primaryKey(),
  name: t.string(),
  age: t.i32(),
  active: t.bool(),
});

export const Result = table({
  name: 'result',
}, {
  id: t.i32().primaryKey(),
  name: t.string(),
});

const spacetimedb = schema(User, Result);

spacetimedb.reducer('lookupUserName', { id: t.i32() },
  (ctx, { id }) => {
    const u = ctx.db.user.id.find(id);
    if (u) {
      ctx.db.result.insert({ id: u.id, name: u.name });
    }
  }
);
