// Reference (oracle) credit-transfer function. A single Convex mutation is a
// serializable transaction, so the two balance writes are atomic and total
// credits are conserved under concurrency.
import { mutation } from "./_generated/server";
import { v } from "convex/values";
import { getUserDoc } from "./lib";

export const tip = mutation({
  args: { fromUser: v.string(), toUser: v.string(), amount: v.number() },
  handler: async (ctx, { fromUser, toUser, amount }) => {
    if (fromUser === toUser) throw new Error("cannot tip yourself");
    if (!Number.isInteger(amount) || amount <= 0) throw new Error("amount must be positive");
    const from = await getUserDoc(ctx, fromUser);
    const to = await getUserDoc(ctx, toUser);
    if (from.balance < amount) throw new Error("insufficient balance");
    await ctx.db.patch(from._id, { balance: from.balance - amount });
    await ctx.db.patch(to._id, { balance: to.balance + amount });
  },
});
