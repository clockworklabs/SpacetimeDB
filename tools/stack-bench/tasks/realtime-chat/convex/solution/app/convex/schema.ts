// Reference (oracle) Convex schema for the stack-bench realtime-chat task.
import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  messages: defineTable({
    sender: v.string(),
    text: v.string(),
    sentAt: v.number(),
  }),
});
