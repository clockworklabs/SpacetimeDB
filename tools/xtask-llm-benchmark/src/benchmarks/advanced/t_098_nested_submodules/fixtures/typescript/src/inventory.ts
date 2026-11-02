import { schema, table, t, SenderError, type ReducerCtx } from 'spacetimedb/server';

export const item = table(
  { name: 'item', public: true },
  { id: t.u64().primaryKey(), quantity: t.u32() }
);

const spacetimedb = schema({ item });
export default spacetimedb;

type InventoryCtx = ReducerCtx<typeof spacetimedb.schemaType>;

export const set = spacetimedb.reducer(
  { id: t.u64(), quantity: t.u32() },
  (ctx, { id, quantity }) => {
    if (ctx.db.item.id.find(id)) ctx.db.item.id.update({ id, quantity });
    else ctx.db.item.insert({ id, quantity });
  }
);

export function take(ctx: InventoryCtx, id: bigint, quantity: number): void {
  const row = ctx.db.item.id.find(id);
  if (!row) throw new SenderError('missing item');
  if (row.quantity < quantity) throw new SenderError('insufficient stock');
  ctx.db.item.id.update({ id, quantity: row.quantity - quantity });
}

export function add(ctx: InventoryCtx, id: bigint, quantity: number): void {
  const row = ctx.db.item.id.find(id);
  const total = (row?.quantity ?? 0) + quantity;
  if (total > 0xffff_ffff) throw new Error('quantity overflow');
  if (row) ctx.db.item.id.update({ id, quantity: total });
  else ctx.db.item.insert({ id, quantity: total });
}
