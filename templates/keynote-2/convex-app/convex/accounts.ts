import { query } from "./_generated/server";
import { v } from 'convex/values';

export const get_account = query(async ({ db }, { id }: { id: number }) => {
  const row = await db
    .query("accounts")
    .withIndex("by_account_id", (q) => q.eq("id", id))
    .unique();

  if (!row) return null;
  return { id: row.id, balance: row.balance };
});

export const get_stats = query({
  args: {
    initialBalance: v.number(),
  },
  handler: async (ctx, { initialBalance }) => {
    const rows = await ctx.db.query("accounts").collect();

    let count = 0;
    let total = 0;
    let changed = 0;

    for (const row of rows) {
      count++;
      total += row.balance;
      if (row.balance !== initialBalance) changed++;
    }

    return { count, total, changed };
  },
});