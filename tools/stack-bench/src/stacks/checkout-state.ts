import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';
import { STACK_BENCH_ROOT } from '../package-root.js';

const id = z.string().min(1);
const integer = z.number().int().safe();
const allocation = z.strictObject({ warehouseId: id, quantity: integer });
const line = z.strictObject({ itemId: id, quantity: integer, priceMinor: integer, allocations: z.array(allocation) });
export const checkoutStateSchema = z.strictObject({
  accountId: id,
  itemId: id,
  priceMinor: integer,
  cart: z.array(z.strictObject({ itemId: id, quantity: integer })),
  stock: z.array(z.strictObject({ warehouseId: id, quantity: integer })).min(1),
  reservations: z.array(z.strictObject({ itemId: id, warehouseId: id, quantity: integer })),
  orders: z.array(z.strictObject({ id, accountId: id, totalMinor: integer, status: z.string(), lines: z.array(line) })),
  payments: z.array(z.strictObject({ id, orderId: id, amountMinor: integer, status: z.string() })),
  orphanOrderLines: integer,
});
export type CheckoutState = z.infer<typeof checkoutStateSchema>;

// Row order is not business state. Preserve duplicates while normalizing nesting.
function normalized(state: CheckoutState): CheckoutState {
  const rows = <T>(values: readonly T[]) => [...values].sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  return { ...state, cart: rows(state.cart), stock: rows(state.stock), reservations: rows(state.reservations),
    payments: rows(state.payments), orders: rows(state.orders.map(row => ({ ...row,
      lines: rows(row.lines.map(line => ({ ...line, allocations: rows(line.allocations) }))),
    }))) };
}

// One-unit, non-credit purchases on idle reference apps. Histories identify the
// buyer, not a durable request ID: reconcile per-buyer counts, not request identity.
export function purchaseDifferences(before: CheckoutState, after: CheckoutState,
  accepted: ReadonlyMap<string, number>, restocked: ReadonlyMap<string, number>) {
  const differences: Array<{ control: string; observed: number; expected: number }> = [];
  const check = (control: string, observed: number, expected: number) => {
    if (observed !== expected) differences.push({ control, observed, expected });
  };
  const oldOrders = new Set(before.orders.map(row => row.id));
  const oldPayments = new Set(before.payments.map(row => row.id));
  const orders = after.orders.filter(row => !oldOrders.has(row.id));
  const payments = after.payments.filter(row => !oldPayments.has(row.id));
  const expected = structuredClone(before);
  expected.orders.push(...orders);
  expected.payments.push(...payments);
  for (const [account, count] of accepted) {
    check(`purchase orders for account ${account}`, orders.filter(row => row.accountId === account).length, count);
  }
  check('purchase orders for unexpected accounts', orders.filter(row => !accepted.has(row.accountId)).length, 0);
  for (const order of orders) {
    check('purchase order status', Number(order.status === 'pending'), 1);
    check('purchase order total', order.totalMinor, before.priceMinor);
    check('purchase order line count', order.lines.length, 1);
    for (const line of order.lines) {
      check('purchase item', Number(line.itemId === before.itemId), 1);
      check('purchase quantity', line.quantity, 1);
      check('purchase unit price', line.priceMinor, before.priceMinor);
      check('purchase allocation count', line.allocations.length, 1);
      for (const allocation of line.allocations) {
        check('purchase allocated quantity', allocation.quantity, 1);
        const stock = expected.stock.find(row => row.warehouseId === allocation.warehouseId);
        check('purchase allocation warehouse', Number(Boolean(stock)), 1);
        if (stock) stock.quantity = integer.parse(stock.quantity - allocation.quantity);
      }
    }
    const paid = payments.filter(row => row.orderId === order.id);
    check('purchase payment count per order', paid.length, 1);
    for (const payment of paid) {
      check('purchase payment amount', payment.amountMinor, before.priceMinor);
      check('purchase payment status', Number(payment.status === 'paid'), 1);
    }
  }
  check('purchase orphan payments', payments.filter(row => !orders.some(order => order.id === row.orderId)).length, 0);
  for (const [warehouse, quantity] of restocked) {
    const stock = expected.stock.find(row => row.warehouseId === warehouse);
    if (!stock || !Number.isSafeInteger(quantity) || quantity < 1) throw new Error('invalid restock expectation');
    stock.quantity = integer.parse(stock.quantity + quantity);
  }
  for (const state of [before, after]) {
    check('purchase orphan order lines', state.orphanOrderLines, 0);
    check('purchase negative stock', state.stock.filter(row => row.quantity < 0).length, 0);
    check('purchase duplicate warehouse', state.stock.length - new Set(state.stock.map(row => row.warehouseId)).size, 0);
    check('purchase duplicate order', state.orders.length - new Set(state.orders.map(row => row.id)).size, 0);
    check('purchase duplicate payment', state.payments.length - new Set(state.payments.map(row => row.id)).size, 0);
  }
  const wanted = normalized(expected), observed = normalized(after);
  for (const key of Object.keys(wanted) as Array<keyof CheckoutState>) {
    check(`purchase ${key}`, Number(isDeepStrictEqual(observed[key], wanted[key])), 1);
  }
  return differences;
}

