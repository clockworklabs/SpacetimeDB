import { internalMutationGeneric } from 'convex/server';
import { getAuthSessionId, getAuthUserId } from '@convex-dev/auth/server';
import { v, ConvexError } from 'convex/values';

export const ensure = internalMutationGeneric({ args: { userId: v.id('users') }, handler: async (ctx, { userId }) => {
  if (await ctx.db.query('order_account').withIndex('auth_user', q => q.eq('auth_user_id', userId)).unique()) return;
  const user = await ctx.db.get(userId);
  await ctx.db.insert('order_account', { auth_user_id: userId, username: user.name, roles: [],
    profile: { name: '', address: '' }, preference: { order: false, stock: false } });
}});

export async function account(ctx, required = true) {
  const userId = await getAuthUserId(ctx);
  const sessionId = await getAuthSessionId(ctx);
  const session = sessionId && await ctx.db.get(sessionId);
  const valid = userId && session && session.userId === userId && session.expirationTime > Date.now();
  const row = valid && await ctx.db.query('order_account').withIndex('auth_user', q => q.eq('auth_user_id', userId)).unique();
  if (!row && required) throw new ConvexError('Sign in required');
  return row || null;
}
export const isAdmin = a => Boolean(a?.roles.includes('admin'));
export const isStaff = a => Boolean(a?.roles.some(role => ['admin', 'staff', 'inventory'].includes(role)));
export async function operator(ctx, admin = false) {
  const user = await account(ctx);
  if (!(admin ? isAdmin(user) : isStaff(user))) throw new ConvexError('Access denied');
  return user;
}
export const publicAccount = a => a ? ({ id: a._id, username: a.username, roles: a.roles,
  isAdmin: isAdmin(a), isStaff: isStaff(a) }) : null;
