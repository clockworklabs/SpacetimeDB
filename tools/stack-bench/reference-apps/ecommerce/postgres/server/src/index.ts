import "dotenv/config";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import express from "express";
import type { Request, Response, NextFunction } from "express";
import http from "node:http";
import cookieParser from "cookie-parser";
import { parse as parseCookie } from "cookie";
import { Server as SocketIOServer } from "socket.io";
import { sql, eq, and, asc, desc, inArray, ne } from "drizzle-orm";
import { db, pool } from "./db.js";
import { item, warehouse, stock, account, session, cart, cartItem, orders, orderItem, review } from "./schema.js";
import { hashPassword, verifyPassword, newToken } from "./auth.js";
import { seed } from "./seed.js";
import {
  attachProgressionSocket,
  checkoutReservedCart,
  emitProgression,
  processImmediateRestock,
  processProgressionTimers,
  registerProgression,
  removeReservedCartItem,
  reserveCartItem,
  setReservedCartQuantity,
  syncProgressionSocket,
} from "./progression.js";

const PORT = Number(process.env.PORT) || 6301;

const app = express();
app.use(express.json());
app.use((req, res, next) => {
  if (["GET", "HEAD", "OPTIONS"].includes(req.method) || !req.headers.origin) return next();
  return URL.parse(req.headers.origin)?.host === req.headers.host
    ? next() : res.status(403).json({ error: "cross-origin request refused" });
});
app.use(cookieParser());

type AccountInfo = { id: number; username: string; isAdmin: boolean; isStaff: boolean };

declare global {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace Express {
    interface Request {
      account?: AccountInfo | null;
    }
  }
}

async function loadAccountFromToken(token: string | undefined): Promise<AccountInfo | null> {
  if (!token) return null;
  const rows = await db
    .select({ id: account.id, username: account.username, isAdmin: account.isAdmin, isStaff: account.isStaff })
    .from(session)
    .innerJoin(account, eq(account.id, session.accountId))
    .where(eq(session.id, token))
    .limit(1);
  if (rows.length === 0) return null;
  return rows[0];
}

app.use(async (req: Request, _res: Response, next: NextFunction) => {
  const token = req.cookies?.sid as string | undefined;
  req.account = await loadAccountFromToken(token);
  next();
});

function requireAuth(req: Request, res: Response, next: NextFunction) {
  if (!req.account) return res.status(401).json({ error: "sign in required" });
  next();
}

function requireAdmin(req: Request, res: Response, next: NextFunction) {
  if (!req.account) return res.status(401).json({ error: "sign in required" });
  if (!req.account.isAdmin) return res.status(403).json({ error: "admin only" });
  next();
}

function requireStaff(req: Request, res: Response, next: NextFunction) {
  if (!req.account) return res.status(401).json({ error: "sign in required" });
  if (!req.account.isAdmin && !req.account.isStaff) return res.status(403).json({ error: "staff only" });
  next();
}

function asyncHandler(fn: (req: Request, res: Response) => Promise<void>) {
  return (req: Request, res: Response, next: NextFunction) => {
    fn(req, res).catch(next);
  };
}

// ---------- shared query builders ----------

async function buildCatalog() {
  const purchaseCounts = db.select({ itemId: orderItem.itemId,
    count: sql<number>`sum(${orderItem.quantity})::int`.as("count") }).from(orderItem)
    .innerJoin(orders, eq(orders.id, orderItem.orderId))
    .where(and(eq(orderItem.returned, false), ne(orders.status, "cancelled")))
    .groupBy(orderItem.itemId).as("purchase_counts");
  const rows = await db.select({ id: item.id, name: item.name, price: item.price,
    category: item.category, variants: item.variants,
    stock: sql<number>`coalesce(sum(${stock.quantity}), 0)::int`,
    purchaseCount: sql<number>`coalesce(${purchaseCounts.count}, 0)::int`,
  }).from(item).leftJoin(stock, eq(stock.itemId, item.id))
    .leftJoin(purchaseCounts, eq(purchaseCounts.itemId, item.id))
    .groupBy(item.id, purchaseCounts.count)
    .orderBy(desc(sql`coalesce(${purchaseCounts.count}, 0)`), asc(item.name));
  return rows.map((row) => ({
    ...row,
    price: Number(row.price),
  }));
}

