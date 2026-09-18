import { defineSchema, defineTable } from 'convex/server';
import { v } from 'convex/values';
export default defineSchema({
  items: defineTable({ name: v.string(), stock: v.number(), marker: v.string() }),
  orders: defineTable({ itemId: v.id('items'), buyer: v.string(), quantity: v.number() }),
});
