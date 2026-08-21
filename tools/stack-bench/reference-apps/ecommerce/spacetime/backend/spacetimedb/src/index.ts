import { SenderError, t, type InferSchema, type ReducerCtx, type ViewCtx } from 'spacetimedb/server';
import spacetimedb from './schema';

export { default } from './schema';

type S = InferSchema<typeof spacetimedb>;
type Ctx = ReducerCtx<S>;
type VCtx = ViewCtx<S>;
type AnyCtx = Ctx | VCtx;

// --- Password hashing (no crypto module is available inside the module runtime) ---

function hashPassword(password: string, salt: string): string {
  const combined = `${salt}:${password}`;
  let h1 = 0x811c9dc5 >>> 0;
  for (let i = 0; i < combined.length; i++) {
    h1 ^= combined.charCodeAt(i);
    h1 = Math.imul(h1, 0x01000193) >>> 0;
  }
  let h2 = 0x1000193 >>> 0;
  for (let i = combined.length - 1; i >= 0; i--) {
    h2 ^= combined.charCodeAt(i);
    h2 = Math.imul(h2, 0x811c9dc5) >>> 0;
  }
  return h1.toString(16).padStart(8, '0') + h2.toString(16).padStart(8, '0');
}

function randomSalt(ctx: Ctx): string {
  const bytes = ctx.random.fill(new Uint8Array(8));
  return Array.from(bytes)
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
}

// --- Auth helpers ---

function findAccount(ctx: AnyCtx) {
  const s = ctx.db.session.identity.find(ctx.sender);
  if (!s) return null;
  return ctx.db.account.id.find(s.accountId);
}

function requireAccount(ctx: Ctx) {
  const acc = findAccount(ctx);
  if (!acc) throw new SenderError('You must be signed in.');
  return acc;
}

function requireAdmin(ctx: Ctx) {
  const acc = requireAccount(ctx);
  if (!acc.isAdmin) throw new SenderError('Admin access required.');
  return acc;
}

function getOrCreateCart(ctx: Ctx, accountId: bigint) {
  const existing = ctx.db.cart.accountId.find(accountId);
  if (existing) return existing;
  return ctx.db.cart.insert({ id: 0n, accountId });
}

function totalStock(ctx: AnyCtx, itemId: bigint): number {
  let sum = 0;
  for (const row of ctx.db.stock.byItemWarehouse.filter(itemId)) sum += row.quantity;
  return sum;
}

function decrementStock(ctx: Ctx, itemId: bigint, qty: number) {
  let remaining = qty;
  const rows = [...ctx.db.stock.byItemWarehouse.filter(itemId)];
  for (const row of rows) {
    if (remaining <= 0) break;
    if (row.quantity <= 0) continue;
    const take = Math.min(row.quantity, remaining);
    ctx.db.stock.id.update({ ...row, quantity: row.quantity - take });
    remaining -= take;
  }
  if (remaining > 0) throw new SenderError('Not enough stock.');
}

// --- Lifecycle ---

