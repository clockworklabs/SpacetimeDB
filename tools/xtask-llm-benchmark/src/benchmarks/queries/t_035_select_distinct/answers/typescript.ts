import { schema, table, t } from 'spacetimedb/server';

const order = table({
  name: 'order',
}, {
  id: t.u64().primaryKey().autoInc(),
  category: t.string(),
  amount: t.u32(),
});

const distinct_category = table({
  name: 'distinct_category',
}, {
  category: t.string().primaryKey(),
});

const spacetimedb = schema({ order, distinct_category });
export default spacetimedb;

export const collect_distinct_categories = spacetimedb.reducer(
  (ctx) => {
    const categories = new Set<string>();
    for (const o of ctx.db.order.iter()) {
      categories.add(o.category);
    }
    for (const category of categories) {
      ctx.db.distinct_category.insert({ category });
    }
  }
);
