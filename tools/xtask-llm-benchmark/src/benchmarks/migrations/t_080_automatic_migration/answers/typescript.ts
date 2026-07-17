import { schema, table, t } from 'spacetimedb/server';

const product = table(
  { name: 'product', public: true },
  { id: t.u64().primaryKey(), name: t.string() }
);
const category = table(
  { name: 'category', public: true },
  { id: t.u64().primaryKey(), label: t.string() }
);
const spacetimedb = schema({ product, category });
export default spacetimedb;

export const seed = spacetimedb.reducer(ctx => {
  ctx.db.product.insert({ id: 1n, name: 'legacy' });
});
export const touch = spacetimedb.reducer(_ctx => {});
export const create_category = spacetimedb.reducer(
  { id: t.u64(), label: t.string() },
  (ctx, { id, label }) => ctx.db.category.insert({ id, label })
);