async function buildAdminState() {
  const itemsResult = await db.select({ id: item.id, name: item.name, price: item.price,
    category: item.category, stock: sql<number>`coalesce(sum(${stock.quantity}), 0)::int` })
    .from(item).leftJoin(stock, eq(stock.itemId, item.id)).groupBy(item.id)
    .orderBy(asc(item.name));
  const warehousesResult = await db.select({ id: warehouse.id, name: warehouse.name,
    total: sql<number>`coalesce(sum(${stock.quantity}), 0)::int` }).from(warehouse)
    .leftJoin(stock, eq(stock.warehouseId, warehouse.id)).groupBy(warehouse.id)
    .orderBy(asc(warehouse.id));
  const locationsResult = await db.select({ itemId: stock.itemId, itemName: item.name,
    warehouseId: stock.warehouseId, warehouseName: warehouse.name, quantity: stock.quantity })
    .from(stock).innerJoin(item, eq(item.id, stock.itemId))
    .innerJoin(warehouse, eq(warehouse.id, stock.warehouseId))
    .orderBy(asc(item.name), asc(warehouse.name));
  const revenueResult = await db.execute<{ revenue: string }>(sql`
    SELECT
      COALESCE((SELECT SUM(total - refund_total) FROM orders WHERE status != 'cancelled'), 0)
      - COALESCE((
          SELECT SUM(oi.quantity * oi.price)
          FROM order_item oi JOIN orders o ON o.id = oi.order_id
          WHERE oi.returned = true AND o.status != 'cancelled'
        ), 0) AS revenue
  `);
  const categoryResult = await db.execute<{ category: string; units: number; revenue: string }>(sql`
    SELECT i.category,
           COALESCE(SUM(CASE WHEN oi.returned = false AND o.status != 'cancelled' THEN oi.quantity ELSE 0 END), 0)::int AS units,
           COALESCE(SUM(CASE WHEN oi.returned = false AND o.status != 'cancelled' THEN oi.quantity * oi.price ELSE 0 END), 0) AS revenue
    FROM item i
    LEFT JOIN order_item oi ON oi.item_id = i.id
    LEFT JOIN orders o ON o.id = oi.order_id
    GROUP BY i.category
    ORDER BY i.category ASC
  `);
  const queueDepthResult = await db.select({ count: sql<number>`count(*)::int` }).from(orders)
    .where(eq(orders.status, "pending"));

  const items = itemsResult.map((row) => ({
    ...row,
    price: Number(row.price),
  }));

  const lowStock = items
    .filter((it) => it.stock <= 10)
    .slice()
    .sort((a, b) => a.stock - b.stock)
    .map((it) => ({ id: it.id, name: it.name, stock: it.stock }));

  return {
    items,
    warehouses: warehousesResult,
    locations: locationsResult,
    revenue: Number(revenueResult.rows[0].revenue),
    lowStock,
    categoryTotals: categoryResult.rows.map((r) => ({
      category: r.category,
      units: r.units,
      revenue: Number(r.revenue),
    })),
    queueDepth: queueDepthResult[0].count,
  };
}

async function buildFulfilmentQueue() {
  const rows = await db.select({ id: orders.id, createdAt: orders.createdAt,
    name: orderItem.itemName, quantity: orderItem.quantity, warehouse: warehouse.name })
    .from(orders).innerJoin(orderItem, eq(orderItem.orderId, orders.id))
    .innerJoin(warehouse, eq(warehouse.id, orderItem.warehouseId))
    .where(eq(orders.status, "pending"))
    .orderBy(asc(orders.createdAt), asc(orders.id), asc(orderItem.id));
  const out: Array<{ id: number; createdAt: Date;
    items: Array<{ name: string; quantity: number; warehouse: string }> }> = [];
  for (const row of rows) {
    let order = out.at(-1);
    if (order?.id !== row.id) {
      order = { id: row.id, createdAt: row.createdAt, items: [] };
      out.push(order);
    }
    order.items.push({ name: row.name, quantity: row.quantity, warehouse: row.warehouse });
  }
  return out;
}