// These readers are for audited reference schemas, not a schema discovery system.
// Saved model apps need their own verified mapping before this diagnostic applies.
export function verifyCheckoutSchema(backend: string, app: string, files: readonly string[]): Record<string, string> {
  return Object.fromEntries(files.map(file => {
    const read = (root: string) => readFileSync(join(root, file), 'utf8').replaceAll('\r\n', '\n');
    const actual = read(app);
    if (actual !== read(join(STACK_BENCH_ROOT, 'reference-apps/ecommerce', backend))) {
      throw new Error(`checkout state reader has no verified mapping for ${backend}: ${file}`);
    }
    return [file, createHash('sha256').update(actual).digest('hex')];
  }));
}

export function checkoutId(value: unknown): string {
  if (typeof value === 'string' && value) return value;
  if (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0) return String(value);
  throw new Error('checkout state reader received an invalid or inexact identifier');
}

export function checkoutMinor(value: unknown): number {
  if (typeof value !== 'number' && (typeof value !== 'string' || !/^-?\d+(?:\.\d+)?$/.test(value))) {
    throw new Error('checkout state reader received an invalid amount');
  }
  const scaled = Number(value) * 100;
  const minor = Math.round(scaled);
  if (!Number.isSafeInteger(minor) || Math.abs(scaled - minor) > 0.000001) {
    throw new Error('checkout state amount is not an exact minor-unit amount');
  }
  return minor;
}

export function checkoutDifferences(before: CheckoutState, prepared: CheckoutState, after: CheckoutState,
  quantity: number, allowUnchanged = false): Array<{ control: string; observed: number; expected: number }> {
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
  for (const state of [before, prepared, after]) {
    check('order lines without an order', state.orphanOrderLines, 0);
    check('duplicate stock warehouse rows', state.stock.length - new Set(state.stock.map(row => row.warehouseId)).size, 0);
    check('negative stored stock rows', state.stock.filter(row => row.quantity < 0).length, 0);
    check('duplicate order ids', state.orders.length - new Set(state.orders.map(row => row.id)).size, 0);
    check('duplicate payment ids', state.payments.length - new Set(state.payments.map(row => row.id)).size, 0);
  }
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
  // A crash can lose the response. Only an unacknowledged checkout may leave
  // its complete prepared state unchanged; an error response is not rollback proof.
  if (allowUnchanged && isDeepStrictEqual(normalized(prepared), normalized(after))) return differences;
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
  same('stock warehouses preserved', sorted(after.stock.map(row => row.warehouseId)), sorted(before.stock.map(row => row.warehouseId)));
  check('stored stock consumed by one checkout', integer.parse(total(before.stock) - total(after.stock)), quantity);
  return differences;
}

// Reference diagnostic: the account has one pending, non-credit, single-product
// order. Paid records remain history; cancellation removes the order from revenue.
// Other storage conventions need a verified mapping before this can score them.
export function cancellationDifferences(before: CheckoutState, after: CheckoutState):
  Array<{ control: string; observed: number; expected: number }> {
  const orders = before.orders.filter(order => order.accountId === before.accountId);
  const order = orders[0];
  if (orders.length !== 1 || !order || order.status !== 'pending' || !order.lines.length
    || order.lines.some(line => line.itemId !== before.itemId || line.quantity <= 0
      || !line.allocations.length || line.allocations.some(row => row.quantity <= 0
        || !before.stock.some(stock => stock.warehouseId === row.warehouseId))
      || line.allocations.reduce((sum, row) => integer.parse(sum + row.quantity), 0) !== line.quantity)) {
    throw new Error('cancellation diagnostic requires one pending single-product order with verified allocations');
  }
  const expected = structuredClone(before);
  expected.orders.find(row => row.id === order.id)!.status = 'cancelled';
  for (const stock of expected.stock) {
    for (const allocation of order.lines.flatMap(line => line.allocations)) {
      if (allocation.warehouseId === stock.warehouseId) stock.quantity = integer.parse(stock.quantity + allocation.quantity);
    }
  }
  // Database row order is not part of cancellation. Keep nested allocation and
  // line order independent too, while retaining duplicates for comparison.
  const wanted = normalized(expected), observed = normalized(after);
  return (Object.keys(wanted) as Array<keyof CheckoutState>)
    .filter(key => !isDeepStrictEqual(observed[key], wanted[key]))
    .map(key => ({ control: `cancellation ${key}`, observed: 0, expected: 1 }));
}
