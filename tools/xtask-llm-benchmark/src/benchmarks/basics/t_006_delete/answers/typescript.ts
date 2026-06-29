import { table, schema, t } from 'spacetimedb/server';

const user = table(
  {
    name: 'user',
  },
  {
    id: t.u64().primaryKey().autoInc(),
    name: t.string(),
    age: t.i32(),
    active: t.bool(),
  }
);

const spacetimedb = schema({ user });
export default spacetimedb;

export const insertUser = spacetimedb.reducer(
  { name: t.string(), age: t.i32(), active: t.bool() },
  (ctx, { name, age, active }) => {
    ctx.db.user.insert({ id: 0n, name, age, active });
  }
);

export const deleteUser = spacetimedb.reducer(
  { id: t.u64() },
  (ctx, { id }) => {
    ctx.db.user.id.delete(id);
  }
);
