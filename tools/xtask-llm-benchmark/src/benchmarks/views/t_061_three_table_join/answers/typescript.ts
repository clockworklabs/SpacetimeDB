import { schema, table, t } from 'spacetimedb/server';
const customer = table({ name: 'customer', public: true }, { id: t.u64().primaryKey(), name: t.string() });
const purchase = table({ name: 'purchase', public: true }, { id: t.u64().primaryKey(), customerId: t.u64().index('btree') });
const lineItem = table({ name: 'line_item', public: true }, { id: t.u64().primaryKey(), purchaseId: t.u64().index('btree'), sku: t.string(), visible: t.bool().index('btree') });
const OrderLineDetail = t.row('ThreeTableJoinRow', { lineId: t.u64(), customerName: t.string(), sku: t.string() });
const spacetimedb = schema({ customer, purchase, lineItem }); export default spacetimedb;
export const seed = spacetimedb.reducer(ctx => { ctx.db.customer.insert({ id: 1n, name: 'Ada' }); ctx.db.purchase.insert({ id: 10n, customerId: 1n }); ctx.db.lineItem.insert({ id: 100n, purchaseId: 10n, sku: 'SKU-1', visible: true }); });
export const order_line_detail = spacetimedb.anonymousView({ name: 'order_line_detail', public: true }, t.array(OrderLineDetail), ctx => {
  const rows: Array<{ lineId: bigint; customerName: string; sku: string }> = []; for (const line of ctx.db.lineItem.visible.filter(true)) { const purchase = ctx.db.purchase.id.find(line.purchaseId); if (!purchase) continue; const customer = ctx.db.customer.id.find(purchase.customerId); if (customer) rows.push({ lineId: line.id, customerName: customer.name, sku: line.sku }); } return rows;
});
