import { schema, table, t } from 'spacetimedb/server';

const product = table({
  name: 'product',
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
  price: t.u32().index('btree'),
});

const priceRangeResult = table({
  name: 'price_range_result',
}, {
  productId: t.u64().primaryKey(),
  name: t.string(),
  price: t.u32(),
});

const spacetimedb = schema({ product, priceRangeResult });
export default spacetimedb;

export const find_in_price_range = spacetimedb.reducer(
  { minPrice: t.u32(), maxPrice: t.u32() },
  (ctx, { minPrice, maxPrice }) => {
    for (const p of ctx.db.product.iter()) {
      if (p.price >= minPrice && p.price <= maxPrice) {
        ctx.db.priceRangeResult.insert({
          productId: p.id,
          name: p.name,
          price: p.price,
        });
      }
    }
  }
);
