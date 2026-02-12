import { table, schema, t } from 'spacetimedb/server';

export const User = table({
  name: 'user',
}, {
  id: t.i32().primaryKey(),
  name: t.string(),
  age: t.i32(),
  active: t.bool(),
});

const spacetimedb = schema({ User });
export default spacetimedb;

export const deleteUser = spacetimedb.reducer({ id: t.i32() },
  (ctx, { id }) => {
    ctx.db.user.id.delete(id);
  }
);
