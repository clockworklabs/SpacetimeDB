import { mutationGeneric, queryGeneric } from 'convex/server';
import { getAuthUserId, getAuthSessionId } from '@convex-dev/auth/server';
import { v, ConvexError } from 'convex/values';

async function account(ctx) {
  const userId = await getAuthUserId(ctx);
  const sessionId = await getAuthSessionId(ctx);
  const session = sessionId && await ctx.db.get(sessionId);
  if (!userId || !session || session.userId !== userId || session.expirationTime <= Date.now()) {
    throw new ConvexError('Sign in');
  }
  return userId;
}
export const current = queryGeneric({ args: {}, handler: async (ctx) => {
  const user = await ctx.db.get(await account(ctx));
  return { id: user._id, name: user.name };
}});
export const purchase = mutationGeneric({ args: { itemId: v.id('items'), quantity: v.number() }, handler: async (ctx, args) => {
  const buyer = await account(ctx);
  if (!Number.isSafeInteger(args.quantity) || args.quantity < 1) throw new ConvexError('Invalid quantity');
  const item = await ctx.db.get(args.itemId);
  if (!item || item.stock < args.quantity) throw new ConvexError('Insufficient stock');
  await ctx.db.patch(item._id, { stock: item.stock - args.quantity });
  return ctx.db.insert('orders', { itemId: item._id, buyer, quantity: args.quantity });
}});
