import { schema, table, t, SenderError } from 'spacetimedb/server';
import type { InferSchema, ReducerCtx } from 'spacetimedb/server';
const sale = table(
  { name: 'sale', public: true },
  {
    id: t.u64().primaryKey(),
    category: t.string(),
    amount: t.i64(),
  }
);
const categoryTotal = table(
  { name: 'category_total', public: true },
  {
    category: t.string().primaryKey(),
    totalAmount: t.i64(),
    saleCount: t.u64(),
  }
);
const spacetimedb = schema({ sale, categoryTotal });
export default spacetimedb;

function adjust(
  ctx: ReducerCtx<InferSchema<typeof spacetimedb>>,
  category: string,
  amount: bigint,
  adding: boolean
) {
  const total = ctx.db.categoryTotal.category.find(category) ?? {
    category,
    totalAmount: 0n,
    saleCount: 0n,
  };
  const next = {
    category,
    totalAmount: total.totalAmount + (adding ? amount : -amount),
    saleCount: total.saleCount + (adding ? 1n : -1n),
  };
  ctx.db.categoryTotal.category.delete(category);
  if (next.saleCount > 0n) ctx.db.categoryTotal.insert(next);
}
export const set_sale = spacetimedb.reducer(
  { id: t.u64(), category: t.string(), amount: t.i64() },
  (ctx, { id, category, amount }) => {
    if (!category) throw new SenderError('invalid category');
    const old = ctx.db.sale.id.find(id);
    if (old) {
      adjust(ctx, old.category, old.amount, false);
      ctx.db.sale.id.delete(id);
    }
    ctx.db.sale.insert({ id, category, amount });
    adjust(ctx, category, amount, true);
  }
);

export const remove_sale = spacetimedb.reducer(
  { id: t.u64() },
  (ctx, { id }) => {
    const old = ctx.db.sale.id.find(id);
    if (old) {
      ctx.db.sale.id.delete(id);
      adjust(ctx, old.category, old.amount, false);
    }
  }
);