export const init = spacetimedb.init((ctx) => {
  if ([...ctx.db.item.iter()].length > 0) return;

  const east = ctx.db.warehouse.insert({ id: 0n, name: 'East' });
  const west = ctx.db.warehouse.insert({ id: 0n, name: 'West' });

  const catalogue: Array<{ name: string; price: number; east: number; west: number }> = [
    { name: 'Air Purifier', price: 189.0, east: 60, west: 40 },
    { name: 'Bluetooth Speaker', price: 79.5, east: 50, west: 50 },
    { name: 'Coffee Grinder', price: 64.0, east: 70, west: 30 },
    { name: 'Desk Lamp', price: 42.0, east: 55, west: 45 },
    { name: 'Espresso Machine', price: 449.0, east: 80, west: 20 },
    { name: 'Gaming Mouse', price: 59.0, east: 50, west: 50 },
    { name: 'Headphones', price: 199.0, east: 60, west: 40 },
    { name: 'Induction Cooktop', price: 329.0, east: 50, west: 50 },
    { name: 'Keyboard', price: 89.0, east: 70, west: 30 },
    { name: 'Laptop Stand', price: 29.0, east: 90, west: 10 },
    { name: 'Mirrorless Camera', price: 1299.0, east: 2, west: 1 },
    { name: 'Webcam', price: 69.0, east: 60, west: 40 },
  ];

  for (const c of catalogue) {
    const it = ctx.db.item.insert({ id: 0n, name: c.name, price: c.price, purchaseCount: 0 });
    ctx.db.stock.insert({ id: 0n, itemId: it.id, warehouseId: east.id, quantity: c.east });
    ctx.db.stock.insert({ id: 0n, itemId: it.id, warehouseId: west.id, quantity: c.west });
  }

  const salt = randomSalt(ctx);
  const passwordHash = hashPassword('stackbench-admin-2026', salt);
  ctx.db.account.insert({
    id: 0n,
    username: 'admin',
    passwordSalt: salt,
    passwordHash,
    isAdmin: true,
  });
});

// --- Auth reducers ---

export const signUp = spacetimedb.reducer(
  { username: t.string(), password: t.string() },
  (ctx, { username, password }) => {
    const uname = username.trim();
    if (uname.length === 0) throw new SenderError('Username is required.');
    if (password.length === 0) throw new SenderError('Password is required.');
    const existing = ctx.db.account.username.find(uname);
    if (existing) throw new SenderError('Username is already taken.');

    const salt = randomSalt(ctx);
    const passwordHash = hashPassword(password, salt);
    const acc = ctx.db.account.insert({
      id: 0n,
      username: uname,
      passwordSalt: salt,
      passwordHash,
      isAdmin: false,
    });
    ctx.db.cart.insert({ id: 0n, accountId: acc.id });

    const existingSession = ctx.db.session.identity.find(ctx.sender);
    if (existingSession) ctx.db.session.identity.update({ ...existingSession, accountId: acc.id });
    else ctx.db.session.insert({ identity: ctx.sender, accountId: acc.id });
  }
);

export const signIn = spacetimedb.reducer(
  { username: t.string(), password: t.string() },
  (ctx, { username, password }) => {
    const acc = ctx.db.account.username.find(username.trim());
    if (!acc) throw new SenderError('Invalid username or password.');
    const hash = hashPassword(password, acc.passwordSalt);
    if (hash !== acc.passwordHash) throw new SenderError('Invalid username or password.');

    const existingSession = ctx.db.session.identity.find(ctx.sender);
    if (existingSession) ctx.db.session.identity.update({ ...existingSession, accountId: acc.id });
    else ctx.db.session.insert({ identity: ctx.sender, accountId: acc.id });
  }
);

export const signOut = spacetimedb.reducer((ctx) => {
  const existingSession = ctx.db.session.identity.find(ctx.sender);
  if (existingSession) ctx.db.session.identity.delete(ctx.sender);
});

// --- Buying ---

export const buyNow = spacetimedb.reducer({ itemId: t.u64() }, (ctx, { itemId }) => {
  const acc = requireAccount(ctx);
  const it = ctx.db.item.id.find(itemId);
  if (!it) throw new SenderError('Item not found.');
  const avail = totalStock(ctx, itemId);
  if (avail <= 0) throw new SenderError('This item is out of stock.');

  decrementStock(ctx, itemId, 1);
  ctx.db.item.id.update({ ...it, purchaseCount: it.purchaseCount + 1 });

  const ord = ctx.db.order.insert({ id: 0n, accountId: acc.id, total: it.price, createdAt: ctx.timestamp });
  ctx.db.orderItem.insert({
    id: 0n,
    orderId: ord.id,
    itemId,
    itemName: it.name,
    quantity: 1,
    price: it.price,
  });
});

// --- Cart ---

