import { mutation } from "./_generated/server";
import { v } from "convex/values";
import { accountBalances, accountKey } from './balances';

export const transfer = mutation({
  args: {
    amount: v.number(),
    from_id: v.number(),
    to_id: v.number(),
  },
  handler: async (ctx, { amount, from_id, to_id }) => {
    if (from_id === to_id || amount <= 0) return;

    const from = await ctx.db
      .query("accounts")
      .withIndex("by_account_id", q => q.eq("id", from_id))
      .first();

    const to = await ctx.db
      .query("accounts")
      .withIndex("by_account_id", q => q.eq("id", to_id))
      .first();

    if (!from || !to) return;

    const fromBalance = from.balance ?? 0;
    const toBalance   = to.balance ?? 0;

    // prevent negative balances
    if (fromBalance < amount) {
      return;
    }

    await ctx.db.patch(from._id, { balance: fromBalance - amount });
    await ctx.db.patch(to._id,   { balance: toBalance + amount });
  },
});


export const transfer_sharded = mutation({
  args: {
    amount: v.number(),
    from_id: v.number(),
    to_id: v.number(),
  },
  handler: async (ctx, { amount, from_id, to_id }) => {
    if (from_id === to_id || amount <= 0) return;

    const from = await ctx.db
      .query("accounts")
      .withIndex("by_account_id", q => q.eq("id", from_id))
      .first();

    const to = await ctx.db
      .query("accounts")
      .withIndex("by_account_id", q => q.eq("id", to_id))
      .first();

    if (!from || !to) return;

    const fromCounter = accountBalances.for(accountKey(from_id));
    const toCounter   = accountBalances.for(accountKey(to_id));

    const fromBalance = await fromCounter.count(ctx);
    if (fromBalance < amount) return;

    await Promise.all([
      fromCounter.subtract(ctx, amount),
      toCounter.add(ctx, amount),
    ]);
  },
});