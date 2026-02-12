import { v } from "convex/values";
import { mutation } from "./_generated/server";

export const clear_accounts = mutation({
  args: {},
  handler: async ({ db }) => {
    const BATCH = 4_000; //limit is 4k
    const docs = await db.query("accounts").take(BATCH);
    for (const doc of docs) {
      await db.delete(doc._id);
    }
    return docs.length;
  },
});

export const seed_range = mutation({
  args: {
    start: v.number(),
    count: v.number(),
    initial: v.number(),
  },
  handler: async ({ db }, { start, count, initial }) => {
    const end = start + count;
    for (let i = start; i < end; i++) {
      await db.insert("accounts", { id: i, balance: initial });
    }
  },
});