export const addToCart = spacetimedb.reducer({ itemId: t.u64() }, (ctx, { itemId }) => {
  const acc = requireAccount(ctx);
  const it = ctx.db.item.id.find(itemId);
  if (!it) throw new SenderError('Item not found.');

  const cartRow = getOrCreateCart(ctx, acc.id);
  const existing = [...ctx.db.cartItem.byCartItem.filter([cartRow.id, itemId])][0];
  if (existing) {
    ctx.db.cartItem.id.update({ ...existing, quantity: existing.quantity + 1 });
  } else {
    ctx.db.cartItem.insert({ id: 0n, cartId: cartRow.id, itemId, quantity: 1 });
  }
});

export const updateCartQuantity = spacetimedb.reducer(
  { itemId: t.u64(), quantity: t.i32() },
  (ctx, { itemId, quantity }) => {
    const acc = requireAccount(ctx);
    if (quantity <= 0) throw new SenderError('Quantity must be at least 1.');

    const cartRow = getOrCreateCart(ctx, acc.id);
    const existing = [...ctx.db.cartItem.byCartItem.filter([cartRow.id, itemId])][0];
    if (!existing) throw new SenderError('That item is not in your cart.');
    ctx.db.cartItem.id.update({ ...existing, quantity });
  }
);

export const removeFromCart = spacetimedb.reducer({ itemId: t.u64() }, (ctx, { itemId }) => {
  const acc = requireAccount(ctx);
  const cartRow = getOrCreateCart(ctx, acc.id);
  const existing = [...ctx.db.cartItem.byCartItem.filter([cartRow.id, itemId])][0];
  if (existing) ctx.db.cartItem.id.delete(existing.id);
});

export const checkout = spacetimedb.reducer((ctx) => {
  const acc = requireAccount(ctx);
  const cartRow = getOrCreateCart(ctx, acc.id);
  const lines = [...ctx.db.cartItem.byCartItem.filter(cartRow.id)];
  if (lines.length === 0) throw new SenderError('Your cart is empty.');

  const prepared = lines.map(line => {
    const it = ctx.db.item.id.find(line.itemId);
    if (!it) throw new SenderError('Item not found.');
    const avail = totalStock(ctx, line.itemId);
    if (avail < line.quantity) throw new SenderError(`Not enough stock for ${it.name}.`);
    return { line, it };
  });

  const ord = ctx.db.order.insert({ id: 0n, accountId: acc.id, total: 0, createdAt: ctx.timestamp });
  let total = 0;
  for (const { line, it } of prepared) {
    decrementStock(ctx, line.itemId, line.quantity);
    ctx.db.item.id.update({ ...it, purchaseCount: it.purchaseCount + line.quantity });
    ctx.db.orderItem.insert({
      id: 0n,
      orderId: ord.id,
      itemId: line.itemId,
      itemName: it.name,
      quantity: line.quantity,
      price: it.price,
    });
    total += it.price * line.quantity;
    ctx.db.cartItem.id.delete(line.id);
  }
  ctx.db.order.id.update({ ...ord, total });
});

// --- Reviews ---

export const submitReview = spacetimedb.reducer(
  { itemId: t.u64(), rating: t.u8(), comment: t.string() },
  (ctx, { itemId, rating, comment }) => {
    const acc = requireAccount(ctx);
    if (rating < 1 || rating > 5) throw new SenderError('Rating must be between 1 and 5.');

    const hasPurchased = [...ctx.db.order.byAccount.filter(acc.id)].some(o =>
      [...ctx.db.orderItem.byOrder.filter(o.id)].some(oi => oi.itemId === itemId)
    );
    if (!hasPurchased) throw new SenderError('You can only review items you have purchased.');

    const existing = [...ctx.db.review.byItemAccount.filter([itemId, acc.id])][0];
    if (existing) {
      ctx.db.review.id.update({ ...existing, rating, comment, createdAt: ctx.timestamp });
    } else {
      ctx.db.review.insert({ id: 0n, itemId, accountId: acc.id, rating, comment, createdAt: ctx.timestamp });
    }
  }
);

