import { schema, table, t } from 'spacetimedb/server';

const customer = table({
  name: 'customer',
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
});

const order = table({
  name: 'order',
}, {
  id: t.u64().primaryKey().autoInc(),
  customerId: t.u64().index('btree'),
  product: t.string(),
  amount: t.u32(),
});

const order_detail = table({
  name: 'order_detail',
}, {
  orderId: t.u64().primaryKey(),
  customerName: t.string(),
  product: t.string(),
  amount: t.u32(),
});

const spacetimedb = schema({ customer, order, order_detail });
export default spacetimedb;

export const build_order_details = spacetimedb.reducer(
  (ctx) => {
    for (const o of ctx.db.order.iter()) {
      const c = ctx.db.customer.id.find(o.customerId);
      if (c) {
        ctx.db.order_detail.insert({
          orderId: o.id,
          customerName: c.name,
          product: o.product,
          amount: o.amount,
        });
      }
    }
  }
);
