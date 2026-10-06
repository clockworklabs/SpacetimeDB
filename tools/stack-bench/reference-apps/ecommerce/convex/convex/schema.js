import { defineSchema, defineTable } from 'convex/server';
import { v } from 'convex/values';
import { authTables } from '@convex-dev/auth/server';

export default defineSchema({
  ...authTables,
  users: defineTable({ name: v.optional(v.string()), image: v.optional(v.string()),
    email: v.optional(v.string()), emailVerificationTime: v.optional(v.number()),
    phone: v.optional(v.string()), phoneVerificationTime: v.optional(v.number()),
    isAnonymous: v.optional(v.boolean()), registrationNonce: v.optional(v.string()),
  }).index('email', ['email']).index('phone', ['phone']),
  order_account: defineTable({ auth_user_id: v.id('users'), username: v.string(),
    roles: v.array(v.string()), profile: v.object({ name: v.string(), address: v.string() }),
    preference: v.object({ order: v.boolean(), stock: v.boolean() }),
  }).index('auth_user', ['auth_user_id']).index('username', ['username']),
  item: defineTable({ name: v.string(), price: v.number(), description: v.string(),
    category: v.string(), variants: v.array(v.string()) }).index('name', ['name']),
  warehouse: defineTable({ name: v.string() }).index('name', ['name']),
  stock: defineTable({ item_id: v.id('item'), warehouse_id: v.id('warehouse'), quantity: v.number() })
    .index('item', ['item_id']).index('location', ['item_id', 'warehouse_id']),
  order_header: defineTable({ account_id: v.id('order_account'), total: v.number(), refunded: v.number(), status: v.string() })
    .index('account', ['account_id']),
  order_line: defineTable({ order_id: v.id('order_header'), item_id: v.id('item'), quantity: v.number(),
    unit_price: v.number(), returned: v.boolean() }).index('order', ['order_id']),
  order_cart: defineTable({ account_id: v.id('order_account'), item_id: v.id('item'), quantity: v.number() })
    .index('account', ['account_id']).index('line', ['account_id', 'item_id']),
  // This app does not reserve warehouse stock before checkout.
  order_reservation: defineTable({ account_id: v.id('order_account'), item_id: v.id('item'),
    warehouse_id: v.id('warehouse'), quantity: v.number() }),
  order_allocation: defineTable({ order_line_id: v.id('order_line'), warehouse_id: v.id('warehouse'), quantity: v.number() })
    .index('line', ['order_line_id']),
  review: defineTable({ itemId: v.id('item'), accountId: v.id('order_account'), rating: v.number(), comment: v.string() })
    .index('item', ['itemId']).index('author', ['itemId', 'accountId']),
  support: defineTable({ accountId: v.union(v.id('order_account'), v.null()), email: v.string(),
    subject: v.string(), message: v.string(), reference: v.string(), status: v.string(),
    priority: v.string(), assignee: v.string(), replies: v.array(v.object({ username: v.string(), body: v.string(), createdAt: v.number() })) }),
  promotion: defineTable({ code: v.string(), discountPercent: v.number(), startMicros: v.number(), endMicros: v.number(), usageLimit: v.number() }),
  stock_alert: defineTable({ accountId: v.id('order_account'), itemId: v.id('item'), delivered: v.boolean() })
    .index('item', ['itemId']).index('owner', ['accountId', 'itemId']),
  notification: defineTable({ accountId: v.id('order_account'), type: v.string(), message: v.string() }).index('account', ['accountId']),
  scheduled_restock: defineTable({ itemId: v.id('item'), warehouseId: v.id('warehouse'), quantity: v.number(),
    dueAt: v.number(), status: v.string(), scheduledId: v.optional(v.id('_scheduled_functions')) }),
  stock_ledger: defineTable({ itemId: v.id('item'), quantity: v.number() }),
});
