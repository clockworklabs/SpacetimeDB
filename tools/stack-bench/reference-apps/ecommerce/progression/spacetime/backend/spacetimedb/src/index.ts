import {
  t,
  SenderError,
  ScheduleAt,
  type InferSchema,
  type ReducerCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import spacetimedb, {
  item,
  warehouse,
  stock,
  account,
  session,
  cartItem,
  customerOrder,
  orderItem,
  orderItemStock,
  review,
  itemStats,
  category,
  itemCategory,
  customerProfile,
  staffRole,
  itemVariant,
  paymentRecord,
  staffActivity,
  supportTicket,
  supportReply,
  promotion,
  cartPromotion,
  notificationPreference,
  stockAlert,
  notification,
  reservation,
  expiredCartItem,
  cartExpiry,
  scheduledRestock,
  stockLedger,
  deliverySchedule,
  reorderRule,
  recommendationDismissal,
  maintenanceTick,
} from './schema';

export { default } from './schema';

type S = InferSchema<typeof spacetimedb>;
type Ctx = ReducerCtx<S>;
type VCtx = ViewCtx<S>;

// --- helpers ---

function hashPassword(password: string): string {
  const salted = 'stackbench-ecom-salt::' + password;
  let hash = 0x811c9dc5;
  for (let i = 0; i < salted.length; i++) {
    hash ^= salted.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  let hash2 = 0x1505;
  for (let i = 0; i < salted.length; i++) {
    hash2 = (Math.imul(hash2, 33) ^ salted.charCodeAt(i)) >>> 0;
  }
  return (hash >>> 0).toString(16).padStart(8, '0') + hash2.toString(16).padStart(8, '0');
}

function getAccountId(ctx: Ctx | VCtx): bigint | null {
  const s = ctx.db.session.identity.find(ctx.sender);
  return s ? s.accountId : null;
}

function requireAccount(ctx: Ctx) {
  const accountId = getAccountId(ctx);
  if (accountId === null) throw new SenderError('You must be signed in.');
  const acc = ctx.db.account.id.find(accountId);
  if (!acc) throw new SenderError('You must be signed in.');
  return acc;
}

function requireAdmin(ctx: Ctx) {
  const acc = requireAccount(ctx);
  if (!acc.isAdmin) throw new SenderError('Admin access required.');
  return acc;
}

function requireStaffOrAdmin(ctx: Ctx) {
  const acc = requireAccount(ctx);
  if (!acc.isAdmin && !acc.isStaff) throw new SenderError('Staff or admin access required.');
  return acc;
}

function requireOrderOwner(ctx: Ctx, orderId: bigint) {
  const acc = requireAccount(ctx);
  const order = ctx.db.customerOrder.id.find(orderId);
  if (!order) throw new SenderError('Order not found.');
  if (order.accountId !== acc.id) throw new SenderError('That is not your order.');
  return order;
}

function totalStock(ctx: Ctx | VCtx, itemId: bigint): number {
  let total = 0;
  for (const row of ctx.db.stock.by_item_warehouse.filter(itemId)) {
    total += row.quantity;
  }
  return total;
}

function decrementStockTracked(
  ctx: Ctx,
  itemId: bigint,
  qty: number
): Array<{ warehouseId: bigint; quantity: number }> {
  let remaining = qty;
  const rows = [...ctx.db.stock.by_item_warehouse.filter(itemId)];
  const allocations: Array<{ warehouseId: bigint; quantity: number }> = [];
  for (const row of rows) {
    if (remaining <= 0) break;
    const take = Math.min(row.quantity, remaining);
    if (take <= 0) continue;
    ctx.db.stock.by_item_warehouse.delete([row.item_id, row.warehouse_id]);
    ctx.db.stock.insert({ ...row, quantity: row.quantity - take });
    allocations.push({ warehouseId: row.warehouse_id, quantity: take });
    remaining -= take;
  }
  if (remaining > 0) throw new SenderError('Not enough stock.');
  return allocations;
}

function restoreStock(ctx: Ctx, itemId: bigint, warehouseId: bigint, quantity: number) {
  let existing = null;
  for (const row of ctx.db.stock.by_item_warehouse.filter([itemId, warehouseId])) {
    existing = row;
    break;
  }
  if (existing) {
    ctx.db.stock.by_item_warehouse.delete([itemId, warehouseId]);
    ctx.db.stock.insert({ ...existing, quantity: existing.quantity + quantity });
  } else {
    ctx.db.stock.insert({ item_id: itemId, warehouse_id: warehouseId, quantity });
  }
}

function recordOrderItemStock(
  ctx: Ctx,
  orderItemId: bigint,
  allocations: Array<{ warehouseId: bigint; quantity: number }>
) {
  for (const a of allocations) {
    ctx.db.orderItemStock.insert({ id: 0n, orderItemId, warehouseId: a.warehouseId, quantity: a.quantity });
  }
}

function restoreOrderItemStock(ctx: Ctx, orderItem: { id: bigint; itemId: bigint }) {
  for (const alloc of ctx.db.orderItemStock.orderItemId.filter(orderItem.id)) {
    restoreStock(ctx, orderItem.itemId, alloc.warehouseId, alloc.quantity);
  }
}

function bumpPurchaseCount(ctx: Ctx, itemId: bigint, qty: number) {
  const stats = ctx.db.itemStats.itemId.find(itemId);
  if (stats) {
    ctx.db.itemStats.itemId.update({ ...stats, purchaseCount: stats.purchaseCount + qty });
  } else {
    ctx.db.itemStats.insert({ itemId, purchaseCount: qty });
  }
}

function decrementPurchaseCount(ctx: Ctx, itemId: bigint, qty: number) {
  const stats = ctx.db.itemStats.itemId.find(itemId);
  if (stats) {
    ctx.db.itemStats.itemId.update({ ...stats, purchaseCount: Math.max(0, stats.purchaseCount - qty) });
  }
}

function isOrderCounted(order: { status: string }): boolean {
  return order.status !== 'cancelled';
}

function findCartLine(ctx: Ctx, accountId: bigint, itemId: bigint) {
  for (const row of ctx.db.cartItem.byAccountItem.filter([accountId, itemId])) {
    return row;
  }
  return null;
}

const SECOND = 1_000_000n;

function nowMicros(ctx: Ctx): bigint {
  return ctx.timestamp.microsSinceUnixEpoch;
}

function findReservation(ctx: Ctx, accountId: bigint, itemId: bigint) {
  for (const row of ctx.db.reservation.byAccountItem.filter([accountId, itemId])) return row;
  return null;
}

function reserveUnits(ctx: Ctx, accountId: bigint, itemId: bigint, quantity: number) {
  const rows = [...ctx.db.stock.by_item_warehouse.filter(itemId)];
  const row = rows.find(candidate => candidate.quantity >= quantity);
  if (!row) throw new SenderError('Not enough stock to reserve.');
  ctx.db.stock.by_item_warehouse.delete([row.item_id, row.warehouse_id]);
  ctx.db.stock.insert({ ...row, quantity: row.quantity - quantity });
  return ctx.db.reservation.insert({
    id: 0n,
    accountId,
    itemId,
    warehouseId: row.warehouse_id,
    quantity,
    expiresMicros: nowMicros(ctx) + 90n * SECOND,
    expired: false,
  });
}

function releaseReservation(ctx: Ctx, row: {
  id: bigint;
  itemId: bigint;
  warehouseId: bigint;
  quantity: number;
}) {
  restoreStock(ctx, row.itemId, row.warehouseId, row.quantity);
  ctx.db.reservation.id.delete(row.id);
}

function touchCart(ctx: Ctx, accountId: bigint) {
  const expiresMicros = nowMicros(ctx) + 300n * SECOND;
  const row = ctx.db.cartExpiry.accountId.find(accountId);
  if (row) ctx.db.cartExpiry.accountId.update({ ...row, expiresMicros, expired: false });
  else ctx.db.cartExpiry.insert({ accountId, expiresMicros, expired: false });
}

function recordActivity(ctx: Ctx, actorAccountId: bigint, action: string, subject: string) {
  ctx.db.staffActivity.insert({
    id: 0n,
    actorAccountId,
    action,
    subject,
    createdMicros: nowMicros(ctx),
  });
}

function notify(ctx: Ctx, accountId: bigint, kind: string, message: string) {
  const preference = ctx.db.notificationPreference.accountId.find(accountId);
  if (kind === 'delivery' && preference && !preference.orderEnabled) return;
  if (kind === 'stock' && preference && !preference.stockEnabled) return;
  ctx.db.notification.insert({ id: 0n, accountId, kind, message, unread: true });
}

function processReorderRules(ctx: Ctx, itemId: bigint) {
  for (const rule of ctx.db.reorderRule.iter()) {
    if (rule.itemId !== itemId || totalStock(ctx, itemId) > rule.threshold) continue;
    const pending = [...ctx.db.scheduledRestock.iter()].some(row =>
      row.itemId === itemId && row.status === 'pending');
    if (!pending) {
      ctx.db.scheduledRestock.insert({
        id: 0n,
        itemId,
        warehouseId: rule.warehouseId,
        quantity: rule.quantity,
        dueMicros: nowMicros(ctx) + 10n * SECOND,
        status: 'pending',
      });
    }
  }
}

function processMaintenance(ctx: Ctx) {
  const now = nowMicros(ctx);
  for (const row of [...ctx.db.reservation.iter()]) {
    if (row.expired || row.expiresMicros > now) continue;
    restoreStock(ctx, row.itemId, row.warehouseId, row.quantity);
    ctx.db.reservation.id.update({ ...row, expired: true });
  }
  for (const expiry of [...ctx.db.cartExpiry.iter()]) {
    if (expiry.expired || expiry.expiresMicros > now) continue;
    for (const line of [...ctx.db.cartItem.byAccountItem.filter(expiry.accountId)]) {
      ctx.db.expiredCartItem.insert({
        id: 0n,
        accountId: expiry.accountId,
        itemId: line.itemId,
        quantity: line.quantity,
      });
      const held = findReservation(ctx, expiry.accountId, line.itemId);
      if (held && !held.expired) releaseReservation(ctx, held);
      else if (held) ctx.db.reservation.id.delete(held.id);
      ctx.db.cartItem.id.delete(line.id);
    }
    ctx.db.cartExpiry.accountId.update({ ...expiry, expired: true });
  }
  for (const pending of [...ctx.db.scheduledRestock.iter()]) {
    if (pending.status !== 'pending' || pending.dueMicros > now) continue;
    restoreStock(ctx, pending.itemId, pending.warehouseId, pending.quantity);
    ctx.db.scheduledRestock.id.update({ ...pending, status: 'complete' });
    ctx.db.stockLedger.insert({
      id: 0n,
      itemId: pending.itemId,
      warehouseId: pending.warehouseId,
      quantity: pending.quantity,
      createdMicros: now,
      source: 'scheduled restock',
    });
    for (const alert of [...ctx.db.stockAlert.itemId.filter(pending.itemId)]) {
      if (alert.fulfilled) continue;
      const name = ctx.db.item.id.find(pending.itemId)?.name ?? 'Item';
      notify(ctx, alert.accountId, 'stock', `${name} is available again.`);
      ctx.db.stockAlert.id.update({ ...alert, fulfilled: true });
    }
  }
  for (const schedule of [...ctx.db.deliverySchedule.iter()]) {
    if (schedule.completed || schedule.dueMicros > now) continue;
    const order = ctx.db.customerOrder.id.find(schedule.orderId);
    if (order && order.status === 'shipped') {
      ctx.db.customerOrder.id.update({ ...order, status: 'delivered' });
      notify(ctx, order.accountId, 'delivery', `Order ${order.id} was delivered.`);
    }
    ctx.db.deliverySchedule.orderId.update({ ...schedule, completed: true });
  }
}

// --- lifecycle ---

const CATALOGUE: Array<[string, number, number, number, string]> = [
  ['Air Purifier', 189.0, 60, 40, 'Home'],
  ['Bluetooth Speaker', 79.5, 50, 50, 'Audio'],
  ['Coffee Grinder', 64.0, 70, 30, 'Home'],
  ['Desk Lamp', 42.0, 55, 45, 'Home'],
  ['Espresso Machine', 449.0, 80, 20, 'Home'],
  ['Gaming Mouse', 59.0, 50, 50, 'Computing'],
  ['Headphones', 199.0, 60, 40, 'Audio'],
  ['Induction Cooktop', 329.0, 50, 50, 'Home'],
  ['Keyboard', 89.0, 70, 30, 'Computing'],
  ['Laptop Stand', 29.0, 90, 10, 'Computing'],
  ['Mirrorless Camera', 1299.0, 2, 1, 'Photo'],
  ['Webcam', 69.0, 60, 40, 'Computing'],
];

export const init = spacetimedb.init((ctx) => {
  const hasItems = [...ctx.db.item.iter()].length > 0;

  if (!hasItems) {
    const east = ctx.db.warehouse.insert({ id: 0n, name: 'East' });
    const west = ctx.db.warehouse.insert({ id: 0n, name: 'West' });

    for (const [name, price, eastQty, westQty] of CATALOGUE) {
      const it = ctx.db.item.insert({ id: 0n, name, price });
      ctx.db.stock.insert({ item_id: it.id, warehouse_id: east.id, quantity: eastQty });
      ctx.db.stock.insert({ item_id: it.id, warehouse_id: west.id, quantity: westQty });
      ctx.db.itemStats.insert({ itemId: it.id, purchaseCount: 0 });
    }
  }

  const hasCategories = [...ctx.db.category.iter()].length > 0;
  if (!hasCategories) {
    const categoryIdByName = new Map<string, bigint>();
    for (const [, , , , categoryName] of CATALOGUE) {
      if (categoryIdByName.has(categoryName)) continue;
      const cat = ctx.db.category.insert({ id: 0n, name: categoryName });
      categoryIdByName.set(categoryName, cat.id);
    }
    const itemsByName = new Map<string, bigint>();
    for (const it of ctx.db.item.iter()) itemsByName.set(it.name, it.id);
    for (const [name, , , , categoryName] of CATALOGUE) {
      const itemId = itemsByName.get(name);
      const categoryId = categoryIdByName.get(categoryName);
      if (itemId === undefined || categoryId === undefined) continue;
      ctx.db.itemCategory.insert({ itemId, categoryId });
    }
  }

  if (!ctx.db.account.username.find('admin')) {
    ctx.db.account.insert({
      id: 0n,
      username: 'admin',
      passwordHash: hashPassword('stackbench-admin-2026'),
      isAdmin: true,
      isStaff: false,
    });
  }

  if (!ctx.db.account.username.find('staff')) {
    const staffAccount = ctx.db.account.insert({
      id: 0n,
      username: 'staff',
      passwordHash: hashPassword('stackbench-staff-2026'),
      isAdmin: false,
      isStaff: true,
    });
    ctx.db.staffRole.insert({ accountId: staffAccount.id, role: 'operations' });
  }

  if (!ctx.db.account.username.find('customer')) {
    ctx.db.account.insert({
      id: 0n,
      username: 'customer',
      passwordHash: hashPassword('stackbench-customer-2026'),
      isAdmin: false,
      isStaff: false,
    });
  }

  if ([...ctx.db.maintenanceTick.iter()].length === 0) {
    ctx.db.maintenanceTick.insert({
      id: 0n,
      scheduledAt: ScheduleAt.time(nowMicros(ctx) + SECOND),
    });
  }
});

export const onConnect = spacetimedb.clientConnected((_ctx) => {});

export const onDisconnect = spacetimedb.clientDisconnected((_ctx) => {});

// --- auth ---

export const signUp = spacetimedb.reducer(
  { username: t.string(), password: t.string() },
  (ctx, { username, password }) => {
    const uname = username.trim();
    if (uname.length === 0) throw new SenderError('Username is required.');
    if (password.length === 0) throw new SenderError('Password is required.');
    const existing = ctx.db.account.username.find(uname);
    if (existing) throw new SenderError('That username is already taken.');

    const acc = ctx.db.account.insert({
      id: 0n,
      username: uname,
      passwordHash: hashPassword(password),
      isAdmin: false,
      isStaff: false,
    });

    const existingSession = ctx.db.session.identity.find(ctx.sender);
    if (existingSession) {
      ctx.db.session.identity.update({ ...existingSession, accountId: acc.id });
    } else {
      ctx.db.session.insert({ identity: ctx.sender, accountId: acc.id });
    }
  }
);

export const signIn = spacetimedb.reducer(
  { username: t.string(), password: t.string() },
  (ctx, { username, password }) => {
    const acc = ctx.db.account.username.find(username.trim());
    if (!acc || acc.passwordHash !== hashPassword(password)) {
      throw new SenderError('Incorrect username or password.');
    }

    const existingSession = ctx.db.session.identity.find(ctx.sender);
    if (existingSession) {
      ctx.db.session.identity.update({ ...existingSession, accountId: acc.id });
    } else {
      ctx.db.session.insert({ identity: ctx.sender, accountId: acc.id });
    }
  }
);

export const signOut = spacetimedb.reducer((ctx) => {
  const existingSession = ctx.db.session.identity.find(ctx.sender);
  if (existingSession) ctx.db.session.identity.delete(ctx.sender);
});

// --- views ---

export const currentUser = spacetimedb.view(
  { name: 'current_user', public: true },
  t.option(
    t.object('CurrentUserView', {
      id: t.u64(),
      username: t.string(),
      isAdmin: t.bool(),
      isStaff: t.bool(),
    })
  ),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return undefined;
    const acc = ctx.db.account.id.find(accountId);
    if (!acc) return undefined;
    return { id: acc.id, username: acc.username, isAdmin: acc.isAdmin, isStaff: acc.isStaff };
  }
);

const CartLineView = t.object('CartLineView', {
  itemId: t.u64(),
  quantity: t.u32(),
});

export const myCart = spacetimedb.view(
  { name: 'my_cart', public: true },
  t.array(CartLineView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    const out: Array<{ itemId: bigint; quantity: number }> = [];
    for (const row of ctx.db.cartItem.byAccountItem.filter(accountId)) {
      out.push({ itemId: row.itemId, quantity: row.quantity });
    }
    return out;
  }
);

const MyOrderView = t.object('MyOrderView', {
  orderId: t.u64(),
  createdAt: t.timestamp(),
  total: t.f64(),
  status: t.string(),
  discount: t.f64(),
  refundedTotal: t.f64(),
});

export const myOrders = spacetimedb.view(
  { name: 'my_orders', public: true },
  t.array(MyOrderView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    const rows = [...ctx.db.customerOrder.accountId.filter(accountId)];
    rows.sort((a, b) => (b.createdAt.microsSinceUnixEpoch > a.createdAt.microsSinceUnixEpoch ? 1 : -1));
    return rows.map((o) => ({
      orderId: o.id,
      createdAt: o.createdAt,
      total: o.total,
      status: o.status,
      discount: o.discount,
      refundedTotal: o.refundedTotal,
    }));
  }
);

const MyOrderItemView = t.object('MyOrderItemView', {
  orderId: t.u64(),
  itemId: t.u64(),
  itemName: t.string(),
  quantity: t.u32(),
  unitPrice: t.f64(),
  returned: t.bool(),
});

export const myOrderItems = spacetimedb.view(
  { name: 'my_order_items', public: true },
  t.array(MyOrderItemView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    const out: Array<{
      orderId: bigint;
      itemId: bigint;
      itemName: string;
      quantity: number;
      unitPrice: number;
      returned: boolean;
    }> = [];
    for (const o of ctx.db.customerOrder.accountId.filter(accountId)) {
      for (const li of ctx.db.orderItem.orderId.filter(o.id)) {
        out.push({
          orderId: o.id,
          itemId: li.itemId,
          itemName: li.itemName,
          quantity: li.quantity,
          unitPrice: li.unitPrice,
          returned: li.returned,
        });
      }
    }
    return out;
  }
);

const AdminRevenueView = t.object('AdminRevenueView', { total: t.f64() });

export const adminRevenue = spacetimedb.view(
  { name: 'admin_revenue', public: true },
  t.option(AdminRevenueView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return { total: 0 };
    const acc = ctx.db.account.id.find(accountId);
    if (!acc || !acc.isAdmin) return { total: 0 };
    let total = 0;
    for (const o of ctx.db.customerOrder.iter()) {
      if (!isOrderCounted(o)) continue;
      for (const li of ctx.db.orderItem.orderId.filter(o.id)) {
        if (li.returned) continue;
        total += li.unitPrice * li.quantity;
      }
    }
    return { total };
  }
);

const QueueOrderView = t.object('QueueOrderView', {
  orderId: t.u64(),
  createdAt: t.timestamp(),
  itemNames: t.array(t.string()),
  warehouseNames: t.array(t.string()),
});

export const fulfilmentQueue = spacetimedb.view(
  { name: 'fulfilment_queue', public: true },
  t.array(QueueOrderView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    const acc = ctx.db.account.id.find(accountId);
    if (!acc || (!acc.isAdmin && !acc.isStaff)) return [];

    const pending = [...ctx.db.customerOrder.iter()].filter((o) => o.status === 'pending');
    pending.sort((a, b) => (a.createdAt.microsSinceUnixEpoch < b.createdAt.microsSinceUnixEpoch ? -1 : 1));

    return pending.map((order) => {
      const itemNames: string[] = [];
      const warehouseNames: string[] = [];
      for (const li of ctx.db.orderItem.orderId.filter(order.id)) {
        for (const alloc of ctx.db.orderItemStock.orderItemId.filter(li.id)) {
          itemNames.push(li.itemName);
          const wh = ctx.db.warehouse.id.find(alloc.warehouseId);
          warehouseNames.push(wh ? wh.name : 'Unknown');
        }
      }
      return { orderId: order.id, createdAt: order.createdAt, itemNames, warehouseNames };
    });
  }
);

const CategoryTotalView = t.object('CategoryTotalView', {
  categoryId: t.u64(),
  name: t.string(),
  unitsSold: t.u32(),
  revenue: t.f64(),
});

export const categoryTotals = spacetimedb.view(
  { name: 'category_totals', public: true },
  t.array(CategoryTotalView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    const acc = accountId !== null ? ctx.db.account.id.find(accountId) : null;
    if (!acc || !acc.isAdmin) return [];

    const stats = new Map<bigint, { name: string; units: number; revenue: number }>();
    for (const cat of ctx.db.category.iter()) stats.set(cat.id, { name: cat.name, units: 0, revenue: 0 });

    for (const order of ctx.db.customerOrder.iter()) {
      if (!isOrderCounted(order)) continue;
      for (const li of ctx.db.orderItem.orderId.filter(order.id)) {
        if (li.returned) continue;
        const link = ctx.db.itemCategory.itemId.find(li.itemId);
        if (!link) continue;
        const s = stats.get(link.categoryId);
        if (!s) continue;
        s.units += li.quantity;
        s.revenue += li.unitPrice * li.quantity;
      }
    }

    return [...stats.entries()]
      .map(([categoryId, s]) => ({ categoryId, name: s.name, unitsSold: s.units, revenue: s.revenue }))
      .sort((a, b) => a.name.localeCompare(b.name));
  }
);

const RecommendedItemView = t.object('RecommendedItemView', {
  itemId: t.u64(),
  name: t.string(),
  price: t.f64(),
});

export const recommended = spacetimedb.view(
  { name: 'recommended', public: true },
  t.array(RecommendedItemView),
  (ctx) => {
    const purchaseCountOf = (id: bigint) => ctx.db.itemStats.itemId.find(id)?.purchaseCount ?? 0;
    const accountId = getAccountId(ctx);

    if (accountId === null) {
      const items = [...ctx.db.item.iter()];
      items.sort((a, b) => {
        const diff = purchaseCountOf(b.id) - purchaseCountOf(a.id);
        return diff !== 0 ? diff : a.name.localeCompare(b.name);
      });
      return items.slice(0, 10).map((i) => ({ itemId: i.id, name: i.name, price: i.price }));
    }

    const purchasedCategoryIds = new Set<bigint>();
    const purchasedItemIds = new Set<bigint>();
    for (const order of ctx.db.customerOrder.accountId.filter(accountId)) {
      if (!isOrderCounted(order)) continue;
      for (const li of ctx.db.orderItem.orderId.filter(order.id)) {
        if (li.returned) continue;
        purchasedItemIds.add(li.itemId);
        const link = ctx.db.itemCategory.itemId.find(li.itemId);
        if (link) purchasedCategoryIds.add(link.categoryId);
      }
    }
    if (purchasedCategoryIds.size === 0) return [];

    const cartItemIds = new Set<bigint>();
    for (const c of ctx.db.cartItem.byAccountItem.filter(accountId)) cartItemIds.add(c.itemId);
    const dismissedItemIds = new Set<bigint>();
    for (const row of ctx.db.recommendationDismissal.byAccountItem.filter(accountId)) {
      dismissedItemIds.add(row.itemId);
    }

    const candidates: Array<{ itemId: bigint; name: string; price: number }> = [];
    for (const link of ctx.db.itemCategory.iter()) {
      if (!purchasedCategoryIds.has(link.categoryId)) continue;
      if (purchasedItemIds.has(link.itemId)) continue;
      if (cartItemIds.has(link.itemId)) continue;
      if (dismissedItemIds.has(link.itemId)) continue;
      const it = ctx.db.item.id.find(link.itemId);
      if (!it) continue;
      candidates.push({ itemId: it.id, name: it.name, price: it.price });
    }

    candidates.sort((a, b) => {
      const diff = purchaseCountOf(b.itemId) - purchaseCountOf(a.itemId);
      return diff !== 0 ? diff : a.name.localeCompare(b.name);
    });
    return candidates;
  }
);

// --- cart & purchasing ---

export const buyNow = spacetimedb.reducer({ itemId: t.u64() }, (ctx, { itemId }) => {
  const acc = requireAccount(ctx);
  const it = ctx.db.item.id.find(itemId);
  if (!it) throw new SenderError('Item not found.');
  if (totalStock(ctx, itemId) <= 0) throw new SenderError('This item is out of stock.');

  const allocations = decrementStockTracked(ctx, itemId, 1);

  const order = ctx.db.customerOrder.insert({
    id: 0n,
    accountId: acc.id,
    createdAt: ctx.timestamp,
    total: it.price,
    status: 'pending',
    discount: 0,
    promotionId: undefined,
    refundedTotal: 0,
  });
  const orderItemRow = ctx.db.orderItem.insert({
    id: 0n,
    orderId: order.id,
    itemId: it.id,
    itemName: it.name,
    quantity: 1,
    unitPrice: it.price,
    returned: false,
  });
  recordOrderItemStock(ctx, orderItemRow.id, allocations);
  bumpPurchaseCount(ctx, itemId, 1);
  ctx.db.paymentRecord.insert({ id: 0n, orderId: order.id, amount: order.total, status: 'paid' });
  processReorderRules(ctx, itemId);
});

export const addToCart = spacetimedb.reducer({ itemId: t.u64() }, (ctx, { itemId }) => {
  const acc = requireAccount(ctx);
  const it = ctx.db.item.id.find(itemId);
  if (!it) throw new SenderError('Item not found.');

  const existing = findCartLine(ctx, acc.id, itemId);
  const held = findReservation(ctx, acc.id, itemId);
  if (existing) {
    if (!held || held.expired) {
      if (held) ctx.db.reservation.id.delete(held.id);
      reserveUnits(ctx, acc.id, itemId, existing.quantity + 1);
    } else {
      let stockRow = null;
      for (const row of ctx.db.stock.by_item_warehouse.filter([itemId, held.warehouseId])) {
        stockRow = row;
        break;
      }
      if (!stockRow || stockRow.quantity < 1) throw new SenderError('Not enough stock to reserve.');
      ctx.db.stock.by_item_warehouse.delete([itemId, held.warehouseId]);
      ctx.db.stock.insert({ ...stockRow, quantity: stockRow.quantity - 1 });
      ctx.db.reservation.id.update({
        ...held,
        quantity: held.quantity + 1,
        expiresMicros: nowMicros(ctx) + 90n * SECOND,
      });
    }
    ctx.db.cartItem.id.update({ ...existing, quantity: existing.quantity + 1 });
  } else {
    reserveUnits(ctx, acc.id, itemId, 1);
    ctx.db.cartItem.insert({ id: 0n, accountId: acc.id, itemId, quantity: 1 });
  }
  touchCart(ctx, acc.id);
});

export const updateCartQuantity = spacetimedb.reducer(
  { itemId: t.u64(), quantity: t.i32() },
  (ctx, { itemId, quantity }) => {
    const acc = requireAccount(ctx);
    if (quantity < 1) throw new SenderError('Quantity must be at least 1.');
    const existing = findCartLine(ctx, acc.id, itemId);
    if (!existing) throw new SenderError('That item is not in your cart.');
    const held = findReservation(ctx, acc.id, itemId);
    if (!held || held.expired) {
      if (held) ctx.db.reservation.id.delete(held.id);
      reserveUnits(ctx, acc.id, itemId, quantity);
    } else if (quantity > existing.quantity) {
      const increase = quantity - existing.quantity;
      let stockRow = null;
      for (const row of ctx.db.stock.by_item_warehouse.filter([itemId, held.warehouseId])) {
        stockRow = row;
        break;
      }
      if (!stockRow || stockRow.quantity < increase) throw new SenderError('Not enough stock to reserve.');
      ctx.db.stock.by_item_warehouse.delete([itemId, held.warehouseId]);
      ctx.db.stock.insert({ ...stockRow, quantity: stockRow.quantity - increase });
      ctx.db.reservation.id.update({
        ...held,
        quantity,
        expiresMicros: nowMicros(ctx) + 90n * SECOND,
        expired: false,
      });
    } else if (quantity < existing.quantity) {
      const released = existing.quantity - quantity;
      restoreStock(ctx, itemId, held.warehouseId, released);
      ctx.db.reservation.id.update({
        ...held,
        quantity,
        expiresMicros: nowMicros(ctx) + 90n * SECOND,
      });
    } else {
      ctx.db.reservation.id.update({
        ...held,
        expiresMicros: nowMicros(ctx) + 90n * SECOND,
        expired: false,
      });
    }
    ctx.db.cartItem.id.update({ ...existing, quantity });
    touchCart(ctx, acc.id);
  }
);

export const removeFromCart = spacetimedb.reducer({ itemId: t.u64() }, (ctx, { itemId }) => {
  const acc = requireAccount(ctx);
  const existing = findCartLine(ctx, acc.id, itemId);
  if (existing) {
    const held = findReservation(ctx, acc.id, itemId);
    if (held && !held.expired) releaseReservation(ctx, held);
    else if (held) ctx.db.reservation.id.delete(held.id);
    ctx.db.cartItem.id.delete(existing.id);
    touchCart(ctx, acc.id);
  }
});

export const checkout = spacetimedb.reducer((ctx) => {
  const acc = requireAccount(ctx);
  const lines = [...ctx.db.cartItem.byAccountItem.filter(acc.id)];
  if (lines.length === 0) throw new SenderError('Your cart is empty.');

  let total = 0;
  const priced: Array<{ itemId: bigint; name: string; quantity: number; price: number }> = [];
  for (const line of lines) {
    const it = ctx.db.item.id.find(line.itemId);
    if (!it) throw new SenderError('An item in your cart no longer exists.');
    const held = findReservation(ctx, acc.id, line.itemId);
    if (!held || held.expired || held.quantity !== line.quantity) {
      throw new SenderError(`The reservation for ${it.name} has expired.`);
    }
    priced.push({ itemId: it.id, name: it.name, quantity: line.quantity, price: it.price });
    total += it.price * line.quantity;
  }

  const applied = ctx.db.cartPromotion.accountId.find(acc.id);
  const promo = applied ? ctx.db.promotion.id.find(applied.promotionId) : null;
  const discount = promo ? total * (promo.discountPercent / 100) : 0;

  const order = ctx.db.customerOrder.insert({
    id: 0n,
    accountId: acc.id,
    createdAt: ctx.timestamp,
    total: total - discount,
    status: 'pending',
    discount,
    promotionId: promo?.id,
    refundedTotal: 0,
  });

  for (const p of priced) {
    const held = findReservation(ctx, acc.id, p.itemId);
    if (!held) throw new SenderError('Reservation not found.');
    const allocations = [{ warehouseId: held.warehouseId, quantity: held.quantity }];
    const orderItemRow = ctx.db.orderItem.insert({
      id: 0n,
      orderId: order.id,
      itemId: p.itemId,
      itemName: p.name,
      quantity: p.quantity,
      unitPrice: p.price,
      returned: false,
    });
    recordOrderItemStock(ctx, orderItemRow.id, allocations);
    bumpPurchaseCount(ctx, p.itemId, p.quantity);
    ctx.db.reservation.id.delete(held.id);
    processReorderRules(ctx, p.itemId);
  }

  for (const line of lines) ctx.db.cartItem.id.delete(line.id);
  const expiry = ctx.db.cartExpiry.accountId.find(acc.id);
  if (expiry) ctx.db.cartExpiry.accountId.delete(acc.id);
  ctx.db.paymentRecord.insert({ id: 0n, orderId: order.id, amount: order.total, status: 'paid' });
  if (promo) {
    ctx.db.promotion.id.update({ ...promo, redemptions: promo.redemptions + 1 });
    ctx.db.cartPromotion.accountId.delete(acc.id);
  }
});

// --- reviews ---

export const writeReview = spacetimedb.reducer(
  { itemId: t.u64(), rating: t.u32(), comment: t.string() },
  (ctx, { itemId, rating, comment }) => {
    const acc = requireAccount(ctx);
    if (rating < 1 || rating > 5) throw new SenderError('Rating must be between 1 and 5.');

    let bought = false;
    for (const o of ctx.db.customerOrder.accountId.filter(acc.id)) {
      for (const li of ctx.db.orderItem.orderId.filter(o.id)) {
        if (li.itemId === itemId) {
          bought = true;
          break;
        }
      }
      if (bought) break;
    }
    if (!bought) throw new SenderError('You can only review items you have purchased.');

    let existing = null;
    for (const r of ctx.db.review.byItemAccount.filter([itemId, acc.id])) {
      existing = r;
      break;
    }
    if (existing) {
      ctx.db.review.id.update({ ...existing, rating, comment, createdAt: ctx.timestamp });
    } else {
      ctx.db.review.insert({
        id: 0n,
        itemId,
        accountId: acc.id,
        rating,
        comment,
        createdAt: ctx.timestamp,
      });
    }
  }
);

// --- admin ---

export const adminRestock = spacetimedb.reducer(
  { itemId: t.u64(), warehouseId: t.u64(), quantity: t.u32() },
  (ctx, { itemId, warehouseId, quantity }) => {
    requireAdmin(ctx);
    if (quantity < 1) throw new SenderError('Restock quantity must be at least 1.');
    const it = ctx.db.item.id.find(itemId);
    if (!it) throw new SenderError('Item not found.');
    const wh = ctx.db.warehouse.id.find(warehouseId);
    if (!wh) throw new SenderError('Warehouse not found.');

    let existing = null;
    for (const row of ctx.db.stock.by_item_warehouse.filter([itemId, warehouseId])) {
      existing = row;
      break;
    }
    if (existing) {
      ctx.db.stock.by_item_warehouse.delete([itemId, warehouseId]);
      ctx.db.stock.insert({ ...existing, quantity: existing.quantity + quantity });
    } else {
      ctx.db.stock.insert({ item_id: itemId, warehouse_id: warehouseId, quantity });
    }
    for (const alert of [...ctx.db.stockAlert.itemId.filter(itemId)]) {
      if (alert.fulfilled) continue;
      notify(ctx, alert.accountId, 'stock', `${it.name} is available again.`);
      ctx.db.stockAlert.id.update({ ...alert, fulfilled: true });
    }
  }
);

export const adminChangePrice = spacetimedb.reducer(
  { itemId: t.u64(), price: t.f64() },
  (ctx, { itemId, price }) => {
    requireAdmin(ctx);
    if (!(price > 0)) throw new SenderError('Price must be positive.');
    const it = ctx.db.item.id.find(itemId);
    if (!it) throw new SenderError('Item not found.');
    ctx.db.item.id.update({ ...it, price });
  }
);

export const adminTransferStock = spacetimedb.reducer(
  { itemId: t.u64(), fromWarehouseId: t.u64(), toWarehouseId: t.u64(), quantity: t.u32() },
  (ctx, { itemId, fromWarehouseId, toWarehouseId, quantity }) => {
    requireAdmin(ctx);
    if (quantity < 1) throw new SenderError('Transfer quantity must be at least 1.');
    if (fromWarehouseId === toWarehouseId) throw new SenderError('Choose two different warehouses.');
    if (!ctx.db.item.id.find(itemId)) throw new SenderError('Item not found.');
    if (!ctx.db.warehouse.id.find(fromWarehouseId) || !ctx.db.warehouse.id.find(toWarehouseId)) {
      throw new SenderError('Warehouse not found.');
    }

    let fromRow = null;
    for (const row of ctx.db.stock.by_item_warehouse.filter([itemId, fromWarehouseId])) {
      fromRow = row;
      break;
    }
    const available = fromRow?.quantity ?? 0;
    if (available < quantity) {
      throw new SenderError(`Not enough stock in source warehouse: only ${available} available.`);
    }

    ctx.db.stock.by_item_warehouse.delete([itemId, fromWarehouseId]);
    ctx.db.stock.insert({ item_id: itemId, warehouse_id: fromWarehouseId, quantity: available - quantity });

    let toRow = null;
    for (const row of ctx.db.stock.by_item_warehouse.filter([itemId, toWarehouseId])) {
      toRow = row;
      break;
    }
    if (toRow) {
      ctx.db.stock.by_item_warehouse.delete([itemId, toWarehouseId]);
      ctx.db.stock.insert({ ...toRow, quantity: toRow.quantity + quantity });
    } else {
      ctx.db.stock.insert({ item_id: itemId, warehouse_id: toWarehouseId, quantity });
    }
  }
);

// --- fulfilment ---

export const shipOrder = spacetimedb.reducer({ orderId: t.u64() }, (ctx, { orderId }) => {
  requireStaffOrAdmin(ctx);
  const order = ctx.db.customerOrder.id.find(orderId);
  if (!order) throw new SenderError('Order not found.');
  if (order.status !== 'pending') throw new SenderError('Order is not pending.');
  ctx.db.customerOrder.id.update({ ...order, status: 'shipped' });
  const existing = ctx.db.deliverySchedule.orderId.find(orderId);
  const schedule = { orderId, dueMicros: nowMicros(ctx) + 60n * SECOND, completed: false };
  if (existing) ctx.db.deliverySchedule.orderId.update(schedule);
  else ctx.db.deliverySchedule.insert(schedule);
});

// --- cancellation & returns ---

export const cancelOrder = spacetimedb.reducer({ orderId: t.u64() }, (ctx, { orderId }) => {
  const order = requireOrderOwner(ctx, orderId);
  if (order.status !== 'pending') throw new SenderError('Order has already shipped.');

  for (const li of ctx.db.orderItem.orderId.filter(order.id)) {
    restoreOrderItemStock(ctx, li);
    decrementPurchaseCount(ctx, li.itemId, li.quantity);
    processReorderRules(ctx, li.itemId);
  }
  ctx.db.customerOrder.id.update({ ...order, status: 'cancelled' });
});

export const returnOrderItem = spacetimedb.reducer(
  { orderId: t.u64(), itemId: t.u64() },
  (ctx, { orderId, itemId }) => {
    const order = requireOrderOwner(ctx, orderId);
    if (order.status !== 'shipped') throw new SenderError('Order has not shipped yet.');

    let target = null;
    for (const li of ctx.db.orderItem.orderId.filter(order.id)) {
      if (li.itemId === itemId) {
        target = li;
        break;
      }
    }
    if (!target) throw new SenderError('Item not found in this order.');
    if (target.returned) throw new SenderError('Item already returned.');

    restoreOrderItemStock(ctx, target);
    decrementPurchaseCount(ctx, target.itemId, target.quantity);
    ctx.db.orderItem.id.update({ ...target, returned: true });
    processReorderRules(ctx, target.itemId);
  }
);

// --- progression maintenance ---

export const processMaintenanceTick = spacetimedb.reducer(
  { onSchedule: maintenanceTick },
  { tick: maintenanceTick.rowType },
  (ctx) => {
    processMaintenance(ctx);
    ctx.db.maintenanceTick.insert({
      id: 0n,
      scheduledAt: ScheduleAt.time(nowMicros(ctx) + SECOND),
    });
  }
);

export const saveProfile = spacetimedb.reducer(
  { name: t.string(), address: t.string() },
  (ctx, { name, address }) => {
    const acc = requireAccount(ctx);
    const row = { accountId: acc.id, name: name.trim(), address: address.trim() };
    const existing = ctx.db.customerProfile.accountId.find(acc.id);
    if (existing) ctx.db.customerProfile.accountId.update(row);
    else ctx.db.customerProfile.insert(row);
  }
);

export const assignStaffRole = spacetimedb.reducer(
  { accountId: t.u64(), role: t.string() },
  (ctx, { accountId, role }) => {
    const actor = requireAdmin(ctx);
    const target = ctx.db.account.id.find(accountId);
    if (!target || (!target.isStaff && !target.isAdmin)) throw new SenderError('Staff account not found.');
    const row = { accountId, role: role.trim() };
    const existing = ctx.db.staffRole.accountId.find(accountId);
    if (existing) ctx.db.staffRole.accountId.update(row);
    else ctx.db.staffRole.insert(row);
    recordActivity(ctx, actor.id, 'assigned role', target.username);
  }
);

export const addCatalogProduct = spacetimedb.reducer(
  { name: t.string(), categoryName: t.string(), price: t.f64(), variants: t.string() },
  (ctx, { name, categoryName, price, variants }) => {
    const actor = requireStaffOrAdmin(ctx);
    const role = ctx.db.staffRole.accountId.find(actor.id)?.role ?? '';
    if (!actor.isAdmin && role !== 'catalog') throw new SenderError('Catalog role required.');
    if (price <= 0) throw new SenderError('Price must be positive.');
    const product = ctx.db.item.insert({ id: 0n, name: name.trim(), price });
    ctx.db.itemStats.insert({ itemId: product.id, purchaseCount: 0 });
    let cat = ctx.db.category.name.find(categoryName.trim());
    if (!cat) cat = ctx.db.category.insert({ id: 0n, name: categoryName.trim() });
    ctx.db.itemCategory.insert({ itemId: product.id, categoryId: cat.id });
    for (const variantName of variants.split(',').map(value => value.trim()).filter(Boolean)) {
      ctx.db.itemVariant.insert({ id: 0n, itemId: product.id, name: variantName });
    }
    recordActivity(ctx, actor.id, 'added product', product.name);
  }
);

export const createSupportTicket = spacetimedb.reducer(
  { email: t.string(), subject: t.string(), message: t.string() },
  (ctx, { email, subject, message }) => {
    const accountId = getAccountId(ctx);
    const reference = `SUP-${nowMicros(ctx)}-${accountId ?? 0n}`;
    ctx.db.supportTicket.insert({
      id: 0n,
      reference,
      accountId: accountId ?? undefined,
      email: email.trim(),
      subject: subject.trim(),
      message: message.trim(),
      status: 'new',
      priority: 'normal',
      assigneeId: undefined,
      orderId: undefined,
      refundTotal: 0,
    });
  }
);

function requireSupportAccess(ctx: Ctx, ticketId: bigint) {
  const actor = requireAccount(ctx);
  const ticket = ctx.db.supportTicket.id.find(ticketId);
  if (!ticket) throw new SenderError('Support ticket not found.');
  if (!actor.isAdmin && !actor.isStaff && ticket.accountId !== actor.id) {
    throw new SenderError('That support ticket is private.');
  }
  return { actor, ticket };
}

export const triageSupport = spacetimedb.reducer(
  { ticketId: t.u64(), assigneeId: t.u64(), priority: t.string(), status: t.string() },
  (ctx, { ticketId, assigneeId, priority, status }) => {
    requireStaffOrAdmin(ctx);
    const ticket = ctx.db.supportTicket.id.find(ticketId);
    if (!ticket) throw new SenderError('Support ticket not found.');
    const assignee = ctx.db.account.id.find(assigneeId);
    if (!assignee || (!assignee.isStaff && !assignee.isAdmin)) throw new SenderError('Assignee must be staff.');
    ctx.db.supportTicket.id.update({ ...ticket, assigneeId, priority, status });
  }
);

export const replySupport = spacetimedb.reducer(
  { ticketId: t.u64(), body: t.string() },
  (ctx, { ticketId, body }) => {
    const { actor } = requireSupportAccess(ctx, ticketId);
    ctx.db.supportReply.insert({
      id: 0n,
      ticketId,
      accountId: actor.id,
      body: body.trim(),
      createdMicros: nowMicros(ctx),
    });
  }
);

export const linkSupportOrder = spacetimedb.reducer(
  { ticketId: t.u64(), orderId: t.u64() },
  (ctx, { ticketId, orderId }) => {
    const ticket = requireSupportAccess(ctx, ticketId).ticket;
    const order = requireOrderOwner(ctx, orderId);
    ctx.db.supportTicket.id.update({ ...ticket, orderId: order.id });
  }
);

export const supportRefund = spacetimedb.reducer(
  { ticketId: t.u64() },
  (ctx, { ticketId }) => {
    requireStaffOrAdmin(ctx);
    const ticket = ctx.db.supportTicket.id.find(ticketId);
    if (!ticket || ticket.orderId === undefined) throw new SenderError('The case has no linked order.');
    const order = ctx.db.customerOrder.id.find(ticket.orderId);
    if (!order) throw new SenderError('Order not found.');
    if (order.refundedTotal > 0) throw new SenderError('The order was already refunded.');
    ctx.db.customerOrder.id.update({ ...order, refundedTotal: order.total, status: 'refunded' });
    ctx.db.supportTicket.id.update({ ...ticket, refundTotal: order.total, status: 'refunded' });
    ctx.db.paymentRecord.insert({ id: 0n, orderId: order.id, amount: -order.total, status: 'refunded' });
  }
);

export const createPromotion = spacetimedb.reducer(
  {
    code: t.string(),
    discountPercent: t.f64(),
    startMicros: t.i64(),
    endMicros: t.i64(),
    usageLimit: t.u32(),
  },
  (ctx, input) => {
    requireStaffOrAdmin(ctx);
    if (input.discountPercent <= 0 || input.discountPercent > 100) {
      throw new SenderError('Discount must be between 0 and 100.');
    }
    if (input.endMicros <= input.startMicros) throw new SenderError('Promotion dates are invalid.');
    ctx.db.promotion.insert({ id: 0n, ...input, code: input.code.trim(), redemptions: 0 });
  }
);

export const applyPromotion = spacetimedb.reducer({ code: t.string() }, (ctx, { code }) => {
  const acc = requireAccount(ctx);
  const promo = ctx.db.promotion.code.find(code.trim());
  const now = nowMicros(ctx);
  if (!promo || promo.startMicros > now || promo.endMicros < now || promo.redemptions >= promo.usageLimit) {
    throw new SenderError('Promotion is expired, exhausted, or unknown.');
  }
  const row = { accountId: acc.id, promotionId: promo.id };
  const existing = ctx.db.cartPromotion.accountId.find(acc.id);
  if (existing) ctx.db.cartPromotion.accountId.update(row);
  else ctx.db.cartPromotion.insert(row);
});

export const saveNotificationPreferences = spacetimedb.reducer(
  { orderEnabled: t.bool(), stockEnabled: t.bool() },
  (ctx, input) => {
    const acc = requireAccount(ctx);
    const row = { accountId: acc.id, ...input };
    const existing = ctx.db.notificationPreference.accountId.find(acc.id);
    if (existing) ctx.db.notificationPreference.accountId.update(row);
    else ctx.db.notificationPreference.insert(row);
  }
);

export const requestStockAlert = spacetimedb.reducer({ itemId: t.u64() }, (ctx, { itemId }) => {
  const acc = requireAccount(ctx);
  if (totalStock(ctx, itemId) > 0) throw new SenderError('The item is already available.');
  const duplicate = [...ctx.db.stockAlert.itemId.filter(itemId)]
    .some(row => row.accountId === acc.id && !row.fulfilled);
  if (!duplicate) ctx.db.stockAlert.insert({ id: 0n, accountId: acc.id, itemId, fulfilled: false });
});

export const scheduleRestock = spacetimedb.reducer(
  { itemId: t.u64(), warehouseId: t.u64(), quantity: t.u32(), delaySeconds: t.u32() },
  (ctx, input) => {
    requireAdmin(ctx);
    if (!ctx.db.item.id.find(input.itemId) || !ctx.db.warehouse.id.find(input.warehouseId)) {
      throw new SenderError('Item or warehouse not found.');
    }
    ctx.db.scheduledRestock.insert({
      id: 0n,
      itemId: input.itemId,
      warehouseId: input.warehouseId,
      quantity: input.quantity,
      dueMicros: nowMicros(ctx) + BigInt(input.delaySeconds) * SECOND,
      status: 'pending',
    });
  }
);

export const cancelScheduledRestock = spacetimedb.reducer({ restockId: t.u64() }, (ctx, { restockId }) => {
  requireAdmin(ctx);
  const row = ctx.db.scheduledRestock.id.find(restockId);
  if (!row || row.status !== 'pending') throw new SenderError('Pending restock not found.');
  ctx.db.scheduledRestock.id.update({ ...row, status: 'cancelled' });
});

export const restoreExpiredCart = spacetimedb.reducer((ctx) => {
  const acc = requireAccount(ctx);
  for (const row of [...ctx.db.expiredCartItem.accountId.filter(acc.id)]) {
    if (totalStock(ctx, row.itemId) < row.quantity) continue;
    reserveUnits(ctx, acc.id, row.itemId, row.quantity);
    ctx.db.cartItem.insert({ id: 0n, accountId: acc.id, itemId: row.itemId, quantity: row.quantity });
    ctx.db.expiredCartItem.id.delete(row.id);
  }
  touchCart(ctx, acc.id);
});

export const saveReorderRule = spacetimedb.reducer(
  { itemId: t.u64(), warehouseId: t.u64(), threshold: t.u32(), quantity: t.u32() },
  (ctx, input) => {
    requireStaffOrAdmin(ctx);
    ctx.db.reorderRule.insert({ id: 0n, ...input });
    processReorderRules(ctx, input.itemId);
  }
);

export const dismissRecommendation = spacetimedb.reducer({ itemId: t.u64() }, (ctx, { itemId }) => {
  const acc = requireAccount(ctx);
  const exists = [...ctx.db.recommendationDismissal.byAccountItem.filter([acc.id, itemId])].length > 0;
  if (!exists) ctx.db.recommendationDismissal.insert({ id: 0n, accountId: acc.id, itemId });
});

// --- progression views ---

const ProfileView = t.object('ProfileView', {
  accountId: t.u64(),
  name: t.string(),
  address: t.string(),
});

export const myProfile = spacetimedb.view(
  { name: 'my_profile', public: true },
  t.option(ProfileView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    return accountId === null ? undefined : ctx.db.customerProfile.accountId.find(accountId) ?? undefined;
  }
);

const CatalogDetailView = t.object('CatalogDetailView', {
  itemId: t.u64(),
  category: t.string(),
});

export const catalogDetails = spacetimedb.view(
  { name: 'catalog_details', public: true },
  t.array(CatalogDetailView),
  (ctx) => [...ctx.db.itemCategory.iter()].map(row => ({
    itemId: row.itemId,
    category: ctx.db.category.id.find(row.categoryId)?.name ?? '',
  }))
);

const StaffRoleView = t.object('StaffRoleView', {
  accountId: t.u64(),
  username: t.string(),
  role: t.string(),
});

export const staffRoles = spacetimedb.view(
  { name: 'staff_roles', public: true },
  t.array(StaffRoleView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    const actor = accountId === null ? null : ctx.db.account.id.find(accountId);
    if (!actor || (!actor.isAdmin && !actor.isStaff)) return [];
    return [...ctx.db.account.iter()].filter(row => row.isStaff || row.isAdmin).map(row => ({
      accountId: row.id,
      username: row.username,
      role: ctx.db.staffRole.accountId.find(row.id)?.role ?? (row.isAdmin ? 'administrator' : ''),
    }));
  }
);

const PaymentView = t.object('PaymentView', {
  orderId: t.u64(),
  amount: t.f64(),
  status: t.string(),
});

export const myPayments = spacetimedb.view(
  { name: 'my_payments', public: true },
  t.array(PaymentView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    const orderIds = new Set([...ctx.db.customerOrder.accountId.filter(accountId)].map(row => row.id));
    return [...ctx.db.paymentRecord.iter()].filter(row => orderIds.has(row.orderId))
      .map(row => ({ orderId: row.orderId, amount: row.amount, status: row.status }));
  }
);

const ActivityView = t.object('ActivityView', {
  actor: t.string(),
  action: t.string(),
  subject: t.string(),
  createdMicros: t.i64(),
});

export const activityHistory = spacetimedb.view(
  { name: 'activity_history', public: true },
  t.array(ActivityView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    const actor = accountId === null ? null : ctx.db.account.id.find(accountId);
    if (!actor || (!actor.isAdmin && !actor.isStaff)) return [];
    return [...ctx.db.staffActivity.iter()].map(row => ({
      actor: ctx.db.account.id.find(row.actorAccountId)?.username ?? 'unknown',
      action: row.action,
      subject: row.subject,
      createdMicros: row.createdMicros,
    }));
  }
);

const SupportTicketView = t.object('SupportTicketView', {
  id: t.u64(),
  reference: t.string(),
  subject: t.string(),
  status: t.string(),
  priority: t.string(),
  assignee: t.string(),
  orderId: t.option(t.u64()),
  refundTotal: t.f64(),
});

export const visibleSupportTickets = spacetimedb.view(
  { name: 'visible_support_tickets', public: true },
  t.array(SupportTicketView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    const actor = ctx.db.account.id.find(accountId);
    if (!actor) return [];
    return [...ctx.db.supportTicket.iter()]
      .filter(row => actor.isAdmin || actor.isStaff || row.accountId === accountId)
      .map(row => ({
        id: row.id,
        reference: row.reference,
        subject: row.subject,
        status: row.status,
        priority: row.priority,
        assignee: row.assigneeId === undefined ? '' : ctx.db.account.id.find(row.assigneeId)?.username ?? '',
        orderId: row.orderId,
        refundTotal: row.refundTotal,
      }));
  }
);

const SupportReplyView = t.object('SupportReplyView', {
  ticketId: t.u64(),
  author: t.string(),
  body: t.string(),
});

export const visibleSupportReplies = spacetimedb.view(
  { name: 'visible_support_replies', public: true },
  t.array(SupportReplyView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    const actor = ctx.db.account.id.find(accountId);
    if (!actor) return [];
    const allowed = new Set([...ctx.db.supportTicket.iter()]
      .filter(row => actor.isAdmin || actor.isStaff || row.accountId === accountId)
      .map(row => row.id));
    return [...ctx.db.supportReply.iter()].filter(row => allowed.has(row.ticketId)).map(row => ({
      ticketId: row.ticketId,
      author: ctx.db.account.id.find(row.accountId)?.username ?? 'unknown',
      body: row.body,
    }));
  }
);

const PreferenceView = t.object('PreferenceView', {
  orderEnabled: t.bool(),
  stockEnabled: t.bool(),
});

export const myNotificationPreferences = spacetimedb.view(
  { name: 'my_notification_preferences', public: true },
  t.option(PreferenceView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return undefined;
    const row = ctx.db.notificationPreference.accountId.find(accountId);
    return row ? { orderEnabled: row.orderEnabled, stockEnabled: row.stockEnabled } : undefined;
  }
);

const NotificationView = t.object('NotificationView', {
  id: t.u64(),
  kind: t.string(),
  message: t.string(),
  unread: t.bool(),
});

export const myNotifications = spacetimedb.view(
  { name: 'my_notifications', public: true },
  t.array(NotificationView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    return [...ctx.db.notification.accountId.filter(accountId)].map(row => ({
      id: row.id,
      kind: row.kind,
      message: row.message,
      unread: row.unread,
    }));
  }
);

const ReservationView = t.object('ReservationView', {
  itemId: t.u64(),
  expiresMicros: t.i64(),
  expired: t.bool(),
});

export const myReservations = spacetimedb.view(
  { name: 'my_reservations', public: true },
  t.array(ReservationView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    return [...ctx.db.reservation.byAccountItem.filter(accountId)].map(row => ({
      itemId: row.itemId,
      expiresMicros: row.expiresMicros,
      expired: row.expired,
    }));
  }
);

const ExpiredCartView = t.object('ExpiredCartView', { itemId: t.u64(), quantity: t.u32() });

export const myExpiredCart = spacetimedb.view(
  { name: 'my_expired_cart', public: true },
  t.array(ExpiredCartView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    return [...ctx.db.expiredCartItem.accountId.filter(accountId)]
      .map(row => ({ itemId: row.itemId, quantity: row.quantity }));
  }
);

const RestockView = t.object('RestockView', {
  id: t.u64(),
  itemId: t.u64(),
  warehouseId: t.u64(),
  quantity: t.u32(),
  dueMicros: t.i64(),
  status: t.string(),
});

export const visibleRestocks = spacetimedb.view(
  { name: 'visible_restocks', public: true },
  t.array(RestockView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    const actor = accountId === null ? null : ctx.db.account.id.find(accountId);
    if (!actor || (!actor.isAdmin && !actor.isStaff)) return [];
    return [...ctx.db.scheduledRestock.iter()];
  }
);

const LedgerView = t.object('LedgerView', {
  itemId: t.u64(),
  warehouseId: t.u64(),
  quantity: t.u32(),
  source: t.string(),
});

export const visibleStockLedger = spacetimedb.view(
  { name: 'visible_stock_ledger', public: true },
  t.array(LedgerView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    const actor = accountId === null ? null : ctx.db.account.id.find(accountId);
    if (!actor || (!actor.isAdmin && !actor.isStaff)) return [];
    return [...ctx.db.stockLedger.iter()].map(row => ({
      itemId: row.itemId,
      warehouseId: row.warehouseId,
      quantity: row.quantity,
      source: row.source,
    }));
  }
);

const CompletedOrderView = t.object('CompletedOrderView', {
  orderId: t.u64(),
  status: t.string(),
  itemNames: t.array(t.string()),
});

export const completedOrders = spacetimedb.view(
  { name: 'completed_orders', public: true },
  t.array(CompletedOrderView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    const actor = accountId === null ? null : ctx.db.account.id.find(accountId);
    if (!actor || (!actor.isAdmin && !actor.isStaff)) return [];
    return [...ctx.db.customerOrder.iter()].filter(row => row.status !== 'pending')
      .map(row => ({
        orderId: row.id,
        status: row.status,
        itemNames: [...ctx.db.orderItem.orderId.filter(row.id)].map(itemRow => itemRow.itemName),
      }));
  }
);

const PromotionReportView = t.object('PromotionReportView', {
  promotionId: t.u64(),
  code: t.string(),
  redemptions: t.u32(),
  revenue: t.f64(),
});

const PromotionView = t.object('PromotionView', {
  id: t.u64(),
  code: t.string(),
  discountPercent: t.f64(),
  startMicros: t.i64(),
  endMicros: t.i64(),
  usageLimit: t.u32(),
});

export const visiblePromotions = spacetimedb.view(
  { name: 'visible_promotions', public: true },
  t.array(PromotionView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    const actor = accountId === null ? null : ctx.db.account.id.find(accountId);
    if (!actor || (!actor.isAdmin && !actor.isStaff)) return [];
    return [...ctx.db.promotion.iter()].map(row => ({
      id: row.id,
      code: row.code,
      discountPercent: row.discountPercent,
      startMicros: row.startMicros,
      endMicros: row.endMicros,
      usageLimit: row.usageLimit,
    }));
  }
);

export const promotionReports = spacetimedb.view(
  { name: 'promotion_reports', public: true },
  t.array(PromotionReportView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    const actor = accountId === null ? null : ctx.db.account.id.find(accountId);
    if (!actor || (!actor.isAdmin && !actor.isStaff)) return [];
    return [...ctx.db.promotion.iter()].map(promo => ({
      promotionId: promo.id,
      code: promo.code,
      redemptions: promo.redemptions,
      revenue: [...ctx.db.customerOrder.iter()]
        .filter(order => order.promotionId === promo.id)
        .reduce((total, order) => total + order.total, 0),
    }));
  }
);

export const visibleReorderRules = spacetimedb.view(
  { name: 'visible_reorder_rules', public: true },
  t.array(reorderRule.rowType),
  (ctx) => {
    const accountId = getAccountId(ctx);
    const actor = accountId === null ? null : ctx.db.account.id.find(accountId);
    return actor && (actor.isAdmin || actor.isStaff) ? [...ctx.db.reorderRule.iter()] : [];
  }
);

const DismissedRecommendationView = t.object('DismissedRecommendationView', { itemId: t.u64() });

export const myDismissedRecommendations = spacetimedb.view(
  { name: 'my_dismissed_recommendations', public: true },
  t.array(DismissedRecommendationView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    return [...ctx.db.recommendationDismissal.byAccountItem.filter(accountId)]
      .map(row => ({ itemId: row.itemId }));
  }
);
