import { table, schema, t } from 'spacetimedb/server';

const user = table(
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

export const insertUser = spacetimedb.reducer(
  { id: t.i32(), name: t.string(), age: t.i32(), active: t.bool() },
  (ctx, { id, name, age, active }) => {
    ctx.db.user.insert({ id, name, age, active });
  }
);
