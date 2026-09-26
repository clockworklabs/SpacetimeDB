// Reference (oracle) user/presence functions.
import { mutation, query } from "./_generated/server";
import { v } from "convex/values";
import { getUserDoc, STATUSES } from "./lib";

export const register = mutation({
  args: { username: v.string() },
  handler: async (ctx, { username }) => {
    if (username.length === 0) throw new Error("username must not be empty");
    const existing = await ctx.db
      .query("users")
      .withIndex("by_username", (q) => q.eq("username", username))
      .unique();
    // Idempotent: existing users are left completely unchanged.
    if (!existing) {
      await ctx.db.insert("users", { username, status: "online", balance: 100 });
    }
  },
});

export const setStatus = mutation({
  args: { username: v.string(), status: v.string() },
  handler: async (ctx, { username, status }) => {
    if (!STATUSES.includes(status)) throw new Error(`invalid status: ${status}`);
    const user = await getUserDoc(ctx, username);
    await ctx.db.patch(user._id, { status });
  },
});

export const get = query({
  args: { username: v.string() },
  handler: async (ctx, { username }) => {
    const doc = await ctx.db
      .query("users")
      .withIndex("by_username", (q) => q.eq("username", username))
      .unique();
    return doc ? { username: doc.username, status: doc.status, balance: doc.balance } : null;
  },
});

export const list = query({
  args: {},
  handler: async (ctx) => {
    const docs = await ctx.db.query("users").collect();
    return docs.map((d) => ({ username: d.username, status: d.status, balance: d.balance }));
  },
});
