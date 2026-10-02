import { mutationGeneric, queryGeneric } from 'convex/server';
import { v, ConvexError } from 'convex/values';
import { account, operator, publicAccount, isAdmin, isStaff } from './accounts.js';

export const money = n => Math.round(n * 100) / 100;
export function quantity(n, allowZero = false) {
  if (!Number.isSafeInteger(n) || n < (allowZero ? 0 : 1)) throw new ConvexError('Invalid quantity');
}
export const rows = (ctx, table) => ctx.db.query(table).collect();
export const publicRow = row => ({ ...row, id: row._id });
export async function holding(ctx, itemId, warehouseId) {
  const row = await ctx.db.query('stock').withIndex('location', q => q.eq('item_id', itemId).eq('warehouse_id', warehouseId)).unique();
  if (!row) throw new ConvexError('Unknown stock location');
  return row;
}
export async function notifyRestock(ctx, itemId) {
  const item = await ctx.db.get(itemId);
  for (const alert of await ctx.db.query('stock_alert').withIndex('item', q => q.eq('itemId', itemId)).collect()) {
    if (alert.delivered) continue;
    await ctx.db.insert('notification', { accountId: alert.accountId, type: 'stock', message: `${item.name} is back in stock` });
    await ctx.db.patch(alert._id, { delivered: true });
  }
}
async function allocate(ctx, itemId, count, lineId) {
  const stocks = await ctx.db.query('stock').withIndex('item', q => q.eq('item_id', itemId)).collect();
  if (stocks.reduce((sum, row) => sum + row.quantity, 0) < count) throw new ConvexError('Not enough stock');
  for (const row of stocks) {
    const take = Math.min(row.quantity, count);
    if (take > 0) {
      await ctx.db.patch(row._id, { quantity: row.quantity - take });
      await ctx.db.insert('order_allocation', { order_line_id: lineId, warehouse_id: row.warehouse_id, quantity: take });
      count -= take;
    }
  }
}
async function purchase(ctx, user, lines) {
  if (!lines.length) throw new ConvexError('Cart is empty');
  const orderId = await ctx.db.insert('order_header', { account_id: user._id, total: 0, refunded: 0, status: 'pending' });
  let total = 0;
  for (const line of lines) {
    quantity(line.quantity);
    const item = await ctx.db.get(line.item_id);
    if (!item) throw new ConvexError('Unknown item');
    const lineId = await ctx.db.insert('order_line', { order_id: orderId, item_id: item._id,
      quantity: line.quantity, unit_price: item.price, returned: false });
    await allocate(ctx, item._id, line.quantity, lineId);
    total += Math.round(item.price * 100) * line.quantity;
  }
  await ctx.db.patch(orderId, { total: total / 100 });
  return { orderId };
}
export const buy = mutationGeneric({ args: { itemId: v.id('item') }, handler: async (ctx, { itemId }) =>
  purchase(ctx, await account(ctx), [{ item_id: itemId, quantity: 1 }]) });
export const checkout = mutationGeneric({ args: {}, handler: async ctx => {
  const user = await account(ctx);
  const lines = await ctx.db.query('order_cart').withIndex('account', q => q.eq('account_id', user._id)).collect();
  const result = await purchase(ctx, user, lines);
  for (const line of lines) await ctx.db.delete(line._id);
  return result;
}});
async function changeCart(ctx, itemId, amount, add) {
  const user = await account(ctx);
  quantity(amount, !add);
  if (!await ctx.db.get(itemId)) throw new ConvexError('Unknown item');
  const existing = await ctx.db.query('order_cart').withIndex('line', q => q.eq('account_id', user._id).eq('item_id', itemId)).unique();
  const next = add ? (existing?.quantity || 0) + amount : amount;
  const available = (await ctx.db.query('stock').withIndex('item', q => q.eq('item_id', itemId)).collect()).reduce((n, s) => n + s.quantity, 0);
  if (next > available) throw new ConvexError('Not enough stock');
  if (existing) {
    if (next) await ctx.db.patch(existing._id, { quantity: next });
    else await ctx.db.delete(existing._id);
  } else if (next) await ctx.db.insert('order_cart', { account_id: user._id, item_id: itemId, quantity: next });
}
export const cartAdd = mutationGeneric({ args: { itemId: v.id('item'), quantity: v.optional(v.number()) },
  handler: (ctx, args) => changeCart(ctx, args.itemId, args.quantity ?? 1, true) });
export const cartSetQuantity = mutationGeneric({ args: { itemId: v.id('item'), quantity: v.number() },
  handler: (ctx, args) => changeCart(ctx, args.itemId, args.quantity, false) });
