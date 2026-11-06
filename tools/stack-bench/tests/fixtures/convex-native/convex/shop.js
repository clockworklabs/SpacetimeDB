import { internalMutationGeneric, mutationGeneric, queryGeneric } from 'convex/server';
import { v, ConvexError } from 'convex/values';
export const seed = internalMutationGeneric({ args: {}, handler: async (ctx) => {
  if ((await ctx.db.query('items').collect()).length) throw new Error('Seed needs a fresh deployment');
  await ctx.db.insert('items', { name: 'Widget', stock: 10, marker: 'preserve-target' });
  await ctx.db.insert('items', { name: 'Other', stock: 31, marker: 'preserve-other' });
  return null;
}});
export const list = queryGeneric({ args: {}, handler: async (ctx) => {
  if (!await ctx.auth.getUserIdentity()) throw new ConvexError('Sign in');
  return ctx.db.query('items').collect();
}});
export const purchase = mutationGeneric({ args: { itemId: v.id('items'), quantity: v.number() }, handler: async (ctx, args) => {
  const who = await ctx.auth.getUserIdentity();
  if (!who || who.subject !== 'buyer') throw new ConvexError('Purchase denied');
  if (!Number.isSafeInteger(args.quantity) || args.quantity < 1) throw new ConvexError('Invalid quantity');
  const item = await ctx.db.get(args.itemId);
  if (!item || item.stock < args.quantity) throw new ConvexError('Insufficient stock');
  await ctx.db.patch(item._id, { stock: item.stock - args.quantity });
  return ctx.db.insert('orders', { itemId: item._id, buyer: who.subject, quantity: args.quantity });
}});
export const deliberateError = mutationGeneric({ args: {}, handler: () => { throw new ConvexError('C0 deliberate error'); } });
export const unhandledError = mutationGeneric({ args: {}, handler: () => { throw new Error('C0 unhandled error'); } });
export const writeMarker = internalMutationGeneric({ args: { value: v.string() }, handler: async (ctx, { value }) => {
  await ctx.db.insert('markers', { value });
}});
export const scheduleMarker = internalMutationGeneric({ args: { value: v.string(), delayMillis: v.number() }, handler: async (ctx, args) => {
  const jobId = await ctx.scheduler.runAfter(args.delayMillis, 'shop:writeMarker', { value: args.value });
  return await ctx.db.system.get(jobId);
}});
