import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';
import { STACK_BENCH_ROOT } from '../package-root.js';
import { addressBookSchemaSource } from '../references/address-book-migration.js';

const id = z.string().min(1);
const integer = z.number().int().safe();
const allocation = z.strictObject({ warehouseId: id, quantity: integer });
const line = z.strictObject({ itemId: id, quantity: integer, priceMinor: integer, allocations: z.array(allocation) });
export const checkoutStateSchema = z.strictObject({
  accountId: id,
  itemId: id,
  priceMinor: integer,
  cart: z.array(z.strictObject({ itemId: id, quantity: integer })),
  stock: z.array(z.strictObject({ warehouseId: id, quantity: integer })),
  reservations: z.array(z.strictObject({ itemId: id, warehouseId: id, quantity: integer })),
  orders: z.array(z.strictObject({ id, accountId: id, totalMinor: integer, status: z.string(), lines: z.array(line) })),
  payments: z.array(z.strictObject({ id, orderId: id, amountMinor: integer, status: z.string() })),
  orphanOrderLines: integer,
});
export type CheckoutState = z.infer<typeof checkoutStateSchema>;
export class CheckoutDataError extends Error {}

export function parseCheckoutState(value: unknown, checkoutInterface?: 'ecommerce-checkout-v1'): CheckoutState {
  if (!value || typeof value !== 'object' || Object.keys(checkoutStateSchema.shape).some(key => !(key in value))) {
    throw new Error('checkout reader returned an incomplete response');
  }
  try { return checkoutStateSchema.parse(value); }
  catch (error) {
    if (checkoutInterface && error instanceof z.ZodError) {
      throw new CheckoutDataError(`stored checkout data violates the existing interface: ${error.message}`, { cause: error });
    }
    throw error;
  }
}

// Version of the fixed, original business-storage interface. Each adapter still
// validates its live query result; this identity is not a source-code whitelist.
export function checkoutInterfaceIdentity(backend: string): Record<string, string> {
  return { 'ecommerce-checkout-v1': createHash('sha256').update(`ecommerce-checkout-v1:${backend}`).digest('hex') };
}

// These readers are for audited reference schemas, not a schema discovery system.
// Saved model apps need their own verified mapping before this diagnostic applies.
export function verifyCheckoutSchema(backend: string, app: string, files: readonly string[],
  { addressBookMigration = false }: { addressBookMigration?: boolean } = {}): Record<string, string> {
  return Object.fromEntries(files.map(file => {
    const read = (root: string) => readFileSync(join(root, file), 'utf8').replaceAll('\r\n', '\n');
    const actual = read(app);
    const reference = read(join(STACK_BENCH_ROOT, 'reference-apps/ecommerce', backend));
    const expected = addressBookMigration && backend === 'spacetime' && file === 'backend/spacetimedb/src/schema.ts'
      ? addressBookSchemaSource(reference) : reference;
    if (actual !== expected) {
      throw new Error(`checkout state reader has no verified mapping for ${backend}: ${file}`);
    }
    return [file, createHash('sha256').update(actual).digest('hex')];
  }));
}

export function checkoutId(value: unknown): string {
  if (typeof value === 'string' && value) return value;
  if (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0) return String(value);
  if (typeof value === 'number') throw new Error('checkout reader cannot represent the observed identifier exactly');
  throw new CheckoutDataError('checkout state reader received an invalid or inexact identifier');
}

export function checkoutMinor(value: unknown): number {
  if (typeof value !== 'number' && (typeof value !== 'string' || !/^-?\d+(?:\.\d+)?$/.test(value))) {
    throw new CheckoutDataError('checkout state reader received an invalid amount');
  }
  const scaled = Number(value) * 100;
  const minor = Math.round(scaled);
  if (!Number.isSafeInteger(minor) || Math.abs(scaled - minor) > 0.000001) {
    throw new CheckoutDataError('checkout state amount is not an exact minor-unit amount');
  }
  return minor;
}

