import { mutationGeneric, queryGeneric, internalMutationGeneric, makeFunctionReference } from 'convex/server';
import { v, ConvexError } from 'convex/values';
import { account, operator, publicAccount, isAdmin, isStaff } from './accounts.js';
import { holding, notifyRestock, quantity, rows, publicRow, money } from './shop.js';

export const state = queryGeneric({ args: {}, handler: async ctx => {
  const user = await account(ctx, false);
  const staff = isStaff(user);
  const tickets = (await rows(ctx, 'support')).filter(t => staff || (user && t.accountId === user._id)).map(publicRow);
  return { user: publicAccount(user), profile: user?.profile || null, tickets,
    preference: user?.preference || { order: false, stock: false },
    notifications: user ? (await ctx.db.query('notification').withIndex('account', q => q.eq('accountId', user._id)).collect()).map(publicRow) : [],
    promotions: staff ? (await rows(ctx, 'promotion')).map(p => ({ ...publicRow(p), discount: p.discountPercent,
      start: new Date(p.startMicros / 1000).toISOString().slice(0, 10), end: new Date(p.endMicros / 1000).toISOString().slice(0, 10), limit: p.usageLimit })) : [],
    scheduledRestocks: staff ? (await rows(ctx, 'scheduled_restock')).filter(r => r.status === 'pending').map(r => ({ ...publicRow(r), dueAt: new Date(r.dueAt).toISOString() })) : [],
    ledger: staff ? (await rows(ctx, 'stock_ledger')).map(publicRow) : [],
    staffUsers: isAdmin(user) ? (await rows(ctx, 'order_account')).filter(isStaff).map(publicAccount) : [],
  };
}});
export const saveProfile = mutationGeneric({ args: { name: v.string(), address: v.string() }, handler: async (ctx, profile) => {
  const user = await account(ctx);
  if (!profile.name.trim() || !profile.address.trim()) throw new ConvexError('Name and address are required');
  await ctx.db.patch(user._id, { profile });
}});
export const savePreferences = mutationGeneric({ args: { order: v.boolean(), stock: v.boolean() }, handler: async (ctx, preference) => {
  await ctx.db.patch((await account(ctx))._id, { preference });
}});
export const submitSupport = mutationGeneric({ args: { email: v.string(), subject: v.string(), message: v.string() }, handler: async (ctx, args) => {
  const user = await account(ctx, false);
  if (!args.email.trim() || !args.subject.trim() || !args.message.trim()) throw new ConvexError('Complete the support form');
  const id = await ctx.db.insert('support', { ...args, accountId: user?._id || null,
    reference: '', status: 'new', priority: 'normal', assignee: '', replies: [] });
  const reference = `SUP-${id.slice(-10)}`;
  await ctx.db.patch(id, { reference });
  return { ticket: { id, reference } };
}});
export const updateSupport = mutationGeneric({ args: { ticketId: v.id('support'), assignee: v.string(), priority: v.string(), status: v.string() }, handler: async (ctx, { ticketId, ...change }) => {
  await operator(ctx);
  if (!await ctx.db.get(ticketId)) throw new ConvexError('Ticket not found');
  await ctx.db.patch(ticketId, change);
}});
export const replySupport = mutationGeneric({ args: { ticketId: v.id('support'), body: v.string() }, handler: async (ctx, { ticketId, body }) => {
  const user = await account(ctx);
  const ticket = await ctx.db.get(ticketId);
  if (!ticket || (!isStaff(user) && ticket.accountId !== user._id)) throw new ConvexError('Ticket not found');
  if (!body.trim()) throw new ConvexError('Reply is empty');
  await ctx.db.patch(ticketId, { replies: [...ticket.replies, { username: user.username, body, createdAt: Date.now() }] });
}});
export const assignStaffRole = mutationGeneric({ args: { accountId: v.id('order_account'), role: v.string() }, handler: async (ctx, { accountId, role }) => {
  await operator(ctx, true);
  if (!['staff', 'inventory', 'admin'].includes(role)) throw new ConvexError('Invalid role');
  if (!await ctx.db.get(accountId)) throw new ConvexError('Account not found');
  await ctx.db.patch(accountId, { roles: [role] });
}});
export const createPromotion = mutationGeneric({ args: { code: v.string(), discountPercent: v.number(), startMicros: v.number(), endMicros: v.number(), usageLimit: v.number() }, handler: async (ctx, args) => {
  await operator(ctx);
  if (!args.code.trim() || !Number.isFinite(args.discountPercent) || args.discountPercent <= 0 || args.discountPercent > 100
    || !Number.isSafeInteger(args.startMicros) || !Number.isSafeInteger(args.endMicros) || args.endMicros <= args.startMicros
    || !Number.isSafeInteger(args.usageLimit) || args.usageLimit <= 0) throw new ConvexError('Invalid promotion');
  await ctx.db.insert('promotion', args);
}});
export const saveCatalog = mutationGeneric({ args: { name: v.string(), category: v.string(), price: v.number(), variants: v.array(v.string()) }, handler: async (ctx, args) => {
  await operator(ctx, true);
  if (!args.name.trim() || !args.category.trim() || !Number.isFinite(args.price) || args.price < 0) throw new ConvexError('Invalid product');
  const id = await ctx.db.insert('item', { ...args, price: money(args.price), description: args.name });
  for (const warehouse of await rows(ctx, 'warehouse')) await ctx.db.insert('stock', { item_id: id, warehouse_id: warehouse._id, quantity: 0 });
}});
export const stockAlert = mutationGeneric({ args: { itemId: v.id('item') }, handler: async (ctx, { itemId }) => {
  const user = await account(ctx);
  if (!await ctx.db.get(itemId)) throw new ConvexError('Item not found');
  const existing = await ctx.db.query('stock_alert').withIndex('owner', q => q.eq('accountId', user._id).eq('itemId', itemId)).unique();
  if (!existing) await ctx.db.insert('stock_alert', { accountId: user._id, itemId, delivered: false });
}});
export const scheduleRestock = mutationGeneric({ args: { item: v.string(), warehouse: v.string(), quantity: v.number(), delaySeconds: v.number() }, handler: async (ctx, args) => {
  await operator(ctx, true); quantity(args.quantity); quantity(args.delaySeconds, true);
  const item = await ctx.db.query('item').withIndex('name', q => q.eq('name', args.item)).unique();
  const warehouse = await ctx.db.query('warehouse').withIndex('name', q => q.eq('name', args.warehouse)).unique();
  if (!item || !warehouse) throw new ConvexError('Unknown item or warehouse');
  const restockId = await ctx.db.insert('scheduled_restock', { itemId: item._id, warehouseId: warehouse._id,
    quantity: args.quantity, dueAt: Date.now() + args.delaySeconds * 1000, status: 'pending' });
  const scheduledId = await ctx.scheduler.runAfter(args.delaySeconds * 1000, makeFunctionReference('progression:completeRestock'), { restockId });
  await ctx.db.patch(restockId, { scheduledId });
}});
export const completeRestock = internalMutationGeneric({ args: { restockId: v.id('scheduled_restock') }, handler: async (ctx, { restockId }) => {
  const restock = await ctx.db.get(restockId);
  if (!restock || restock.status !== 'pending') return;
  const stock = await holding(ctx, restock.itemId, restock.warehouseId);
  await ctx.db.patch(stock._id, { quantity: stock.quantity + restock.quantity });
  await ctx.db.patch(restockId, { status: 'completed' });
  await ctx.db.insert('stock_ledger', { itemId: restock.itemId, quantity: restock.quantity });
  await notifyRestock(ctx, restock.itemId);
}});
export const cancelScheduledRestock = mutationGeneric({ args: { restockId: v.id('scheduled_restock') }, handler: async (ctx, { restockId }) => {
  await operator(ctx, true);
  const restock = await ctx.db.get(restockId);
  if (!restock || restock.status !== 'pending') throw new ConvexError('Restock is not pending');
  await ctx.scheduler.cancel(restock.scheduledId);
  await ctx.db.patch(restock._id, { status: 'cancelled' });
}});