export const restock = mutationGeneric({ args: { itemId: v.id('item'), warehouseId: v.id('warehouse'), quantity: v.number() },
  handler: async (ctx, args) => {
    await operator(ctx, true); quantity(args.quantity);
    const stock = await holding(ctx, args.itemId, args.warehouseId);
    await ctx.db.patch(stock._id, { quantity: stock.quantity + args.quantity });
    await notifyRestock(ctx, args.itemId);
  } });
export const transfer = mutationGeneric({ args: { itemId: v.id('item'), fromWarehouseId: v.id('warehouse'), toWarehouseId: v.id('warehouse'), quantity: v.number() },
  handler: async (ctx, args) => {
    await operator(ctx, true); quantity(args.quantity);
    if (args.fromWarehouseId === args.toWarehouseId) throw new ConvexError('Choose different warehouses');
    const source = await holding(ctx, args.itemId, args.fromWarehouseId);
    const target = await holding(ctx, args.itemId, args.toWarehouseId);
    if (source.quantity < args.quantity) throw new ConvexError('Not enough stock');
    await ctx.db.patch(source._id, { quantity: source.quantity - args.quantity });
    await ctx.db.patch(target._id, { quantity: target.quantity + args.quantity });
  } });
export const price = mutationGeneric({ args: { itemId: v.id('item'), price: v.number() }, handler: async (ctx, args) => {
  await operator(ctx, true);
  if (!Number.isFinite(args.price) || args.price < 0) throw new ConvexError('Invalid price');
  await ctx.db.patch(args.itemId, { price: money(args.price) });
}});
export const ship = mutationGeneric({ args: { orderId: v.id('order_header') }, handler: async (ctx, { orderId }) => {
  const user = await operator(ctx);
  if (user.roles.includes('inventory') && !isAdmin(user)) throw new ConvexError('Fulfilment access required');
  const order = await ctx.db.get(orderId);
  if (!order || order.status !== 'pending') throw new ConvexError('Order is not pending');
  await ctx.db.patch(orderId, { status: 'shipped' });
}});
async function ownedOrder(ctx, orderId) {
  const user = await account(ctx);
  const order = await ctx.db.get(orderId);
  if (!order || order.account_id !== user._id) throw new ConvexError('Order not found');
  return order;
}
async function restoreLine(ctx, line) {
  for (const allocation of await ctx.db.query('order_allocation').withIndex('line', q => q.eq('order_line_id', line._id)).collect()) {
    const stock = await holding(ctx, line.item_id, allocation.warehouse_id);
    await ctx.db.patch(stock._id, { quantity: stock.quantity + allocation.quantity });
  }
  await ctx.db.patch(line._id, { returned: true });
  await notifyRestock(ctx, line.item_id);
}
export const cancel = mutationGeneric({ args: { orderId: v.id('order_header') }, handler: async (ctx, { orderId }) => {
  const order = await ownedOrder(ctx, orderId);
  if (order.status !== 'pending') throw new ConvexError('Only pending orders can be cancelled');
  for (const line of await ctx.db.query('order_line').withIndex('order', q => q.eq('order_id', orderId)).collect()) await restoreLine(ctx, line);
  await ctx.db.patch(orderId, { status: 'cancelled', refunded: order.total });
}});
export const returnItem = mutationGeneric({ args: { orderId: v.id('order_header'), itemId: v.id('item') }, handler: async (ctx, { orderId, itemId }) => {
  const order = await ownedOrder(ctx, orderId);
  if (!['shipped', 'delivered'].includes(order.status)) throw new ConvexError('Order has not shipped');
  const line = (await ctx.db.query('order_line').withIndex('order', q => q.eq('order_id', orderId)).collect()).find(l => l.item_id === itemId);
  if (!line || line.returned) throw new ConvexError('Item cannot be returned');
  await restoreLine(ctx, line);
  await ctx.db.patch(orderId, { refunded: money(order.refunded + line.quantity * line.unit_price) });
}});
export const submitReview = mutationGeneric({ args: { itemId: v.id('item'), rating: v.number(), comment: v.string() }, handler: async (ctx, args) => {
  const user = await account(ctx);
  if (!Number.isInteger(args.rating) || args.rating < 1 || args.rating > 5 || args.comment.length > 4000) throw new ConvexError('Invalid review');
  const orders = await ctx.db.query('order_header').withIndex('account', q => q.eq('account_id', user._id)).collect();
  let purchased = false;
  for (const order of orders.filter(o => o.status !== 'cancelled')) {
    if ((await ctx.db.query('order_line').withIndex('order', q => q.eq('order_id', order._id)).collect()).some(l => l.item_id === args.itemId && !l.returned)) purchased = true;
  }
  if (!purchased) throw new ConvexError('Purchase this item before reviewing');
  if (await ctx.db.query('review').withIndex('author', q => q.eq('itemId', args.itemId).eq('accountId', user._id)).unique()) throw new ConvexError('Already reviewed');
  await ctx.db.insert('review', { ...args, accountId: user._id });
}});

