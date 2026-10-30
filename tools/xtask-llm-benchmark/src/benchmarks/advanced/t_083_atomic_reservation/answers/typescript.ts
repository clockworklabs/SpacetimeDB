import { schema, table, t, SenderError } from 'spacetimedb/server';

const stock = table(
  { name: 'stock', public: true },
  {
    id: t.u64().primaryKey(),
    available: t.i64(),
  }
);
const reservation = table(
  { name: 'reservation', public: true },
  {
    requestId: t.string().primaryKey(),
    firstId: t.u64(),
    firstQty: t.i64(),
    secondId: t.u64(),
    secondQty: t.i64(),
  }
);
const spacetimedb = schema({ stock, reservation });
export default spacetimedb;

export const add_stock = spacetimedb.reducer(
  { id: t.u64(), available: t.i64() },
  (ctx, { id, available }) => {
    if (available < 0n) throw new SenderError('invalid stock');
    ctx.db.stock.insert({ id, available });
  }
);

export const reserve = spacetimedb.reducer(
  {
    requestId: t.string(),
    firstId: t.u64(),
    firstQty: t.i64(),
    secondId: t.u64(),
    secondQty: t.i64(),
  },
  (ctx, { requestId, firstId, firstQty, secondId, secondQty }) => {
    const old = ctx.db.reservation.requestId.find(requestId);
    if (old) {
      if (
        old.firstId !== firstId ||
        old.firstQty !== firstQty ||
        old.secondId !== secondId ||
        old.secondQty !== secondQty
      )
        throw new SenderError('request conflict');
      return;
    }
    if (!requestId || firstId === secondId || firstQty <= 0n || secondQty <= 0n)
      throw new SenderError('invalid reservation');
    const first = ctx.db.stock.id.find(firstId),
      second = ctx.db.stock.id.find(secondId);
    if (!first || !second) throw new SenderError('missing product');
    if (first.available < firstQty || second.available < secondQty)
      throw new SenderError('insufficient stock');
    ctx.db.stock.id.update({ ...first, available: first.available - firstQty });
    ctx.db.stock.id.update({
      ...second,
      available: second.available - secondQty,
    });
    ctx.db.reservation.insert({
      requestId,
      firstId,
      firstQty,
      secondId,
      secondQty,
    });
  }
);
