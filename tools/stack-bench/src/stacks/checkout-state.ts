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
  // Empty query results are evidence too. Validate setup separately from reads.
  // Reference-specific readers select one item; native order-data reads retain all items.
  stock: z.array(z.strictObject({ itemId: id.optional(), warehouseId: id, quantity: integer })),
  reservations: z.array(z.strictObject({ itemId: id, warehouseId: id, quantity: integer })),
  orders: z.array(z.strictObject({ id, accountId: id, totalMinor: integer, refundedMinor: integer.nullable().optional(), status: z.string(), lines: z.array(line) })),
  payments: z.array(z.strictObject({ id, orderId: id, amountMinor: integer, status: z.string() })),
  refunds: z.array(z.strictObject({ id: id.optional(), orderId: id, accountId: id, amountMinor: integer })).optional(),
  orphanOrderLines: integer,
  orphanAllocations: integer.optional(),
  orphanRefunds: integer.optional(),
});
export type CheckoutState = z.infer<typeof checkoutStateSchema>;
function stockItem(state: CheckoutState, row: CheckoutState['stock'][number]) { return row.itemId ?? state.itemId; }
function stockKey(state: CheckoutState, row: CheckoutState['stock'][number]) {
  return JSON.stringify([stockItem(state, row), row.warehouseId]);
}
// Order accounting does not require a separate payment feature. Reservations,
// when present, explain stock already held during cart preparation.
export const orderCheckoutStateSchema = checkoutStateSchema.extend({
  orders: z.array(checkoutStateSchema.shape.orders.element.extend({ refundedMinor: integer.nullable() })),
  orphanAllocations: integer,
  payments: checkoutStateSchema.shape.payments.length(0),
});

// Row order is not business state. Preserve duplicates while normalizing nesting.
function normalized(state: CheckoutState): CheckoutState {
  const rows = <T>(values: readonly T[]) => [...values].sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  return { ...state, cart: rows(state.cart), stock: rows(state.stock), reservations: rows(state.reservations),
    ...(state.refunds ? { refunds: rows(state.refunds) } : {}),
    payments: rows(state.payments), orders: rows(state.orders.map(row => ({ ...row,
      lines: rows(row.lines.map(line => ({ ...line, allocations: rows(line.allocations) }))),
    }))) };
}

// One-unit, non-credit purchases on idle reference apps. Histories identify the
// buyer, not a durable request ID: reconcile per-buyer counts, not request identity.
export function purchaseDifferences(before: CheckoutState, after: CheckoutState,
  accepted: ReadonlyMap<string, number>, restocked: ReadonlyMap<string, number>) {
  return comparePurchases(before, after, accepted, restocked, true);
}

export function orderPurchaseDifferences(before: CheckoutState, after: CheckoutState,
  accepted: ReadonlyMap<string, number>, restocked: ReadonlyMap<string, number>, warehouses = true) {
  for (const state of [before, after]) orderCheckoutStateSchema.parse(state);
  return comparePurchases(before, after, accepted, restocked, false, warehouses);
}

function comparePurchases(before: CheckoutState, after: CheckoutState,
  accepted: ReadonlyMap<string, number>, restocked: ReadonlyMap<string, number>, requirePayment: boolean, warehouses = true) {
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
    if (!requirePayment) check('purchase refunded amount', order.refundedMinor ?? -1, 0);
    check('purchase order line count', order.lines.length, 1);
    for (const line of order.lines) {
      check('purchase item', Number(line.itemId === before.itemId), 1);
      check('purchase quantity', line.quantity, 1);
      check('purchase unit price', line.priceMinor, before.priceMinor);
      if (warehouses) check('purchase allocation count', line.allocations.length, 1);
      for (const allocation of line.allocations) {
        check('purchase allocated quantity', allocation.quantity, 1);
        const stock = expected.stock.find(row => stockItem(expected, row) === line.itemId && row.warehouseId === allocation.warehouseId);
        check('purchase allocation warehouse', Number(Boolean(stock)), 1);
        if (stock) stock.quantity = integer.parse(stock.quantity - allocation.quantity);
      }
    }
    const paid = payments.filter(row => row.orderId === order.id);
    if (requirePayment) check('purchase payment count per order', paid.length, 1);
    for (const payment of paid) {
      check('purchase payment amount', payment.amountMinor, before.priceMinor);
      check('purchase payment status', Number(payment.status === 'paid'), 1);
    }
  }
  check('purchase orphan payments', payments.filter(row => !orders.some(order => order.id === row.orderId)).length, 0);
  for (const [warehouse, quantity] of restocked) {
    const stock = expected.stock.find(row => stockItem(expected, row) === before.itemId && row.warehouseId === warehouse);
    if (!stock || !Number.isSafeInteger(quantity) || quantity < 1) throw new Error('invalid restock expectation');
    stock.quantity = integer.parse(stock.quantity + quantity);
  }
  for (const state of [before, after]) {
    check('purchase orphan order lines', state.orphanOrderLines, 0);
    if (state.orphanAllocations !== undefined) check('purchase orphan allocations', state.orphanAllocations, 0);
    check('purchase negative stock', state.stock.filter(row => row.quantity < 0).length, 0);
    check('purchase duplicate warehouse', state.stock.length - new Set(state.stock.map(row => stockKey(state, row))).size, 0);
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
  return compareCheckout(before, prepared, after, quantity, allowUnchanged, true);
}