async function buildRecommended(accountId: number | null) {
  const catalog = await buildCatalog();
  if (accountId == null) {
    return catalog.slice(0, 10);
  }
  const categoriesResult = await db.selectDistinct({ category: item.category }).from(orderItem)
    .innerJoin(orders, eq(orders.id, orderItem.orderId))
    .innerJoin(item, eq(item.id, orderItem.itemId))
    .where(and(eq(orders.accountId, accountId), eq(orderItem.returned, false),
      ne(orders.status, "cancelled")));
  const categories = new Set(categoriesResult.map((row) => row.category));
  if (categories.size === 0) return [];
  const cartItemsResult = await db.select({ itemId: cartItem.itemId }).from(cart)
    .innerJoin(cartItem, eq(cartItem.cartId, cart.id)).where(eq(cart.accountId, accountId));
  const inCart = new Set(cartItemsResult.map((row) => row.itemId));
  return catalog.filter((i) => categories.has(i.category) && !inCart.has(i.id)).slice(0, 10);
}

async function buildItemReviews(itemId: number) {
  const rows = await db.select({ id: review.id, accountId: review.accountId,
    username: account.username, rating: review.rating, comment: review.comment,
    createdAt: review.createdAt }).from(review)
    .innerJoin(account, eq(account.id, review.accountId)).where(eq(review.itemId, itemId))
    .orderBy(desc(review.createdAt));
  const reviews = rows.map((row) => ({
    ...row,
  }));
  const average =
    reviews.length > 0 ? reviews.reduce((sum, r) => sum + r.rating, 0) / reviews.length : null;
  return { reviews, average };
}

async function buildCartState(accountId: number) {
  const [cartMeta, rows] = await Promise.all([
    db.select({ expiredAt: cart.expiredAt }).from(cart).where(eq(cart.accountId, accountId)).limit(1),
    db.select({ itemId: cartItem.itemId, name: item.name, price: item.price,
      quantity: cartItem.quantity, expired: cartItem.expired,
      reservationSeconds: sql<number>`extract(epoch from (${cartItem.reservedUntil} - now()))::int` })
      .from(cart).innerJoin(cartItem, eq(cartItem.cartId, cart.id))
      .innerJoin(item, eq(item.id, cartItem.itemId)).where(eq(cart.accountId, accountId))
      .orderBy(asc(cartItem.id)),
  ]);
  const items = rows.map((row) => ({
    itemId: row.itemId,
    name: row.name,
    price: Number(row.price),
    quantity: row.quantity,
    expired: row.expired,
    reservationSeconds: Math.max(0, row.reservationSeconds ?? 0),
    lineTotal: Number(row.price) * row.quantity,
  }));
  const total = items.reduce((sum, i) => sum + i.lineTotal, 0);
  return { items, total, expiredAt: cartMeta[0]?.expiredAt ?? null };
}

async function buildOrders(accountId: number) {
  const orderRows = await db.select().from(orders).where(eq(orders.accountId, accountId))
    .orderBy(desc(orders.createdAt));
  const lines = orderRows.length === 0 ? [] : await db.select().from(orderItem)
    .where(inArray(orderItem.orderId, orderRows.map((row) => row.id))).orderBy(asc(orderItem.id));
  return orderRows.map((order) => ({
      id: order.id,
      createdAt: order.createdAt,
      total: Number(order.total),
      status: order.status,
      discount: Number(order.discount),
      paymentStatus: order.paymentStatus,
      paymentAmount: Number(order.paymentAmount),
      refundTotal: Number(order.refundTotal),
      items: lines.filter((line) => line.orderId === order.id).map((line) => ({
        orderItemId: line.id,
        itemId: line.itemId,
        name: line.itemName,
        quantity: line.quantity,
        price: Number(line.price),
        returned: line.returned,
      })),
    }));
}

async function getOrCreateCart(accountId: number): Promise<number> {
  const result = await db.insert(cart).values({ accountId }).onConflictDoUpdate({
    target: cart.accountId, set: { accountId },
  }).returning({ id: cart.id });
  return result[0].id;
}

// ---------- broadcasting ----------

let io: SocketIOServer;
let lastCatalogJson = "";
let lastAdminJson = "";

async function broadcastCatalog() {
  const catalog = await buildCatalog();
  const json = JSON.stringify(catalog);
  if (json !== lastCatalogJson) {
    lastCatalogJson = json;
    io.emit("items:update", { items: catalog });
    io.to("visitors").emit("recommended:update", {
      items: catalog.slice(0, 10),
    });
  }
  const adminState = await buildAdminState();
  const adminJson = JSON.stringify(adminState);
  if (adminJson !== lastAdminJson) {
    lastAdminJson = adminJson;
    io.to("admin").emit("admin:update", adminState);
  }
}