export function checkoutDifferences(before: CheckoutState, prepared: CheckoutState, after: CheckoutState,
  quantity: number): Array<{ control: string; observed: number; expected: number }> {
  if (!Number.isSafeInteger(quantity) || quantity <= 0 || !Number.isSafeInteger(before.priceMinor * quantity)) {
    throw new Error('checkout expectation is not an exact quantity and amount');
  }
  const differences: Array<{ control: string; observed: number; expected: number }> = [];
  const check = (control: string, observed: number, expected: number) => {
    if (observed !== expected) differences.push({ control, observed, expected });
  };
  const same = (control: string, observed: unknown, expected: unknown) =>
    check(control, Number(isDeepStrictEqual(observed, expected)), 1);
  const sorted = <T>(values: readonly T[]) => [...values].sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  const total = (rows: readonly { quantity: number }[]) => rows.reduce((sum, row) => integer.parse(sum + row.quantity), 0);
  check('initial stock available for checkout', Number(before.stock.length > 0), 1);
  for (const state of [prepared, after]) {
    same('checkout account and item identity', [state.accountId, state.itemId, state.priceMinor],
      [before.accountId, before.itemId, before.priceMinor]);
  }
  check('initial cart lines', before.cart.length, 0);
  check('initial cart reservations', before.reservations.length, 0);
  same('prepared cart', prepared.cart, [{ itemId: before.itemId, quantity }]);
  same('orders unchanged during cart preparation', sorted(prepared.orders), sorted(before.orders));
  same('payments unchanged during cart preparation', sorted(prepared.payments), sorted(before.payments));
  same('stock warehouses preserved during preparation', sorted(prepared.stock.map(row => row.warehouseId)), sorted(before.stock.map(row => row.warehouseId)));
  check('invalid reservation rows', prepared.reservations.filter(row => row.itemId !== before.itemId
    || row.quantity <= 0 || !before.stock.some(stock => stock.warehouseId === row.warehouseId)).length, 0);
  if (prepared.reservations.length) {
    check('prepared reserved quantity', total(prepared.reservations), quantity);
  }
  for (const stock of before.stock) {
    const reserved = total(prepared.reservations.filter(row => row.warehouseId === stock.warehouseId));
    const readyStock = prepared.stock.find(row => row.warehouseId === stock.warehouseId);
    const finalStock = after.stock.find(row => row.warehouseId === stock.warehouseId);
    if (readyStock) check(`prepared stock in warehouse ${stock.warehouseId}`, readyStock.quantity, stock.quantity - reserved);
    if (finalStock) {
      check('warehouses with an unexpected stock increase', Number(finalStock.quantity > stock.quantity), 0);
      if (prepared.reservations.length) check(`reserved stock in warehouse ${stock.warehouseId}`, finalStock.quantity, stock.quantity - reserved);
    }
  }
  check('remaining cart lines after checkout', after.cart.length, 0);
  check('remaining reservations after checkout', after.reservations.length, 0);
  const priorOrders = new Set(before.orders.map(order => order.id));
  const priorPayments = new Set(before.payments.map(payment => payment.id));
  same('prior orders preserved', sorted(after.orders.filter(order => priorOrders.has(order.id))), sorted(before.orders));
  same('prior payments preserved', sorted(after.payments.filter(payment => priorPayments.has(payment.id))), sorted(before.payments));
  const orders = after.orders.filter(order => !priorOrders.has(order.id));
  const payments = after.payments.filter(payment => !priorPayments.has(payment.id));
  check('orders created by one checkout', orders.length, 1);
  check('payments created by one checkout', payments.length, 1);
  const order = orders[0];
  if (order) {
    same('checkout order owner', order.accountId, before.accountId);
    same('checkout order status', order.status, 'pending');
    check('checkout order total in minor units', order.totalMinor, before.priceMinor * quantity);
    // Relational orders may split one product across warehouse allocation lines.
    check('checkout order quantity', total(order.lines), quantity);
    check('unexpected checkout order lines', order.lines.filter(line => line.itemId !== before.itemId
      || line.priceMinor !== before.priceMinor || line.quantity <= 0).length, 0);
    for (const line of order.lines) {
      check('allocated order quantity', total(line.allocations), line.quantity);
      check('invalid order allocations', line.allocations.filter(row => row.quantity <= 0
        || !before.stock.some(stock => stock.warehouseId === row.warehouseId)).length, 0);
    }
    for (const stock of before.stock) {
      const allocated = total(order.lines.flatMap(line => line.allocations).filter(row => row.warehouseId === stock.warehouseId));
      const finalStock = after.stock.find(row => row.warehouseId === stock.warehouseId);
      if (finalStock) check(`order allocation in warehouse ${stock.warehouseId}`, stock.quantity - finalStock.quantity, allocated);
    }
    if (payments[0]) {
      same('checkout payment order', payments[0].orderId, order.id);
      same('checkout payment status', payments[0].status, 'paid');
      check('checkout payment in minor units', payments[0].amountMinor, before.priceMinor * quantity);
    }
  }
  for (const state of [before, prepared, after]) {
    check('order lines without an order', state.orphanOrderLines, 0);
    check('duplicate stock warehouse rows', state.stock.length - new Set(state.stock.map(row => row.warehouseId)).size, 0);
    check('negative stored stock rows', state.stock.filter(row => row.quantity < 0).length, 0);
    check('duplicate order ids', state.orders.length - new Set(state.orders.map(row => row.id)).size, 0);
    check('duplicate payment ids', state.payments.length - new Set(state.payments.map(row => row.id)).size, 0);
  }
  same('stock warehouses preserved', sorted(after.stock.map(row => row.warehouseId)), sorted(before.stock.map(row => row.warehouseId)));
  check('stored stock consumed by one checkout', integer.parse(total(before.stock) - total(after.stock)), quantity);
  return differences;
}
