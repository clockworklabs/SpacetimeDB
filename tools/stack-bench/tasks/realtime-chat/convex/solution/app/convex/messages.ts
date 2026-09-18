// Reference (oracle) Convex functions for the stack-bench realtime-chat task.
import { query, mutation } from "./_generated/server";
import { v } from "convex/values";

export const listMessages = query({
  args: {},
  handler: async (ctx) => {
    // Reactive by construction: clients subscribed via onUpdate re-receive this
    // on every write to `messages`. Ascending insertion order.
    return await ctx.db.query("messages").collect();
  },
});

export const sendMessage = mutation({
  args: { sender: v.string(), text: v.string() },
  handler: async (ctx, { sender, text }) => {
    if (text.length === 0) {
      throw new Error("text must not be empty");
    }
    await ctx.db.insert("messages", { sender, text, sentAt: Date.now() });
  },
});
