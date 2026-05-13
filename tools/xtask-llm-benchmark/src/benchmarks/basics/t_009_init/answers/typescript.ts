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

export const init = spacetimedb.init(ctx => {
  ctx.db.user.insert({ id: 0n, name: 'Alice', age: 30, active: true });
  ctx.db.user.insert({ id: 0n, name: 'Bob', age: 22, active: false });
});
