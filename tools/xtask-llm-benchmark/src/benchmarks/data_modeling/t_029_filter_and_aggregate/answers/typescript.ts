import { schema, table, t } from 'spacetimedb/server';

const order = table({
  name: 'order',
}, {
  id: t.u64().primaryKey().autoInc(),
  category: t.string().index('btree'),
  amount: t.u64(),
  fulfilled: t.bool(),
});

const category_stats = table({
  name: 'category_stats',
}, {
  category: t.string().primaryKey(),
  totalAmount: t.u64(),
  orderCount: t.u32(),
});

const spacetimedb = schema({ order, category_stats });
export default spacetimedb;

export const compute_stats = spacetimedb.reducer(
  { category: t.string() },
  (ctx, { category }) => {
    let totalAmount = 0n;
    let orderCount = 0;

    for (const o of ctx.db.order.category.filter(category)) {
      totalAmount += o.amount;
      orderCount += 1;
    }

    // Upsert: delete existing then insert
    ctx.db.category_stats.category.delete(category);
    ctx.db.category_stats.insert({
      category,
      totalAmount,
      orderCount,
    });
  }
);
