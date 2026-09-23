import { schema, table, t } from 'spacetimedb/server';

const counter = table(
  { name: 'counter', public: true },
  { id: t.u64().primaryKey(), value: t.i64() }
);
const spacetimedb = schema({ counter });
export default spacetimedb;

export const seed = spacetimedb.reducer(ctx => {
  ctx.db.counter.insert({ id: 1n, value: 1n });
});
export const increment = spacetimedb.reducer(
  { id: t.u64(), amount: t.i64() },
  (ctx, { id, amount }) => {
    const row = ctx.db.counter.id.find(id);
    if (!row) throw new Error('counter');
    ctx.db.counter.id.update({ ...row, value: row.value + amount });
  }
);