async function broadcastCart(accountId: number) {
  const state = await buildCartState(accountId);
  io.to(`account:${accountId}`).emit("cart:update", state);
}

async function broadcastOrders(accountId: number) {
  const ordersList = await buildOrders(accountId);
  io.to(`account:${accountId}`).emit("orders:update", { orders: ordersList });
}

async function broadcastFulfilment() {
  const queue = await buildFulfilmentQueue();
  io.to("fulfilment").emit("queue:update", { queue, depth: queue.length });
}

async function broadcastRecommended(accountId: number) {
  const list = await buildRecommended(accountId);
  io.to(`account:${accountId}`).emit("recommended:update", { items: list });
  await emitProgression();
}

// ---------- auth routes ----------

app.post(
  "/api/auth/signup",
  asyncHandler(async (req, res) => {
    const { username, password } = req.body ?? {};
    if (typeof username !== "string" || typeof password !== "string" || !username.trim() || !password) {
      res.status(400).json({ error: "username and password are required" });
      return;
    }
    const existing = await db.select({ id: account.id }).from(account).where(eq(account.username, username)).limit(1);
    if (existing.length > 0) {
      res.status(409).json({ error: "username already taken" });
      return;
    }
    const passwordHash = hashPassword(password);
    const [created] = await db
      .insert(account)
      .values({ username, passwordHash, isAdmin: false, isStaff: false })
      .returning({ id: account.id, username: account.username, isAdmin: account.isAdmin, isStaff: account.isStaff });
    await getOrCreateCart(created.id);
    const token = newToken();
    await db.insert(session).values({ id: token, accountId: created.id });
    res.cookie("sid", token, { httpOnly: true, sameSite: "lax", path: "/" });
    res.json({ account: created });
  })
);

app.post(
  "/api/auth/signin",
  asyncHandler(async (req, res) => {
    const { username, password } = req.body ?? {};
    if (typeof username !== "string" || typeof password !== "string") {
      res.status(400).json({ error: "username and password are required" });
      return;
    }
    const rows = await db.select().from(account).where(eq(account.username, username)).limit(1);
    if (rows.length === 0 || !verifyPassword(password, rows[0].passwordHash)) {
      res.status(401).json({ error: "invalid username or password" });
      return;
    }
    const acc = rows[0];
    const token = newToken();
    await db.insert(session).values({ id: token, accountId: acc.id });
    res.cookie("sid", token, { httpOnly: true, sameSite: "lax", path: "/" });
    res.json({ account: { id: acc.id, username: acc.username, isAdmin: acc.isAdmin, isStaff: acc.isStaff } });
  })
);

app.post(
  "/api/auth/signout",
  asyncHandler(async (req, res) => {
    const token = req.cookies?.sid as string | undefined;
    if (token) {
      await db.delete(session).where(eq(session.id, token));
    }
    res.clearCookie("sid", { path: "/" });
    res.json({ ok: true });
  })
);

app.get(
  "/api/me",
  asyncHandler(async (req, res) => {
    res.json({ account: req.account ?? null });
  })
);

// ---------- catalog routes ----------

app.get(
  "/api/items",
  asyncHandler(async (_req, res) => {
    const catalog = await buildCatalog();
    res.json({ items: catalog });
  })
);

app.get(
  "/api/items/:id",
  asyncHandler(async (req, res) => {
    const itemId = Number(req.params.id);
    const rows = await db.select().from(item).where(eq(item.id, itemId)).limit(1);
    if (rows.length === 0) {
      res.status(404).json({ error: "item not found" });
      return;
    }
    const stockRows = await db.select({ value: sql<number>`coalesce(sum(${stock.quantity}), 0)::int` })
      .from(stock).where(eq(stock.itemId, itemId));
    const { reviews, average } = await buildItemReviews(itemId);
    res.json({
      item: {
        id: rows[0].id,
        name: rows[0].name,
        price: Number(rows[0].price),
        category: rows[0].category,
        stock: stockRows[0].value,
      },
      reviews,
      average,
    });
  })
);