export type CheckoutLines = readonly { itemId: string; priceMinor: number; quantity: number }[];

export function orderCheckoutDifferences(before: CheckoutState, prepared: CheckoutState, after: CheckoutState,
  quantity: number | CheckoutLines, allowUnchanged = false, warehouses = true): Array<{ control: string; observed: number; expected: number }> {
  for (const state of [before, prepared, after]) orderCheckoutStateSchema.parse(state);
  return compareCheckout(before, prepared, after, quantity, allowUnchanged, false, warehouses);
}

// Separate the interrupted transaction from history that was already committed.
// Durability adds only the requirement introduced by an acknowledgement; a
// malformed new order remains an atomicity failure even when acknowledged.
export function checkoutCrashDifferences(before: CheckoutState, after: CheckoutState, confirmed: boolean,
  compare: (state: CheckoutState, allowUnchanged: boolean) => ReturnType<typeof checkoutDifferences>) {
  const current = structuredClone(after);
  const durability: ReturnType<typeof checkoutDifferences> = [];
  const initial = normalized(before), recovered = normalized(after);
  for (const key of ['orders', 'payments'] as const) {
    const ids = new Set(initial[key].map(row => row.id));
    const retained = recovered[key].filter(row => ids.has(row.id));
    if (!isDeepStrictEqual(retained, initial[key])) {
      durability.push({ control: `acknowledged ${key} preserved`, observed: 0, expected: 1 });
    }
  }
  const orderIds = new Set(before.orders.map(row => row.id));
  const paymentIds = new Set(before.payments.map(row => row.id));
  current.orders = [...before.orders, ...after.orders.filter(row => !orderIds.has(row.id))];
  current.payments = [...before.payments, ...after.payments.filter(row => !paymentIds.has(row.id))];
  if (!isDeepStrictEqual(recovered.refunds, initial.refunds)) {
    durability.push({ control: 'recorded refund history preserved', observed: 0, expected: 1 });
  }
  if (before.refunds) current.refunds = before.refunds;
  else delete current.refunds;
  const atomicity = compare(current, true);
  if (confirmed) {
    durability.push(...compare(current, false).filter(row => !atomicity.some(value => isDeepStrictEqual(value, row))));
  }
  return { atomicity, durability };
}

