import { table, schema, t } from 'spacetimedb/server';

export const user = table(
  {
    name: 'user',
  },
  {
    id: t.i32().primaryKey(),
    name: t.string(),
    age: t.i32(),
    active: t.bool(),
  }
);

const spacetimedb = schema({ user });
export default spacetimedb;

export const updateUser = spacetimedb.reducer(
  { id: t.i32(), name: t.string(), age: t.i32(), active: t.bool() },
  (ctx, { id, name, age, active }) => {
    ctx.db.user.id.update({ id, name, age, active });
  }
);
