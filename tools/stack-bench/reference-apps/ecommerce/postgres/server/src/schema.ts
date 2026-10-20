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
} from "drizzle-orm/pg-core";

export const item = pgTable("item", {
  id: serial("id").primaryKey(),
  name: text("name").notNull(),
  price: numeric("price", { precision: 10, scale: 2 }).notNull(),
  category: text("category").notNull().default(""),
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
});

export const session = pgTable("session", {
  id: text("id").primaryKey(),
  accountId: integer("account_id").notNull().references(() => account.id, { onDelete: "cascade" }),
  createdAt: timestamp("created_at").notNull().defaultNow(),
});

export const cart = pgTable("cart", {
  id: serial("id").primaryKey(),
  accountId: integer("account_id").notNull().unique().references(() => account.id, { onDelete: "cascade" }),
});

export const cartItem = pgTable(
  "cart_item",
  {
    id: serial("id").primaryKey(),
    cartId: integer("cart_id").notNull().references(() => cart.id, { onDelete: "cascade" }),
    itemId: integer("item_id").notNull().references(() => item.id),
    quantity: integer("quantity").notNull(),
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