app.post(
  "/api/items/:id/buy",
  requireAuth,
  asyncHandler(async (req, res) => {
    const itemId = Number(req.params.id);
    const accountId = req.account!.id;

    const result = await db.transaction(async (tx) => {
      const decrement = await tx.execute(sql<{ itemId: number; warehouseId: number }>`
        WITH target AS (
          SELECT item_id, warehouse_id FROM ${stock}
          WHERE item_id = ${itemId} AND quantity > 0
          ORDER BY warehouse_id FOR UPDATE LIMIT 1
        )
        UPDATE ${stock} SET quantity = quantity - 1
        FROM target
        WHERE ${stock.itemId} = target.item_id AND ${stock.warehouseId} = target.warehouse_id
        RETURNING ${stock.itemId} AS "itemId", ${stock.warehouseId} AS "warehouseId"
      `);
      if (decrement.rows.length === 0) return { status: 409, error: "item is out of stock" };
      const itemRows = await tx.select({ name: item.name, price: item.price }).from(item)
        .where(eq(item.id, itemId)).limit(1);
      if (itemRows.length === 0) return { status: 404, error: "item not found" };
      const created = await tx.insert(orders).values({ accountId, total: itemRows[0].price,
        paymentAmount: itemRows[0].price }).returning({ id: orders.id });
      await tx.insert(orderItem).values({ orderId: created[0].id, itemId,
        itemName: itemRows[0].name, quantity: 1, price: itemRows[0].price,
        warehouseId: Number(decrement.rows[0].warehouseId) });
      return null;
    });
    if (result) { res.status(result.status).json({ error: result.error }); return; }

    await broadcastCatalog();
    await broadcastOrders(accountId);
    await broadcastFulfilment();
    await broadcastRecommended(accountId);
    res.json({ ok: true });
  })
);

// ---------- cart routes ----------

app.get(
  "/api/cart",
  requireAuth,
  asyncHandler(async (req, res) => {
    const state = await buildCartState(req.account!.id);
    res.json(state);
  })
);

app.post(
  "/api/cart",
  requireAuth,
  asyncHandler(async (req, res) => {
    const { itemId, quantity } = req.body ?? {};
    const qty = Number(quantity) || 1;
    if (!Number.isInteger(itemId) || qty < 1) {
      res.status(400).json({ error: "invalid item or quantity" });
      return;
    }
    const accountId = req.account!.id;
    try {
      await reserveCartItem(accountId, itemId, qty);
    } catch (error) {
      res.status(409).json({ error: error instanceof Error ? error.message : "could not reserve stock" });
      return;
    }
    const state = await buildCartState(accountId);
    io.to(`account:${accountId}`).emit("cart:update", state);
    await broadcastCatalog();
    await broadcastRecommended(accountId);
    res.json(state);
  })
);

app.patch(
  "/api/cart/:itemId",
  requireAuth,
  asyncHandler(async (req, res) => {
    const itemId = Number(req.params.itemId);
    const { quantity } = req.body ?? {};
    if (!Number.isInteger(quantity) || quantity < 1) {
      res.status(400).json({ error: "quantity must be at least 1" });
      return;
    }
    const accountId = req.account!.id;
    try { await setReservedCartQuantity(accountId, itemId, quantity); }
    catch (error) { res.status(409).json({ error: error instanceof Error ? error.message : "could not reserve stock" }); return; }
    const state = await buildCartState(accountId);
    io.to(`account:${accountId}`).emit("cart:update", state);
    await broadcastCatalog();
    res.json(state);
  })
);

app.delete(
  "/api/cart/:itemId",
  requireAuth,
  asyncHandler(async (req, res) => {
    const itemId = Number(req.params.itemId);
    const accountId = req.account!.id;
    await removeReservedCartItem(accountId, itemId);
    const state = await buildCartState(accountId);
    io.to(`account:${accountId}`).emit("cart:update", state);
    await broadcastCatalog();
    await broadcastRecommended(accountId);
    res.json(state);
  })
);

app.post(
  "/api/checkout",
  requireAuth,
  asyncHandler(async (req, res) => {
    const accountId = req.account!.id;
    try {
      await checkoutReservedCart(accountId);
    } catch (error) {
      res.status(409).json({ error: error instanceof Error ? error.message : "checkout failed" });
      return;
    }

    await broadcastCatalog();
    await broadcastCart(accountId);
    await broadcastOrders(accountId);
    await broadcastFulfilment();
    await broadcastRecommended(accountId);
    res.json({ ok: true });
  })
);

