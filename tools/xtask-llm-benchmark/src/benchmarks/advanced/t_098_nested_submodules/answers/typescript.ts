import { schema, t, SenderError, type ReducerCtx } from 'spacetimedb/server';
import * as branch from './branch';
import * as inventory from './inventory';

const spacetimedb = schema({ east: branch, west: branch, reserve: inventory });
export default spacetimedb;

type RootCtx = ReducerCtx<typeof spacetimedb.schemaType>;

function select(ctx: RootCtx, location: string) {
  switch (location) {
    case 'east':
      return ctx.as.east.as.stock;
    case 'west':
      return ctx.as.west.as.stock;
    case 'reserve':
      return ctx.as.reserve;
    default:
      throw new SenderError('invalid location');
  }
}

export const set_stock = spacetimedb.reducer(
  { location: t.string(), id: t.u64(), quantity: t.u32() },
  (ctx, { location, id, quantity }) => {
    inventory.set(select(ctx, location), { id, quantity });
  }
);

export const transfer = spacetimedb.reducer(
  { source: t.string(), destination: t.string(), id: t.u64(), quantity: t.u32() },
  (ctx, { source, destination, id, quantity }) => {
    const from = select(ctx, source);
    const to = select(ctx, destination);
    if (source === destination) throw new SenderError('same location');
    if (quantity === 0) throw new SenderError('invalid quantity');
    inventory.take(from, id, quantity);
    inventory.add(to, id, quantity);
  }
);

export const east_stock = spacetimedb.anonymousView(
  { name: 'east_stock', public: true },
  t.array(inventory.item.rowType),
  ctx => [...ctx.db.east.stock.item.iter()]
);

export const west_stock = spacetimedb.anonymousView(
  { name: 'west_stock', public: true },
  t.array(inventory.item.rowType),
  ctx => [...ctx.db.west.stock.item.iter()]
);