function compareCheckout(before: CheckoutState, prepared: CheckoutState, after: CheckoutState,
  expectation: number | CheckoutLines, allowUnchanged: boolean, requirePayment: boolean, warehouses = true) {
  if (typeof expectation !== 'number' && warehouses
    && [before, prepared, after].some(state => state.stock.some(row => !row.itemId))) {
    throw new Error('multi-item checkout requires item-keyed stock evidence');
  }
  if (!warehouses && [before, prepared, after].some(state => state.stock.length || state.reservations.length
    || state.orders.some(order => order.lines.some(line => line.allocations.length)))) {
    throw new Error('checkout state includes warehouse data outside the declared scope');
  }
  before = normalized(before); prepared = normalized(prepared); after = normalized(after);
  const expectedLines: CheckoutLines = typeof expectation === 'number'
    ? [{ itemId: before.itemId, priceMinor: before.priceMinor, quantity: expectation }] : expectation;
  const quantity = expectedLines.reduce((sum, row) => sum + row.quantity, 0);
  const amount = expectedLines.reduce((sum, row) => sum + row.priceMinor * row.quantity, 0);
  if (!expectedLines.length || new Set(expectedLines.map(row => row.itemId)).size !== expectedLines.length
    || expectedLines.some(row => !row.itemId || !Number.isSafeInteger(row.quantity) || row.quantity <= 0
      || !Number.isSafeInteger(row.priceMinor) || row.priceMinor < 0 || !Number.isSafeInteger(row.priceMinor * row.quantity))
    || !Number.isSafeInteger(quantity) || !Number.isSafeInteger(amount)) {
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
    if (state.orphanAllocations !== undefined) check('allocations without an order line', state.orphanAllocations, 0);
    if (state.orphanRefunds !== undefined) check('refunds without an order', state.orphanRefunds, 0);
    check('duplicate stock warehouse rows', state.stock.length - new Set(state.stock.map(row => stockKey(state, row))).size, 0);
    check('negative stored stock rows', state.stock.filter(row => row.quantity < 0).length, 0);
    check('duplicate order ids', state.orders.length - new Set(state.orders.map(row => row.id)).size, 0);
    check('duplicate payment ids', state.payments.length - new Set(state.payments.map(row => row.id)).size, 0);
  }
  for (const state of [prepared, after]) {
    same('checkout account and item identity', [state.accountId, state.itemId, state.priceMinor],
      [before.accountId, before.itemId, before.priceMinor]);
  }
  check('initial cart lines', before.cart.length, 0);
  if (warehouses) for (const wanted of expectedLines) {
    check(`initial stock warehouses for item ${wanted.itemId}`, Number(before.stock.some(row => stockItem(before, row) === wanted.itemId)), 1);
  }
  check('initial cart reservations', before.reservations.length, 0);
  same('prepared cart', prepared.cart, sorted(expectedLines.map(({ itemId, quantity }) => ({ itemId, quantity }))));
  same('orders unchanged during cart preparation', sorted(prepared.orders), sorted(before.orders));
  same('payments unchanged during cart preparation', sorted(prepared.payments), sorted(before.payments));
  same('refunds unchanged during cart preparation', prepared.refunds, before.refunds);
  same('refunds unchanged after checkout', after.refunds, before.refunds);
  same('stock warehouses preserved during preparation', sorted(prepared.stock.map(row => stockKey(prepared, row))), sorted(before.stock.map(row => stockKey(before, row))));
  check('invalid reservation rows', prepared.reservations.filter(row => !expectedLines.some(line => line.itemId === row.itemId)
    || row.quantity <= 0 || !before.stock.some(stock => stockItem(before, stock) === row.itemId && stock.warehouseId === row.warehouseId)).length, 0);
  if (prepared.reservations.length) {
    const reserved = total(prepared.reservations);
    if (requirePayment) check('prepared reserved quantity', reserved, quantity);
    else for (const wanted of expectedLines) {
      check(`reserved quantity exceeds prepared cart for item ${wanted.itemId}`,
        Number(total(prepared.reservations.filter(row => row.itemId === wanted.itemId)) > wanted.quantity), 0);
    }
  }
  for (const stock of before.stock) {
    const itemId = stockItem(before, stock);
    const reserved = total(prepared.reservations.filter(row => row.itemId === itemId && row.warehouseId === stock.warehouseId));
    const readyStock = prepared.stock.find(row => stockKey(prepared, row) === stockKey(before, stock));
    const finalStock = after.stock.find(row => stockKey(after, row) === stockKey(before, stock));
    if (readyStock) check(`prepared stock in warehouse ${stock.warehouseId}`, readyStock.quantity, stock.quantity - reserved);
    if (finalStock) {
      check('warehouses with an unexpected stock increase', Number(finalStock.quantity > stock.quantity), 0);
      if (requirePayment && prepared.reservations.length) check(`reserved stock in warehouse ${stock.warehouseId}`, finalStock.quantity, stock.quantity - reserved);
    }
  }
  // A crash can lose the response. Only an unacknowledged checkout may leave
  // its complete prepared state unchanged; an error response is not rollback proof.
  if (allowUnchanged && isDeepStrictEqual(prepared, after)) return differences;
  check('remaining cart lines after checkout', after.cart.length, 0);
  check('remaining reservations after checkout', after.reservations.length, 0);
  const priorOrders = new Set(before.orders.map(order => order.id));
  const priorPayments = new Set(before.payments.map(payment => payment.id));
  same('prior orders preserved', sorted(after.orders.filter(order => priorOrders.has(order.id))), sorted(before.orders));
  same('prior payments preserved', sorted(after.payments.filter(payment => priorPayments.has(payment.id))), sorted(before.payments));
  const orders = after.orders.filter(order => !priorOrders.has(order.id));
  const payments = after.payments.filter(payment => !priorPayments.has(payment.id));
  check('orders created by one checkout', orders.length, 1);
  if (requirePayment) check('payments created by one checkout', payments.length, 1);
  const order = orders[0];
  if (order) {
    same('checkout order owner', order.accountId, before.accountId);
    same('checkout order status', order.status, 'pending');
    check('checkout order total in minor units', order.totalMinor, amount);
    if (typeof order.refundedMinor === 'number') check('checkout order refunded amount', order.refundedMinor, 0);
    // Relational orders may split one product across warehouse allocation lines.
    check('checkout order quantity', total(order.lines), quantity);
    check('unexpected checkout order lines', order.lines.filter(line => line.quantity <= 0
      || !expectedLines.some(wanted => wanted.itemId === line.itemId && wanted.priceMinor === line.priceMinor)).length, 0);
    for (const wanted of expectedLines) {
      check(`checkout quantity for item ${wanted.itemId}`, total(order.lines.filter(line => line.itemId === wanted.itemId)), wanted.quantity);
    }
    for (const line of order.lines) {
      if (warehouses) check('allocated order quantity', total(line.allocations), line.quantity);
      check('invalid order allocations', line.allocations.filter(row => row.quantity <= 0
        || !before.stock.some(stock => stockItem(before, stock) === line.itemId && stock.warehouseId === row.warehouseId)).length, 0);
    }
    for (const stock of before.stock) {
      const allocated = total(order.lines.filter(line => line.itemId === stockItem(before, stock))
        .flatMap(line => line.allocations).filter(row => row.warehouseId === stock.warehouseId));
      const finalStock = after.stock.find(row => stockKey(after, row) === stockKey(before, stock));
      if (finalStock) check(`order allocation in warehouse ${stock.warehouseId}`, stock.quantity - finalStock.quantity, allocated);
    }
    if (payments[0]) {
      same('checkout payment order', payments[0].orderId, order.id);
      same('checkout payment status', payments[0].status, 'paid');
      check('checkout payment in minor units', payments[0].amountMinor, amount);
    }
  }
  same('stock warehouses preserved', sorted(after.stock.map(row => stockKey(after, row))), sorted(before.stock.map(row => stockKey(before, row))));
  if (warehouses) check('stored stock consumed by one checkout', integer.parse(total(before.stock) - total(after.stock)), quantity);
  return differences;
}