app.get(
  "/api/orders",
  requireAuth,
  asyncHandler(async (req, res) => {
    const ordersList = await buildOrders(req.account!.id);
    res.json({ orders: ordersList });
  })
);

app.post(
  "/api/orders/:id/cancel",
  requireAuth,
  asyncHandler(async (req, res) => {
    const orderId = Number(req.params.id);
    const accountId = req.account!.id;
    const result = await db.transaction(async (tx) => {
      const orderRows = await tx.select({ accountId: orders.accountId, status: orders.status })
        .from(orders).where(eq(orders.id, orderId)).for("update");
      if (orderRows.length === 0 || orderRows[0].accountId !== accountId) {
        return { status: 404, error: "order not found" };
      }
      if (orderRows[0].status !== "pending") {
        return { status: 409, error: "order has already shipped" };
      }
      const lines = await tx.select({ itemId: orderItem.itemId, quantity: orderItem.quantity,
        warehouseId: orderItem.warehouseId }).from(orderItem).where(eq(orderItem.orderId, orderId));
      for (const line of lines) {
        await tx.insert(stock).values(line).onConflictDoUpdate({
          target: [stock.itemId, stock.warehouseId],
          set: { quantity: sql`${stock.quantity} + ${line.quantity}` },
        });
      }
      await tx.update(orders).set({ status: "cancelled" }).where(eq(orders.id, orderId));
      return null;
    });
    if (result) { res.status(result.status).json({ error: result.error }); return; }

    await broadcastCatalog();
    await broadcastOrders(accountId);
    await broadcastFulfilment();
    await broadcastRecommended(accountId);
    res.json({ ok: true });
  })
);

app.post(
  "/api/orders/:id/return",
  requireAuth,
  asyncHandler(async (req, res) => {
    const orderId = Number(req.params.id);
    const accountId = req.account!.id;
    const { orderItemId } = req.body ?? {};
    if (!Number.isInteger(orderItemId)) {
      res.status(400).json({ error: "invalid item" });
      return;
    }
    const result = await db.transaction(async (tx) => {
      const orderRows = await tx.select({ accountId: orders.accountId, status: orders.status })
        .from(orders).where(eq(orders.id, orderId)).for("update");
      if (orderRows.length === 0 || orderRows[0].accountId !== accountId) {
        return { status: 404, error: "order not found" };
      }
      if (!["shipped", "delivered"].includes(orderRows[0].status)) {
        return { status: 409, error: "order has not shipped" };
      }
      const lines = await tx.select({ itemId: orderItem.itemId, quantity: orderItem.quantity,
        warehouseId: orderItem.warehouseId, returned: orderItem.returned }).from(orderItem)
        .where(and(eq(orderItem.id, orderItemId), eq(orderItem.orderId, orderId))).for("update");
      if (lines.length === 0) return { status: 404, error: "item not found in order" };
      if (lines[0].returned) return { status: 409, error: "item already returned" };
      await tx.insert(stock).values({ itemId: lines[0].itemId,
        warehouseId: lines[0].warehouseId, quantity: lines[0].quantity }).onConflictDoUpdate({
        target: [stock.itemId, stock.warehouseId],
        set: { quantity: sql`${stock.quantity} + ${lines[0].quantity}` },
      });
      await tx.update(orderItem).set({ returned: true }).where(eq(orderItem.id, orderItemId));
      return null;
    });
    if (result) { res.status(result.status).json({ error: result.error }); return; }

    await broadcastCatalog();
    await broadcastOrders(accountId);
    await broadcastRecommended(accountId);
    res.json({ ok: true });
  })
);

// ---------- reviews ----------

app.post(
  "/api/items/:id/reviews",
  requireAuth,
  asyncHandler(async (req, res) => {
    const itemId = Number(req.params.id);
    const accountId = req.account!.id;
    const { rating, comment } = req.body ?? {};
    const ratingNum = Number(rating);
    if (!Number.isInteger(ratingNum) || ratingNum < 1 || ratingNum > 5) {
      res.status(400).json({ error: "rating must be between 1 and 5" });
      return;
    }
    const purchased = await db.select({ id: orderItem.id }).from(orderItem)
      .innerJoin(orders, eq(orders.id, orderItem.orderId))
      .where(and(eq(orders.accountId, accountId), eq(orderItem.itemId, itemId))).limit(1);
    if (purchased.length === 0) {
      res.status(403).json({ error: "you can only review items you have purchased" });
      return;
    }
    await db.insert(review).values({ itemId, accountId, rating: ratingNum,
      comment: String(comment ?? "") }).onConflictDoUpdate({
        target: [review.itemId, review.accountId],
        set: { rating: ratingNum, comment: String(comment ?? "") },
      });
    const { reviews, average } = await buildItemReviews(itemId);
    io.emit("review:update", { itemId, reviews, average });
    res.json({ reviews, average });
  })
);

