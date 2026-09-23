import { schema, table, t } from 'spacetimedb/server';

const product = table(
  { name: 'product', public: true },
  { id: t.u64().primaryKey(), name: t.string() }
);
const spacetimedb = schema({ product });
export default spacetimedb;

export const seed = spacetimedb.reducer(ctx => {
  ctx.db.product.insert({ id: 1n, name: 'legacy' });
});
export const touch = spacetimedb.reducer(_ctx => {});