// --- Admin ---

export const adminRestock = spacetimedb.reducer(
  { itemId: t.u64(), warehouseId: t.u64(), quantity: t.u32() },
  (ctx, { itemId, warehouseId, quantity }) => {
    requireAdmin(ctx);
    if (quantity <= 0) throw new SenderError('Quantity must be positive.');

    const existing = [...ctx.db.stock.byItemWarehouse.filter([itemId, warehouseId])][0];
    if (existing) {
      ctx.db.stock.id.update({ ...existing, quantity: existing.quantity + quantity });
    } else {
      ctx.db.stock.insert({ id: 0n, itemId, warehouseId, quantity });
    }
  }
);

// --- Views ---

const AccountInfo = t.object('AccountInfo', {
  id: t.u64(),
  username: t.string(),
  isAdmin: t.bool(),
});

export const myAccount = spacetimedb.view(
  { name: 'my_account', public: true },
  t.option(AccountInfo),
  (ctx) => {
    const acc = findAccount(ctx);
    if (!acc) return undefined;
    return { id: acc.id, username: acc.username, isAdmin: acc.isAdmin };
  }
);

const CartLineInfo = t.object('CartLineInfo', {
  itemId: t.u64(),
  itemName: t.string(),
  quantity: t.u32(),
  unitPrice: t.f64(),
});

export const myCart = spacetimedb.view(
  { name: 'my_cart', public: true },
  t.array(CartLineInfo),
  (ctx) => {
    const acc = findAccount(ctx);
    if (!acc) return [];
    const cartRow = ctx.db.cart.accountId.find(acc.id);
    if (!cartRow) return [];
    const out: Array<{ itemId: bigint; itemName: string; quantity: number; unitPrice: number }> = [];
    for (const line of ctx.db.cartItem.byCartItem.filter(cartRow.id)) {
      const it = ctx.db.item.id.find(line.itemId);
      if (!it) continue;
      out.push({ itemId: line.itemId, itemName: it.name, quantity: line.quantity, unitPrice: it.price });
    }
    return out;
  }
);

const OrderSummary = t.object('OrderSummary', {
  orderId: t.u64(),
  total: t.f64(),
  createdAt: t.timestamp(),
});

export const myOrders = spacetimedb.view(
  { name: 'my_orders', public: true },
  t.array(OrderSummary),
  (ctx) => {
    const acc = findAccount(ctx);
    if (!acc) return [];
    return [...ctx.db.order.byAccount.filter(acc.id)].map(o => ({
      orderId: o.id,
      total: o.total,
      createdAt: o.createdAt,
    }));
  }
);

const OrderLineInfo = t.object('OrderLineInfo', {
  orderId: t.u64(),
  itemId: t.u64(),
  itemName: t.string(),
  quantity: t.u32(),
  price: t.f64(),
});

export const myOrderItems = spacetimedb.view(
  { name: 'my_order_items', public: true },
  t.array(OrderLineInfo),
  (ctx) => {
    const acc = findAccount(ctx);
    if (!acc) return [];
    const out: Array<{ orderId: bigint; itemId: bigint; itemName: string; quantity: number; price: number }> = [];
    for (const o of ctx.db.order.byAccount.filter(acc.id)) {
      for (const oi of ctx.db.orderItem.byOrder.filter(o.id)) {
        out.push({ orderId: o.id, itemId: oi.itemId, itemName: oi.itemName, quantity: oi.quantity, price: oi.price });
      }
    }
    return out;
  }
);

const RevenueInfo = t.object('RevenueInfo', {
  total: t.f64(),
});

export const adminRevenue = spacetimedb.view(
  { name: 'admin_revenue', public: true },
  t.option(RevenueInfo),
  (ctx) => {
    const acc = findAccount(ctx);
    if (!acc || !acc.isAdmin) return { total: 0 };
    let total = 0;
    for (const o of ctx.db.order.iter()) total += o.total;
    return { total };
  }
);
