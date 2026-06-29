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

const result = table(
  {
    name: 'result',
  },
  {
    id: t.u64().primaryKey(),
    name: t.string(),
  }
);

const spacetimedb = schema({ user, result });
export default spacetimedb;

export const insertUser = spacetimedb.reducer(
  { name: t.string(), age: t.i32(), active: t.bool() },
  (ctx, { name, age, active }) => {
    ctx.db.user.insert({ id: 0n, name, age, active });
  }
);

export const lookupUserName = spacetimedb.reducer(
  { id: t.u64() },
  (ctx, { id }) => {
    const u = ctx.db.user.id.find(id);
    if (u) {
      ctx.db.result.insert({ id: u.id, name: u.name });
    }
  }
);
