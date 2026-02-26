import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export default defineSchema({
  accounts: defineTable({
    id: v.number(),
    balance: v.number(),
  })
    .index("by_account_id", ["id"]),
});
