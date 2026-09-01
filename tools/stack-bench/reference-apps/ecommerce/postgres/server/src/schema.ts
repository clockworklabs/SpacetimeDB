import {
  pgTable,
  serial,
  text,
  integer,
  numeric,
  boolean,
  timestamp,
  primaryKey,
  unique,
  jsonb,
} from "drizzle-orm/pg-core";

export const item = pgTable("item", {
  id: serial("id").primaryKey(),
  name: text("name").notNull(),
  price: numeric("price", { precision: 10, scale: 2 }).notNull(),
  category: text("category").notNull().default(""),
  variants: text("variants").array().notNull().default([]),
});

export const warehouse = pgTable("warehouse", {
  id: serial("id").primaryKey(),
  name: text("name").notNull(),
});

export const stock = pgTable(
  "stock",
  {
    itemId: integer("item_id").notNull().references(() => item.id),
    warehouseId: integer("warehouse_id").notNull().references(() => warehouse.id),
    quantity: integer("quantity").notNull().default(0),
  },
  (t) => ({
    pk: primaryKey({ columns: [t.itemId, t.warehouseId] }),
  })
);

export const account = pgTable("account", {
  id: serial("id").primaryKey(),
  username: text("username").notNull().unique(),
  passwordHash: text("password_hash").notNull(),
  isAdmin: boolean("is_admin").notNull().default(false),
  isStaff: boolean("is_staff").notNull().default(false),
  createdAt: timestamp("created_at").notNull().defaultNow(),
  profileName: text("profile_name").notNull().default(""),
  profileAddress: text("profile_address").notNull().default(""),
  staffRole: text("staff_role").notNull().default(""),
  notifyOrder: boolean("notify_order").notNull().default(false),
  notifyStock: boolean("notify_stock").notNull().default(false),
});

export const promotion = pgTable("promotion", {
  id: serial("id").primaryKey(),
  code: text("code").notNull().unique(),
  discountPercent: numeric("discount_percent", { precision: 5, scale: 2 }).notNull(),
  startAt: timestamp("start_at", { withTimezone: true }).notNull(),
  endAt: timestamp("end_at", { withTimezone: true }).notNull(),
  redemptionLimit: integer("redemption_limit").notNull(),
  redemptions: integer("redemptions").notNull().default(0),
  revenue: numeric("revenue", { precision: 12, scale: 2 }).notNull().default("0"),
  createdBy: integer("created_by").references(() => account.id),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
});

export const session = pgTable("session", {
  id: text("id").primaryKey(),
  accountId: integer("account_id").notNull().references(() => account.id, { onDelete: "cascade" }),
  createdAt: timestamp("created_at").notNull().defaultNow(),
});

export const cart = pgTable("cart", {
  id: serial("id").primaryKey(),
  accountId: integer("account_id").notNull().unique().references(() => account.id, { onDelete: "cascade" }),
  lastActivity: timestamp("last_activity", { withTimezone: true }).notNull().defaultNow(),
  expiredAt: timestamp("expired_at", { withTimezone: true }),
  promotionId: integer("promotion_id").references(() => promotion.id),
});

export const cartItem = pgTable(
  "cart_item",
  {
    id: serial("id").primaryKey(),
    cartId: integer("cart_id").notNull().references(() => cart.id, { onDelete: "cascade" }),
    itemId: integer("item_id").notNull().references(() => item.id),
    quantity: integer("quantity").notNull(),
    reservedUntil: timestamp("reserved_until", { withTimezone: true }),
    expired: boolean("expired").notNull().default(false),
  },
  (t) => ({
    cartItemUnique: unique().on(t.cartId, t.itemId),
  })
);

export const orders = pgTable("orders", {
  id: serial("id").primaryKey(),
  accountId: integer("account_id").notNull().references(() => account.id),
  createdAt: timestamp("created_at").notNull().defaultNow(),
  total: numeric("total", { precision: 10, scale: 2 }).notNull(),
  status: text("status").notNull().default("pending"), // pending | shipped | cancelled
  shippedAt: timestamp("shipped_at", { withTimezone: true }),
  deliveredAt: timestamp("delivered_at", { withTimezone: true }),
  promotionId: integer("promotion_id").references(() => promotion.id),
  discount: numeric("discount", { precision: 12, scale: 2 }).notNull().default("0"),
  paymentStatus: text("payment_status").notNull().default("paid"),
  paymentAmount: numeric("payment_amount", { precision: 12, scale: 2 }).notNull().default("0"),
  refundTotal: numeric("refund_total", { precision: 12, scale: 2 }).notNull().default("0"),
});

export const orderItem = pgTable("order_item", {
  id: serial("id").primaryKey(),
  orderId: integer("order_id").notNull().references(() => orders.id, { onDelete: "cascade" }),
  itemId: integer("item_id").notNull().references(() => item.id),
  itemName: text("item_name").notNull(),
  quantity: integer("quantity").notNull(),
  price: numeric("price", { precision: 10, scale: 2 }).notNull(),
  warehouseId: integer("warehouse_id").notNull().references(() => warehouse.id),
  returned: boolean("returned").notNull().default(false),
});

