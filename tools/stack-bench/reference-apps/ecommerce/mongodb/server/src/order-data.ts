import type { Connection } from 'mongoose';

export async function initializeOrderData(connection: Connection) {
  const db = connection.db!;
  const lineId = { $concat: [{ $toString: '$_id' }, ':', { $toString: '$lineIndex' }] };
  const views = [
    { name: 'order_account', viewOn: 'users', pipeline: [{ $project: { id: '$_id', username: 1, _id: 0 } }] },
    { name: 'order_header', viewOn: 'orders', pipeline: [{ $project: {
      id: '$_id', account_id: '$userId', total: 1, status: 1, _id: 0,
      refunded: { $cond: [{ $eq: ['$status', 'cancelled'] }, '$total', '$refundTotal'] },
    } }] },
    { name: 'order_line', viewOn: 'orders', pipeline: [
      { $unwind: { path: '$items', includeArrayIndex: 'lineIndex' } },
      { $project: { _id: 0, id: lineId, order_id: '$_id', item_id: '$items.itemId',
        quantity: '$items.quantity', unit_price: '$items.price' } },
    ] },
    { name: 'order_cart', viewOn: 'carts', pipeline: [
      { $unwind: '$items' },
      { $project: { _id: 0, account_id: '$userId', item_id: '$items.itemId', quantity: '$items.quantity' } },
    ] },
    { name: 'order_allocation', viewOn: 'orders', pipeline: [
      { $unwind: { path: '$items', includeArrayIndex: 'lineIndex' } },
      { $unwind: '$items.allocations' },
      { $project: { _id: 0, order_line_id: lineId, warehouse_id: '$items.allocations.warehouseId',
        quantity: '$items.allocations.quantity' } },
    ] },
  ];
  const existing = new Set((await db.listCollections({}, { nameOnly: true }).toArray()).map(row => row.name));
  for (const { name, viewOn, pipeline } of views) {
    if (existing.has(name)) await db.command({ collMod: name, viewOn, pipeline });
    else await db.createCollection(name, { viewOn, pipeline });
  }
}