// Reference diagnostic: the account has one pending, non-credit, single-product
// order. Paid records remain history; cancellation removes the order from revenue.
// Other storage conventions need a verified mapping before this can score them.
export function cancellationDifferences(before: CheckoutState, after: CheckoutState):
  Array<{ control: string; observed: number; expected: number }> {
  return compareCancellation(before, after, false);
}

export function orderCancellationDifferences(before: CheckoutState, after: CheckoutState,
  shipping?: 'wins' | 'competes') {
  for (const state of [before, after]) orderCheckoutStateSchema.parse(state);
  const differences = compareCancellation(before, after, true, shipping);
  for (const state of [before, after]) {
    for (const key of ['orphanOrderLines', 'orphanAllocations'] as const) {
      if (state[key] !== 0) differences.push({ control: `cancellation ${key}`, observed: state[key]!, expected: 0 });
    }
  }
  return differences;
}

function compareCancellation(before: CheckoutState, after: CheckoutState, refund: boolean,
  shipping?: 'wins' | 'competes'):
  Array<{ control: string; observed: number; expected: number }> {
  const orders = before.orders.filter(order => order.accountId === before.accountId);
  const order = orders[0];
  if (orders.length !== 1 || !order || order.status !== 'pending' || !order.lines.length
    || order.lines.some(line => line.itemId !== before.itemId || line.quantity <= 0
      || !line.allocations.length || line.allocations.some(row => row.quantity <= 0
        || !before.stock.some(stock => stockItem(before, stock) === line.itemId && stock.warehouseId === row.warehouseId))
      || line.allocations.reduce((sum, row) => integer.parse(sum + row.quantity), 0) !== line.quantity)) {
    return [{ control: 'cancellation requires one pending single-product order with complete allocations', observed: 0, expected: 1 }];
  }
  const expected = structuredClone(before);
  const resolved = expected.orders.find(row => row.id === order.id)!;
  const shipped = shipping === 'wins' || (shipping === 'competes'
    && after.orders.find(row => row.id === order.id)?.status === 'shipped');
  resolved.status = shipped ? 'shipped' : 'cancelled';
  if (refund && !shipped) resolved.refundedMinor = resolved.totalMinor;
  for (const stock of shipped ? [] : expected.stock) {
    for (const allocation of order.lines.flatMap(line => line.allocations)) {
      if (stockItem(expected, stock) === before.itemId && allocation.warehouseId === stock.warehouseId) stock.quantity = integer.parse(stock.quantity + allocation.quantity);
    }
  }
  // Database row order is not part of cancellation. Keep nested allocation and
  // line order independent too, while retaining duplicates for comparison.
  const wanted = normalized(expected), observed = normalized(after);
  return (Object.keys(wanted) as Array<keyof CheckoutState>)
    .filter(key => !isDeepStrictEqual(observed[key], wanted[key]))
    .map(key => ({ control: `cancellation ${key}`, observed: 0, expected: 1 }));
}