export const review = pgTable(
  "review",
  {
    id: serial("id").primaryKey(),
    itemId: integer("item_id").notNull().references(() => item.id),
    accountId: integer("account_id").notNull().references(() => account.id),
    rating: integer("rating").notNull(),
    comment: text("comment").notNull().default(""),
    createdAt: timestamp("created_at").notNull().defaultNow(),
  },
  (t) => ({
    reviewUnique: unique().on(t.itemId, t.accountId),
  })
);

export const cartReservationAllocation = pgTable("cart_reservation_allocation", {
  cartItemId: integer("cart_item_id").notNull().references(() => cartItem.id, { onDelete: "cascade" }),
  warehouseId: integer("warehouse_id").notNull().references(() => warehouse.id),
  quantity: integer("quantity").notNull(),
}, (t) => ({ pk: primaryKey({ columns: [t.cartItemId, t.warehouseId] }) }));

export const supportCase = pgTable("support_case", {
  id: serial("id").primaryKey(),
  accountId: integer("account_id").references(() => account.id),
  email: text("email").notNull(),
  subject: text("subject").notNull(),
  message: text("message").notNull(),
  reference: text("reference").notNull().unique(),
  assignee: text("assignee").notNull().default(""),
  priority: text("priority").notNull().default("normal"),
  status: text("status").notNull().default("new"),
  orderId: integer("order_id").references(() => orders.id),
  refundTotal: numeric("refund_total", { precision: 12, scale: 2 }).notNull().default("0"),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
  updatedAt: timestamp("updated_at", { withTimezone: true }).notNull().defaultNow(),
});

export const supportReply = pgTable("support_reply", {
  id: serial("id").primaryKey(),
  caseId: integer("case_id").notNull().references(() => supportCase.id, { onDelete: "cascade" }),
  accountId: integer("account_id").notNull().references(() => account.id),
  message: text("message").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
});

export const notification = pgTable("notification", {
  id: serial("id").primaryKey(),
  accountId: integer("account_id").notNull().references(() => account.id, { onDelete: "cascade" }),
  kind: text("kind").notNull(),
  subjectKey: text("subject_key").notNull(),
  message: text("message").notNull(),
  read: boolean("read").notNull().default(false),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
}, (t) => ({ accountKindSubject: unique().on(t.accountId, t.kind, t.subjectKey) }));

export const stockAlertRequest = pgTable("stock_alert_request", {
  accountId: integer("account_id").notNull().references(() => account.id, { onDelete: "cascade" }),
  itemId: integer("item_id").notNull().references(() => item.id, { onDelete: "cascade" }),
  fulfilled: boolean("fulfilled").notNull().default(false),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
}, (t) => ({ pk: primaryKey({ columns: [t.accountId, t.itemId] }) }));

export const scheduledRestock = pgTable("scheduled_restock", {
  id: serial("id").primaryKey(),
  itemId: integer("item_id").notNull().references(() => item.id),
  warehouseId: integer("warehouse_id").notNull().references(() => warehouse.id),
  quantity: integer("quantity").notNull(),
  dueAt: timestamp("due_at", { withTimezone: true }).notNull(),
  cancelled: boolean("cancelled").notNull().default(false),
  applied: boolean("applied").notNull().default(false),
  automatic: boolean("automatic").notNull().default(false),
  createdBy: integer("created_by").references(() => account.id),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
});

export const stockLedger = pgTable("stock_ledger", {
  id: serial("id").primaryKey(),
  itemId: integer("item_id").notNull().references(() => item.id),
  warehouseId: integer("warehouse_id").notNull().references(() => warehouse.id),
  quantity: integer("quantity").notNull(),
  source: text("source").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
});

export const staffActivity = pgTable("staff_activity", {
  id: serial("id").primaryKey(),
  accountId: integer("account_id").notNull().references(() => account.id),
  action: text("action").notNull(),
  subject: text("subject").notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
});

export const reorderRule = pgTable("reorder_rule", {
  id: serial("id").primaryKey(),
  itemId: integer("item_id").notNull().unique().references(() => item.id),
  threshold: integer("threshold").notNull(),
  quantity: integer("quantity").notNull(),
  enabled: boolean("enabled").notNull().default(true),
  createdBy: integer("created_by").notNull().references(() => account.id),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
});

export const expiredCart = pgTable("expired_cart", {
  id: serial("id").primaryKey(),
  accountId: integer("account_id").notNull().references(() => account.id, { onDelete: "cascade" }),
  items: jsonb("items").notNull(),
  restored: boolean("restored").notNull().default(false),
  expiredAt: timestamp("expired_at", { withTimezone: true }).notNull().defaultNow(),
});

export const recommendationDismissal = pgTable("recommendation_dismissal", {
  accountId: integer("account_id").notNull().references(() => account.id, { onDelete: "cascade" }),
  itemId: integer("item_id").notNull().references(() => item.id, { onDelete: "cascade" }),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
}, (t) => ({ pk: primaryKey({ columns: [t.accountId, t.itemId] }) }));

export const refundEntry = pgTable("refund_entry", {
  id: serial("id").primaryKey(),
  orderId: integer("order_id").notNull().unique().references(() => orders.id),
  supportCaseId: integer("support_case_id").notNull().references(() => supportCase.id),
  amount: numeric("amount", { precision: 12, scale: 2 }).notNull(),
  createdAt: timestamp("created_at", { withTimezone: true }).notNull().defaultNow(),
});
