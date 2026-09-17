import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { z } from 'zod';
import { STACK_BENCH_ROOT } from '../package-root.js';
import { checkoutId, checkoutMinor, orderCheckoutStateSchema } from './checkout-state.js';

export function orderDataError(message: string, cause?: unknown): Error {
  return Object.assign(new Error(message, { cause }), { orderDataInterface: true });
}

const id = z.unknown().transform((value, ctx) => {
  try { return checkoutId(value); } catch { ctx.addIssue({ code: 'custom', message: 'invalid identifier' }); return z.NEVER; }
});
const money = z.unknown().transform((value, ctx) => {
  try { return checkoutMinor(value); } catch { ctx.addIssue({ code: 'custom', message: 'invalid money value' }); return z.NEVER; }
});
const integer = z.number().int().safe();
const tablesSchema = z.object({
  order_account: z.array(z.object({ id, username: z.string() })),
  item: z.array(z.object({ id, name: z.string(), price: money })),
  warehouse: z.array(z.object({ id })),
  stock: z.array(z.object({ item_id: id, warehouse_id: id, quantity: integer })),
  order_cart: z.array(z.object({ account_id: id, item_id: id, quantity: integer })),
  order_reservation: z.array(z.object({ account_id: id, item_id: id, warehouse_id: id, quantity: integer })),
  order_header: z.array(z.object({ id, account_id: id, total: money, refunded: money, status: z.string() })),
  order_line: z.array(z.object({ id, order_id: id, item_id: id, quantity: integer, unit_price: money })),
  order_allocation: z.array(z.object({ order_line_id: id, warehouse_id: id, quantity: integer })),
});

// Native snapshots only. The validation schema also owns the fixed query columns.
export const ORDER_DATA_COLUMNS = Object.fromEntries(Object.entries(tablesSchema.shape)
  .map(([table, rows]) => [table, Object.keys(rows.element.shape)])) as Record<keyof typeof tablesSchema.shape, string[]>;

export function readOrderDataSnapshot(raw: unknown, account: string, item: string) {
  const parsed = tablesSchema.safeParse(raw);
  if (!parsed.success) throw orderDataError('order data has missing fields or invalid values', parsed.error);
  const tables = parsed.data;
  // Ambiguous parents must never duplicate, drop, or silently reassign stored effects.
  for (const name of ['order_account', 'item', 'warehouse', 'order_header', 'order_line'] as const) {
    const rows = tables[name];
    if (new Set(rows.map(row => row.id)).size !== rows.length) throw orderDataError(`duplicate ids in ${name}`);
  }
  const accounts = tables.order_account.filter(row => row.username === account);
  const items = tables.item.filter(row => row.name === item);
  if (accounts.length !== 1 || items.length !== 1) throw orderDataError('order account or item is missing or ambiguous');
  const accountId = accounts[0]!.id, itemId = items[0]!.id;
  const orders = new Set(tables.order_header.map(row => row.id));
  const lines = new Set(tables.order_line.map(row => row.id));
  if (tables.order_header.some(row => !tables.order_account.some(account => account.id === row.account_id))
    || tables.order_cart.some(row => !tables.order_account.some(account => account.id === row.account_id))
    || tables.order_reservation.some(row => !tables.order_account.some(account => account.id === row.account_id))
    || [...tables.order_line, ...tables.order_cart, ...tables.order_reservation, ...tables.stock].some(row => !tables.item.some(item => item.id === row.item_id))
    || [...tables.stock, ...tables.order_allocation, ...tables.order_reservation].some(row => !tables.warehouse.some(warehouse => warehouse.id === row.warehouse_id))) {
    throw orderDataError('order account, item or warehouse link is missing');
  }
  const state = orderCheckoutStateSchema.parse({
    accountId, itemId, priceMinor: items[0]!.price,
    cart: tables.order_cart.filter(row => row.account_id === accountId)
      .map(row => ({ itemId: row.item_id, quantity: row.quantity })),
    stock: tables.stock.filter(row => row.item_id === itemId)
      .map(row => ({ warehouseId: row.warehouse_id, quantity: row.quantity })),
    orders: tables.order_header.map(order => ({ id: order.id, accountId: order.account_id,
      totalMinor: order.total, refundedMinor: order.refunded, status: order.status,
      lines: tables.order_line.filter(line => line.order_id === order.id).map(line => ({
        itemId: line.item_id, quantity: line.quantity, priceMinor: line.unit_price,
        allocations: tables.order_allocation.filter(row => row.order_line_id === line.id)
          .map(row => ({ warehouseId: row.warehouse_id, quantity: row.quantity })),
      })),
    })),
    orphanOrderLines: tables.order_line.filter(row => !orders.has(row.order_id)).length,
    orphanAllocations: tables.order_allocation.filter(row => !lines.has(row.order_line_id)).length,
    payments: [], reservations: tables.order_reservation.filter(row => row.account_id === accountId)
      .map(row => ({ itemId: row.item_id, warehouseId: row.warehouse_id, quantity: row.quantity })),
  });
  const contract = readFileSync(join(STACK_BENCH_ROOT, 'tracks/ecommerce/contracts/order-data.md'), 'utf8').replaceAll('\r\n', '\n');
  return { state, scope: 'orders' as const, storage: 'order-data' as const,
    schemaSha256: { contract: createHash('sha256').update(contract).digest('hex') } };
}
