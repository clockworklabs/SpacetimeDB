import { t } from 'spacetimedb/server';
import spacetimedb from './schema';

// Only the database publisher can read these projections. Customer sessions use
// the existing per-account views. These views never store or repair app records.
export const orderAccount = spacetimedb.view({ name: 'order_account', public: true },
  t.array(t.object('OrderDataAccount', { id: t.u64(), username: t.string() })), ctx =>
    ctx.db.orderDataReader.identity.find(ctx.sender)
      ? [...ctx.db.account.iter()].map(row => ({ id: row.id, username: row.username })) : []);

export const orderHeader = spacetimedb.view({ name: 'order_header', public: true },
  t.array(t.object('OrderDataHeader', { id: t.u64(), account_id: t.u64(), total: t.f64(),
    refunded: t.f64(), status: t.string() })), ctx =>
    ctx.db.orderDataReader.identity.find(ctx.sender)
      ? [...ctx.db.customerOrder.iter()].map(row => ({ id: row.id, account_id: row.accountId,
        total: row.total, refunded: row.status === 'cancelled' ? row.total : row.refundedTotal, status: row.status })) : []);

export const orderLine = spacetimedb.view({ name: 'order_line', public: true },
  t.array(t.object('OrderDataLine', { id: t.u64(), order_id: t.u64(), item_id: t.u64(),
    quantity: t.u32(), unit_price: t.f64() })), ctx =>
    ctx.db.orderDataReader.identity.find(ctx.sender)
      ? [...ctx.db.orderItem.iter()].map(row => ({ id: row.id, order_id: row.orderId, item_id: row.itemId,
        quantity: row.quantity, unit_price: row.unitPrice })) : []);

export const orderCart = spacetimedb.view({ name: 'order_cart', public: true },
  t.array(t.object('OrderDataCart', { account_id: t.u64(), item_id: t.u64(), quantity: t.u32() })), ctx =>
    ctx.db.orderDataReader.identity.find(ctx.sender)
      ? [...ctx.db.cartItem.iter()].map(row => ({ account_id: row.accountId, item_id: row.itemId, quantity: row.quantity })) : []);

export const orderAllocation = spacetimedb.view({ name: 'order_allocation', public: true },
  t.array(t.object('OrderDataAllocation', { order_line_id: t.u64(), warehouse_id: t.u64(), quantity: t.u32() })), ctx =>
    ctx.db.orderDataReader.identity.find(ctx.sender)
      ? [...ctx.db.orderItemStock.iter()].map(row => ({ order_line_id: row.orderItemId,
        warehouse_id: row.warehouseId, quantity: row.quantity })) : []);
