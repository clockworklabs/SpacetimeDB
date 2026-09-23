import { schema, table, t } from 'spacetimedb/server';
import type { InferSchema, ReducerCtx } from 'spacetimedb/server';

const sale = table({ name: 'sale', public: true }, {
  id: t.u64().primaryKey(), category: t.string(), amount: t.i64(),
});
const categoryTotal = table({ name: 'category_total', public: true }, {
  category: t.string().primaryKey(), totalAmount: t.i64(), saleCount: t.u64(),
});
const spacetimedb = schema({ sale, categoryTotal });
export default spacetimedb;

type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;

function addToTotal(ctx: Ctx, category: string, amount: bigint) {
  const total = ctx.db.categoryTotal.category.find(category);
  if (total) ctx.db.categoryTotal.category.update({ ...total, totalAmount: total.totalAmount + amount, saleCount: total.saleCount + 1n });
  else ctx.db.categoryTotal.insert({ category, totalAmount: amount, saleCount: 1n });
}

function removeFromTotal(ctx: Ctx, category: string, amount: bigint) {
  const total = ctx.db.categoryTotal.category.find(category);
  if (!total) throw new Error('missing category total');
  if (total.saleCount === 1n) ctx.db.categoryTotal.category.delete(category);
  else ctx.db.categoryTotal.category.update({ ...total, totalAmount: total.totalAmount - amount, saleCount: total.saleCount - 1n });
}

function upsertSale(ctx: Ctx, row: { id: bigint; category: string; amount: bigint }) {
  const old = ctx.db.sale.id.find(row.id);
  if (old) { removeFromTotal(ctx, old.category, old.amount); ctx.db.sale.id.update(row); }
  else ctx.db.sale.insert(row);
  addToTotal(ctx, row.category, row.amount);
}

function deleteSale(ctx: Ctx, id: bigint) {
  const old = ctx.db.sale.id.find(id);
  if (!old) return;
  ctx.db.sale.id.delete(id);
  removeFromTotal(ctx, old.category, old.amount);
}

export const exercise = spacetimedb.reducer(ctx => {
  upsertSale(ctx, { id: 1n, category: 'books', amount: 10n });
  upsertSale(ctx, { id: 2n, category: 'books', amount: 20n });
  upsertSale(ctx, { id: 2n, category: 'books', amount: 25n });
  upsertSale(ctx, { id: 3n, category: 'games', amount: 40n });
  deleteSale(ctx, 3n);
  deleteSale(ctx, 1n);
});

export const category_summary = spacetimedb.view(
  { name: 'category_summary', public: true }, t.array(categoryTotal.rowType), ctx => ctx.from.categoryTotal
);
