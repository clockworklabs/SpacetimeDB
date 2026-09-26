import { defineSchema, defineTable } from 'convex/server';
import { v } from 'convex/values';
import { authTables } from '@convex-dev/auth/server';
export default defineSchema({
  ...authTables,
  // Public Convex Auth schema customization; preserve its default fields/indexes.
  users: defineTable({
    name: v.optional(v.string()), image: v.optional(v.string()), email: v.optional(v.string()),
    emailVerificationTime: v.optional(v.number()), phone: v.optional(v.string()),
    phoneVerificationTime: v.optional(v.number()), isAnonymous: v.optional(v.boolean()),
    registrationNonce: v.optional(v.string()),
  }).index('email', ['email']).index('phone', ['phone']),
  items: defineTable({ name: v.string(), stock: v.number(), marker: v.string() }),
  orders: defineTable({ itemId: v.id('items'), buyer: v.string(), quantity: v.number() }),
  markers: defineTable({ value: v.string() }),
});