// ---------- fulfilment (staff + admin) ----------

app.get(
  "/api/fulfilment/queue",
  requireStaff,
  asyncHandler(async (_req, res) => {
    const queue = await buildFulfilmentQueue();
    res.json({ queue, depth: queue.length });
  })
);

app.post(
  "/api/fulfilment/ship",
  requireStaff,
  asyncHandler(async (req, res) => {
    const { orderId } = req.body ?? {};
    if (!Number.isInteger(orderId)) {
      res.status(400).json({ error: "invalid order" });
      return;
    }
    const result = await db.update(orders).set({ status: "shipped", shippedAt: new Date() })
      .where(and(eq(orders.id, orderId), eq(orders.status, "pending")))
      .returning({ accountId: orders.accountId });
    if (result.length === 0) {
      res.status(409).json({ error: "order is not waiting to ship" });
      return;
    }
    const accountId = result[0].accountId;
    await broadcastFulfilment();
    await broadcastOrders(accountId);
    await broadcastCatalog();
    const queue = await buildFulfilmentQueue();
    res.json({ queue, depth: queue.length });
  })
);

// ---------- recommendations ----------

app.get(
  "/api/recommended",
  asyncHandler(async (req, res) => {
    const list = await buildRecommended(req.account ? req.account.id : null);
    res.json({ items: list });
  })
);

// ---------- admin ----------

app.get(
  "/api/admin/state",
  requireAdmin,
  asyncHandler(async (_req, res) => {
    const state = await buildAdminState();
    res.json(state);
  })
);

app.post(
  "/api/admin/restock",
  requireAdmin,
  asyncHandler(async (req, res) => {
    const { itemId, warehouseId, quantity } = req.body ?? {};
    const qty = Number(quantity);
    if (!Number.isInteger(itemId) || !Number.isInteger(warehouseId) || !Number.isInteger(qty) || qty < 1) {
      res.status(400).json({ error: "invalid restock request" });
      return;
    }
    await db.insert(stock).values({ itemId, warehouseId, quantity: qty }).onConflictDoUpdate({
      target: [stock.itemId, stock.warehouseId],
      set: { quantity: sql`${stock.quantity} + ${qty}` },
    });
    await processImmediateRestock(itemId);
    await broadcastCatalog();
    const state = await buildAdminState();
    res.json(state);
  })
);

app.post(
  "/api/admin/transfer",
  requireAdmin,
  asyncHandler(async (req, res) => {
    const { itemId, fromWarehouseId, toWarehouseId, quantity } = req.body ?? {};
    const qty = Number(quantity);
    if (
      !Number.isInteger(itemId) ||
      !Number.isInteger(fromWarehouseId) ||
      !Number.isInteger(toWarehouseId) ||
      !Number.isInteger(qty) ||
      qty < 1 ||
      fromWarehouseId === toWarehouseId
    ) {
      res.status(400).json({ error: "invalid transfer request" });
      return;
    }
    const result = await db.transaction(async (tx) => {
      // Lock both warehouse rows in a fixed order (regardless of transfer
      // direction) so two transfers of the same item between the same two
      // warehouses can never deadlock waiting on each other's locks.
      const orderedWarehouseIds = [fromWarehouseId, toWarehouseId].sort((a, b) => a - b);
      const lockedRows = await tx.select({ warehouseId: stock.warehouseId,
        quantity: stock.quantity }).from(stock)
        .where(and(eq(stock.itemId, itemId), inArray(stock.warehouseId, orderedWarehouseIds)))
        .orderBy(asc(stock.warehouseId)).for("update");
      const fromRow = lockedRows.find((row) => row.warehouseId === fromWarehouseId);
      const available = fromRow ? fromRow.quantity : 0;
      if (available < qty) {
        return { status: 409, error: "warehouse does not have enough stock to transfer" };
      }
      await tx.update(stock).set({ quantity: sql`${stock.quantity} - ${qty}` })
        .where(and(eq(stock.itemId, itemId), eq(stock.warehouseId, fromWarehouseId)));
      await tx.insert(stock).values({ itemId, warehouseId: toWarehouseId, quantity: qty })
        .onConflictDoUpdate({ target: [stock.itemId, stock.warehouseId],
          set: { quantity: sql`${stock.quantity} + ${qty}` } });
      return null;
    });
    if (result) { res.status(result.status).json({ error: result.error }); return; }

    await broadcastCatalog();
    const state = await buildAdminState();
    res.json(state);
  })
);