export async function snapshot(ctx) {
  const user = await account(ctx, false);
  const [items, warehouses, stocks, headers, lines, allocations, reviews, accounts] = await Promise.all(
    ['item', 'warehouse', 'stock', 'order_header', 'order_line', 'order_allocation', 'review', 'order_account'].map(t => rows(ctx, t)));
  const itemById = new Map(items.map(i => [i._id, i]));
  const warehouseById = new Map(warehouses.map(w => [w._id, w]));
  const accountById = new Map(accounts.map(a => [a._id, a]));
  const orderById = new Map(headers.map(o => [o._id, o]));
  const standing = lines.filter(l => !l.returned && orderById.get(l.order_id)?.status !== 'cancelled');
  const catalog = items.map(i => ({ ...publicRow(i), stock: stocks.filter(s => s.item_id === i._id).reduce((n, s) => n + s.quantity, 0),
    purchaseCount: standing.filter(l => l.item_id === i._id).reduce((n, l) => n + l.quantity, 0) }))
    .sort((a, b) => b.purchaseCount - a.purchaseCount || a.name.localeCompare(b.name));
  const details = catalog.map(i => {
    const itemReviews = reviews.filter(r => r.itemId === i._id).map(r => ({ id: r._id, itemId: r.itemId,
      userId: r.accountId, username: accountById.get(r.accountId)?.username || '', rating: r.rating, comment: r.comment,
      createdAt: new Date(r._creationTime).toISOString() }));
    return { ...i, reviews: itemReviews, average: itemReviews.length ? itemReviews.reduce((n, r) => n + r.rating, 0) / itemReviews.length : 0 };
  });
  const orderViews = headers.map(o => ({ id: o._id, total: o.total, status: o.status, refundTotal: o.refunded,
    createdAt: new Date(o._creationTime).toISOString(), items: lines.filter(l => l.order_id === o._id).map(l => ({
      itemId: l.item_id, name: itemById.get(l.item_id)?.name || '', price: l.unit_price, quantity: l.quantity,
      returned: l.returned, warehouseNames: allocations.filter(a => a.order_line_id === l._id).map(a => warehouseById.get(a.warehouse_id)?.name || '') })) }));
  const ownOrderIds = new Set(headers.filter(o => o.account_id === user?._id).map(o => o._id));
  const cartRows = user ? await ctx.db.query('order_cart').withIndex('account', q => q.eq('account_id', user._id)).collect() : [];
  const cartItems = cartRows.map(l => ({ itemId: l.item_id, quantity: l.quantity, ...Object.fromEntries(['name', 'price', 'stock'].map(k => [k, catalog.find(i => i.id === l.item_id)?.[k] ?? 0])) }));
  const purchasedCategories = new Set(standing.filter(l => ownOrderIds.has(l.order_id)).map(l => itemById.get(l.item_id)?.category));
  const recommendations = user ? catalog.filter(i => purchasedCategories.has(i.category) && !cartRows.some(l => l.item_id === i.id)) : catalog.slice(0, 10);
  const categories = [...new Set(items.map(i => i.category))].map(category => {
    const sold = standing.filter(l => itemById.get(l.item_id)?.category === category);
    return { category, units: sold.reduce((n, l) => n + l.quantity, 0), revenue: money(sold.reduce((n, l) => n + l.quantity * l.unit_price, 0)) };
  });
  return { user: publicAccount(user), items: catalog, details, recommended: recommendations,
    cart: { items: cartItems, total: money(cartItems.reduce((n, l) => n + l.price * l.quantity, 0)) },
    orders: orderViews.filter(o => ownOrderIds.has(o.id)),
    fulfilment: isStaff(user) && (!user.roles.includes('inventory') || isAdmin(user)) ? { orders: orderViews.filter(o => o.status === 'pending'), depth: headers.filter(o => o.status === 'pending').length } : null,
    admin: isAdmin(user) ? { items: catalog, warehouses: warehouses.map(w => ({ ...publicRow(w), total: stocks.filter(s => s.warehouse_id === w._id).reduce((n, s) => n + s.quantity, 0) })),
      locations: stocks.map(s => ({ id: s._id, itemId: s.item_id, warehouseId: s.warehouse_id, itemName: itemById.get(s.item_id)?.name,
        warehouseName: warehouseById.get(s.warehouse_id)?.name, quantity: s.quantity })),
      revenue: money(headers.reduce((n, o) => n + o.total - o.refunded, 0)), categories,
      lowStock: catalog.filter(i => i.stock <= 10).sort((a, b) => a.stock - b.stock), queueDepth: headers.filter(o => o.status === 'pending').length } : null,
  };
}
export const state = queryGeneric({ args: {}, handler: snapshot });
