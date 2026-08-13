import {
  t,
  SenderError,
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
    ctx.db.account.insert({
      id: 0n,
      username: 'staff',
      passwordHash: hashPassword('stackbench-staff-2026'),
      isAdmin: false,
      isStaff: true,
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
});

export const myOrders = spacetimedb.view(
  { name: 'my_orders', public: true },
  t.array(MyOrderView),
  (ctx) => {
    const accountId = getAccountId(ctx);
    if (accountId === null) return [];
    const rows = [...ctx.db.customerOrder.accountId.filter(accountId)];
    rows.sort((a, b) => (b.createdAt.microsSinceUnixEpoch > a.createdAt.microsSinceUnixEpoch ? 1 : -1));
    return rows.map((o) => ({ orderId: o.id, createdAt: o.createdAt, total: o.total, status: o.status }));
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
    for (const order of ctx.db.customerOrder.accountId.filter(accountId)) {
      if (!isOrderCounted(order)) continue;
      for (const li of ctx.db.orderItem.orderId.filter(order.id)) {
        if (li.returned) continue;
        const link = ctx.db.itemCategory.itemId.find(li.itemId);
        if (link) purchasedCategoryIds.add(link.categoryId);
      }
    }
    if (purchasedCategoryIds.size === 0) return [];

    const cartItemIds = new Set<bigint>();
    for (const c of ctx.db.cartItem.byAccountItem.filter(accountId)) cartItemIds.add(c.itemId);

    const candidates: Array<{ itemId: bigint; name: string; price: number }> = [];
    for (const link of ctx.db.itemCategory.iter()) {
      if (!purchasedCategoryIds.has(link.categoryId)) continue;
      if (cartItemIds.has(link.itemId)) continue;
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
});

export const addToCart = spacetimedb.reducer({ itemId: t.u64() }, (ctx, { itemId }) => {
  const acc = requireAccount(ctx);
  const it = ctx.db.item.id.find(itemId);
  if (!it) throw new SenderError('Item not found.');

  const existing = findCartLine(ctx, acc.id, itemId);
  if (existing) {
    ctx.db.cartItem.id.update({ ...existing, quantity: existing.quantity + 1 });
  } else {
    ctx.db.cartItem.insert({ id: 0n, accountId: acc.id, itemId, quantity: 1 });
  }
});

export const updateCartQuantity = spacetimedb.reducer(
  { itemId: t.u64(), quantity: t.i32() },
  (ctx, { itemId, quantity }) => {
    const acc = requireAccount(ctx);
    if (quantity < 1) throw new SenderError('Quantity must be at least 1.');
    const existing = findCartLine(ctx, acc.id, itemId);
    if (!existing) throw new SenderError('That item is not in your cart.');
    ctx.db.cartItem.id.update({ ...existing, quantity });
  }
);

export const removeFromCart = spacetimedb.reducer({ itemId: t.u64() }, (ctx, { itemId }) => {
  const acc = requireAccount(ctx);
  const existing = findCartLine(ctx, acc.id, itemId);
  if (existing) ctx.db.cartItem.id.delete(existing.id);
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
    const available = totalStock(ctx, line.itemId);
    if (available < line.quantity) {
      throw new SenderError(`Not enough stock for ${it.name}: only ${available} left.`);
    }
    priced.push({ itemId: it.id, name: it.name, quantity: line.quantity, price: it.price });
    total += it.price * line.quantity;
  }

  const order = ctx.db.customerOrder.insert({
    id: 0n,
    accountId: acc.id,
    createdAt: ctx.timestamp,
    total,
    status: 'pending',
  });

  for (const p of priced) {
    const allocations = decrementStockTracked(ctx, p.itemId, p.quantity);
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
  }

  for (const line of lines) ctx.db.cartItem.id.delete(line.id);
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
});

// --- cancellation & returns ---

export const cancelOrder = spacetimedb.reducer({ orderId: t.u64() }, (ctx, { orderId }) => {
  const order = requireOrderOwner(ctx, orderId);
  if (order.status !== 'pending') throw new SenderError('Order has already shipped.');

  for (const li of ctx.db.orderItem.orderId.filter(order.id)) {
    restoreOrderItemStock(ctx, li);
    decrementPurchaseCount(ctx, li.itemId, li.quantity);
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
  }
);