app.post(
  "/api/admin/price",
  requireAdmin,
  asyncHandler(async (req, res) => {
    const { itemId, price } = req.body ?? {};
    const priceNum = Number(price);
    if (!Number.isInteger(itemId) || !Number.isFinite(priceNum) || priceNum <= 0) {
      res.status(400).json({ error: "invalid price" });
      return;
    }
    await db.update(item).set({ price: priceNum.toFixed(2) }).where(eq(item.id, itemId));
    await broadcastCatalog();
    const cartAccounts = await db.selectDistinct({ accountId: cart.accountId }).from(cart)
      .innerJoin(cartItem, eq(cartItem.cartId, cart.id)).where(eq(cartItem.itemId, itemId));
    for (const row of cartAccounts) {
      await broadcastCart(row.accountId);
    }
    const state = await buildAdminState();
    res.json(state);
  })
);

registerProgression(app, {
  pool,
  requireAuth,
  requireAdmin,
  requireStaff,
  broadcastCatalog,
  broadcastCart,
  broadcastOrders,
  broadcastFulfilment,
  buildRecommended,
});

const clientDist = join(fileURLToPath(new URL(".", import.meta.url)), "../../client/dist");
app.use(express.static(clientDist));
app.get(/^\/(?!api(?:\/|$)).*/, (_req, res) => res.sendFile(join(clientDist, "index.html")));

// ---------- error handler ----------

app.use((err: unknown, _req: Request, res: Response, _next: NextFunction) => {
  console.error(err);
  res.status(500).json({ error: "internal server error" });
});

// ---------- server + sockets ----------

const httpServer = http.createServer(app);
io = new SocketIOServer(httpServer, {
  path: "/socket.io",
});
attachProgressionSocket(io);

io.on("connection", async (socket) => {
  const cookieHeader = socket.request.headers.cookie;
  const cookies = cookieHeader ? parseCookie(cookieHeader) : {};
  const token = cookies["sid"];
  const acc = await loadAccountFromToken(token);

  if (acc) {
    socket.join(`account:${acc.id}`);
    if (acc.isAdmin) socket.join("admin");
    if (acc.isAdmin || acc.isStaff) socket.join("fulfilment");
  } else {
    socket.join("visitors");
  }

  try {
    await syncProgressionSocket(socket, acc);
    const catalog = await buildCatalog();
    socket.emit("items:update", { items: catalog });
    if (acc) {
      const cartState = await buildCartState(acc.id);
      socket.emit("cart:update", cartState);
      const ordersList = await buildOrders(acc.id);
      socket.emit("orders:update", { orders: ordersList });
      if (acc.isAdmin) {
        const adminState = await buildAdminState();
        socket.emit("admin:update", adminState);
      }
      if (acc.isAdmin || acc.isStaff) {
        const queue = await buildFulfilmentQueue();
        socket.emit("queue:update", { queue, depth: queue.length });
      }
      const recommended = await buildRecommended(acc.id);
      socket.emit("recommended:update", { items: recommended });
    } else {
      const recommended = await buildRecommended(null);
      socket.emit("recommended:update", { items: recommended });
    }
  } catch (err) {
    console.error("socket initial sync failed", err);
  }
});

const POLL_INTERVAL_MS = 2000;
setInterval(() => {
  broadcastCatalog().catch((err) => console.error("poll broadcast failed", err));
  processProgressionTimers().catch((err) => console.error("progression timer failed", err));
}, POLL_INTERVAL_MS);

async function main() {
  await seed();
  httpServer.listen(PORT, () => {
    console.log(`PostgreSQL Shop server listening on port ${PORT}`);
  });
}

main().catch((err) => {
  console.error("fatal startup error", err);
  process.exit(1);
});
