import "dotenv/config";
import express from "express";
import type { Request, Response, NextFunction } from "express";
import http from "node:http";
import cookieParser from "cookie-parser";
import { parse as parseCookie } from "cookie";
import { Server as SocketIOServer } from "socket.io";
import { sql, eq, and } from "drizzle-orm";
import { db, pool } from "./db.js";
import { item, warehouse, stock, account, session, cart, cartItem, orders, orderItem, review } from "./schema.js";
import { hashPassword, verifyPassword, newToken } from "./auth.js";
import { seed } from "./seed.js";

const PORT = Number(process.env.PORT) || 6301;

const app = express();
app.use(express.json());
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
  const result = await pool.query(`
    SELECT i.id, i.name, i.price, i.category, COALESCE(SUM(s.quantity), 0)::int AS stock,
           COALESCE(p.cnt, 0)::int AS purchase_count
    FROM item i
    LEFT JOIN stock s ON s.item_id = i.id
    LEFT JOIN (
      SELECT oi.item_id, SUM(oi.quantity) AS cnt
      FROM order_item oi
      JOIN orders o ON o.id = oi.order_id
      WHERE oi.returned = false AND o.status != 'cancelled'
      GROUP BY oi.item_id
    ) p ON p.item_id = i.id
    GROUP BY i.id, p.cnt
    ORDER BY purchase_count DESC, i.name ASC
  `);
  return result.rows.map((r) => ({
    id: r.id,
    name: r.name,
    price: Number(r.price),
    category: r.category,
    stock: r.stock,
    purchaseCount: r.purchase_count,
  }));
}

async function buildAdminState() {
  const itemsResult = await pool.query(`
    SELECT i.id, i.name, i.price, i.category, COALESCE(SUM(s.quantity), 0)::int AS stock
    FROM item i
    LEFT JOIN stock s ON s.item_id = i.id
    GROUP BY i.id
    ORDER BY i.name ASC
  `);
  const warehousesResult = await pool.query(`
    SELECT w.id, w.name, COALESCE(SUM(s.quantity), 0)::int AS total
    FROM warehouse w
    LEFT JOIN stock s ON s.warehouse_id = w.id
    GROUP BY w.id
    ORDER BY w.id ASC
  `);
  const locationsResult = await pool.query(`
    SELECT s.item_id, i.name AS item_name, s.warehouse_id, w.name AS warehouse_name, s.quantity
    FROM stock s
    JOIN item i ON i.id = s.item_id
    JOIN warehouse w ON w.id = s.warehouse_id
    ORDER BY i.name ASC, w.name ASC
  `);
  const revenueResult = await pool.query(`
    SELECT
      COALESCE((SELECT SUM(total) FROM orders WHERE status != 'cancelled'), 0)
      - COALESCE((
          SELECT SUM(oi.quantity * oi.price)
          FROM order_item oi JOIN orders o ON o.id = oi.order_id
          WHERE oi.returned = true AND o.status != 'cancelled'
        ), 0) AS revenue
  `);
  const categoryResult = await pool.query(`
    SELECT i.category,
           COALESCE(SUM(CASE WHEN oi.returned = false AND o.status != 'cancelled' THEN oi.quantity ELSE 0 END), 0)::int AS units,
           COALESCE(SUM(CASE WHEN oi.returned = false AND o.status != 'cancelled' THEN oi.quantity * oi.price ELSE 0 END), 0) AS revenue
    FROM item i
    LEFT JOIN order_item oi ON oi.item_id = i.id
    LEFT JOIN orders o ON o.id = oi.order_id
    GROUP BY i.category
    ORDER BY i.category ASC
  `);
  const queueDepthResult = await pool.query(`SELECT COUNT(*)::int AS n FROM orders WHERE status = 'pending'`);

  const items = itemsResult.rows.map((r) => ({
    id: r.id,
    name: r.name,
    price: Number(r.price),
    category: r.category,
    stock: r.stock,
  }));

  const lowStock = items
    .filter((it) => it.stock <= 10)
    .slice()
    .sort((a, b) => a.stock - b.stock)
    .map((it) => ({ id: it.id, name: it.name, stock: it.stock }));

  return {
    items,
    warehouses: warehousesResult.rows.map((r) => ({ id: r.id, name: r.name, total: r.total })),
    locations: locationsResult.rows.map((r) => ({
      itemId: r.item_id,
      itemName: r.item_name,
      warehouseId: r.warehouse_id,
      warehouseName: r.warehouse_name,
      quantity: r.quantity,
    })),
    revenue: Number(revenueResult.rows[0].revenue),
    lowStock,
    categoryTotals: categoryResult.rows.map((r) => ({
      category: r.category,
      units: r.units,
      revenue: Number(r.revenue),
    })),
    queueDepth: queueDepthResult.rows[0].n,
  };
}

async function buildFulfilmentQueue() {
  const ordersResult = await pool.query(
    `SELECT id, created_at FROM orders WHERE status = 'pending' ORDER BY created_at ASC, id ASC`
  );
  const out = [];
  for (const o of ordersResult.rows) {
    const lines = await pool.query(
      `SELECT oi.item_name, oi.quantity, w.name AS warehouse_name
       FROM order_item oi JOIN warehouse w ON w.id = oi.warehouse_id
       WHERE oi.order_id = $1
       ORDER BY oi.id ASC`,
      [o.id]
    );
    out.push({
      id: o.id,
      createdAt: o.created_at,
      items: lines.rows.map((l) => ({ name: l.item_name, quantity: l.quantity, warehouse: l.warehouse_name })),
    });
  }
  return out;
}

async function buildRecommended(accountId: number | null) {
  const catalog = await buildCatalog();
  if (accountId == null) {
    return catalog.filter((i) => i.purchaseCount > 0).slice(0, 10);
  }
  const categoriesResult = await pool.query(
    `SELECT DISTINCT i.category
     FROM order_item oi JOIN orders o ON o.id = oi.order_id JOIN item i ON i.id = oi.item_id
     WHERE o.account_id = $1 AND oi.returned = false AND o.status != 'cancelled'`,
    [accountId]
  );
  const categories = new Set(categoriesResult.rows.map((r) => r.category));
  if (categories.size === 0) return [];
  const cartItemsResult = await pool.query(
    `SELECT ci.item_id FROM cart c JOIN cart_item ci ON ci.cart_id = c.id WHERE c.account_id = $1`,
    [accountId]
  );
  const inCart = new Set(cartItemsResult.rows.map((r) => r.item_id));
  return catalog.filter((i) => categories.has(i.category) && !inCart.has(i.id)).slice(0, 10);
}

async function buildItemReviews(itemId: number) {
  const rows = await pool.query(
    `SELECT r.id, r.account_id, a.username, r.rating, r.comment, r.created_at
     FROM review r JOIN account a ON a.id = r.account_id
     WHERE r.item_id = $1
     ORDER BY r.created_at DESC`,
    [itemId]
  );
  const reviews = rows.rows.map((r) => ({
    id: r.id,
    accountId: r.account_id,
    username: r.username,
    rating: r.rating,
    comment: r.comment,
    createdAt: r.created_at,
  }));
  const average =
    reviews.length > 0 ? reviews.reduce((sum, r) => sum + r.rating, 0) / reviews.length : null;
  return { reviews, average };
}

async function buildCartState(accountId: number) {
  const rows = await pool.query(
    `SELECT ci.item_id, i.name, i.price, ci.quantity
     FROM cart c
     JOIN cart_item ci ON ci.cart_id = c.id
     JOIN item i ON i.id = ci.item_id
     WHERE c.account_id = $1
     ORDER BY ci.id ASC`,
    [accountId]
  );
  const items = rows.rows.map((r) => ({
    itemId: r.item_id,
    name: r.name,
    price: Number(r.price),
    quantity: r.quantity,
    lineTotal: Number(r.price) * r.quantity,
  }));
  const total = items.reduce((sum, i) => sum + i.lineTotal, 0);
  return { items, total };
}

async function buildOrders(accountId: number) {
  const ordersResult = await pool.query(
    `SELECT id, created_at, total, status FROM orders WHERE account_id = $1 ORDER BY created_at DESC`,
    [accountId]
  );
  const out = [];
  for (const o of ordersResult.rows) {
    const linesResult = await pool.query(
      `SELECT id, item_id, item_name, quantity, price, returned FROM order_item WHERE order_id = $1 ORDER BY id ASC`,
      [o.id]
    );
    out.push({
      id: o.id,
      createdAt: o.created_at,
      total: Number(o.total),
      status: o.status,
      items: linesResult.rows.map((l) => ({
        orderItemId: l.id,
        itemId: l.item_id,
        name: l.item_name,
        quantity: l.quantity,
        price: Number(l.price),
        returned: l.returned,
      })),
    });
  }
  return out;
}

async function getOrCreateCart(accountId: number): Promise<number> {
  const result = await pool.query(
    `INSERT INTO cart (account_id) VALUES ($1)
     ON CONFLICT (account_id) DO UPDATE SET account_id = excluded.account_id
     RETURNING id`,
    [accountId]
  );
  return result.rows[0].id;
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
    const stockRows = await pool.query(`SELECT COALESCE(SUM(quantity),0)::int AS stock FROM stock WHERE item_id = $1`, [itemId]);
    const { reviews, average } = await buildItemReviews(itemId);
    res.json({
      item: {
        id: rows[0].id,
        name: rows[0].name,
        price: Number(rows[0].price),
        category: rows[0].category,
        stock: stockRows.rows[0].stock,
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

    const client = await pool.connect();
    try {
      await client.query("BEGIN");
      const decrement = await client.query(
        `WITH target AS (
           SELECT item_id, warehouse_id FROM stock
           WHERE item_id = $1 AND quantity > 0
           ORDER BY warehouse_id
           FOR UPDATE
           LIMIT 1
         )
         UPDATE stock s SET quantity = quantity - 1
         FROM target t
         WHERE s.item_id = t.item_id AND s.warehouse_id = t.warehouse_id
         RETURNING s.item_id, s.warehouse_id`,
        [itemId]
      );
      if (decrement.rowCount === 0) {
        await client.query("ROLLBACK");
        res.status(409).json({ error: "item is out of stock" });
        return;
      }
      const warehouseId = decrement.rows[0].warehouse_id;
      const itemRow = await client.query(`SELECT name, price FROM item WHERE id = $1`, [itemId]);
      if (itemRow.rowCount === 0) {
        await client.query("ROLLBACK");
        res.status(404).json({ error: "item not found" });
        return;
      }
      const { name, price } = itemRow.rows[0];
      const orderResult = await client.query(
        `INSERT INTO orders (account_id, total, status) VALUES ($1, $2, 'pending') RETURNING id`,
        [accountId, price]
      );
      const orderId = orderResult.rows[0].id;
      await client.query(
        `INSERT INTO order_item (order_id, item_id, item_name, quantity, price, warehouse_id) VALUES ($1, $2, $3, 1, $4, $5)`,
        [orderId, itemId, name, price, warehouseId]
      );
      await client.query("COMMIT");
    } catch (err) {
      await client.query("ROLLBACK");
      throw err;
    } finally {
      client.release();
    }

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
    const client = await pool.connect();
    let state;
    try {
      await client.query("BEGIN");
      const cartRow = await client.query(
        `INSERT INTO cart (account_id) VALUES ($1)
         ON CONFLICT (account_id) DO UPDATE SET account_id = excluded.account_id
         RETURNING id`,
        [accountId]
      );
      const cartId = cartRow.rows[0].id;
      await client.query(`SELECT id FROM cart WHERE id = $1 FOR UPDATE`, [cartId]);
      await client.query(
        `INSERT INTO cart_item (cart_id, item_id, quantity) VALUES ($1, $2, $3)
         ON CONFLICT (cart_id, item_id) DO UPDATE SET quantity = cart_item.quantity + excluded.quantity`,
        [cartId, itemId, qty]
      );
      const rows = await client.query(
        `SELECT ci.item_id, i.name, i.price, ci.quantity
         FROM cart_item ci JOIN item i ON i.id = ci.item_id
         WHERE ci.cart_id = $1 ORDER BY ci.id ASC`,
        [cartId]
      );
      const items = rows.rows.map((r) => ({
        itemId: r.item_id,
        name: r.name,
        price: Number(r.price),
        quantity: r.quantity,
        lineTotal: Number(r.price) * r.quantity,
      }));
      state = { items, total: items.reduce((sum, i) => sum + i.lineTotal, 0) };
      await client.query("COMMIT");
    } catch (err) {
      await client.query("ROLLBACK");
      throw err;
    } finally {
      client.release();
    }
    io.to(`account:${accountId}`).emit("cart:update", state);
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
    const cartId = await getOrCreateCart(accountId);
    const result = await pool.query(
      `UPDATE cart_item SET quantity = $1 WHERE cart_id = $2 AND item_id = $3`,
      [quantity, cartId, itemId]
    );
    if (result.rowCount === 0) {
      res.status(404).json({ error: "item not in cart" });
      return;
    }
    const state = await buildCartState(accountId);
    io.to(`account:${accountId}`).emit("cart:update", state);
    res.json(state);
  })
);

app.delete(
  "/api/cart/:itemId",
  requireAuth,
  asyncHandler(async (req, res) => {
    const itemId = Number(req.params.itemId);
    const accountId = req.account!.id;
    const cartId = await getOrCreateCart(accountId);
    await pool.query(`DELETE FROM cart_item WHERE cart_id = $1 AND item_id = $2`, [cartId, itemId]);
    const state = await buildCartState(accountId);
    io.to(`account:${accountId}`).emit("cart:update", state);
    await broadcastRecommended(accountId);
    res.json(state);
  })
);

app.post(
  "/api/checkout",
  requireAuth,
  asyncHandler(async (req, res) => {
    const accountId = req.account!.id;
    const client = await pool.connect();
    try {
      await client.query("BEGIN");
      const cartRow = await client.query(`SELECT id FROM cart WHERE account_id = $1 FOR UPDATE`, [accountId]);
      if (cartRow.rowCount === 0) {
        await client.query("ROLLBACK");
        res.status(400).json({ error: "cart is empty" });
        return;
      }
      const cartId = cartRow.rows[0].id;
      const lines = await client.query(
        `SELECT ci.item_id, ci.quantity, i.name, i.price
         FROM cart_item ci JOIN item i ON i.id = ci.item_id
         WHERE ci.cart_id = $1 ORDER BY ci.item_id ASC`,
        [cartId]
      );
      if (lines.rowCount === 0) {
        await client.query("ROLLBACK");
        res.status(400).json({ error: "cart is empty" });
        return;
      }
      const itemIds = lines.rows.map((l) => l.item_id);
      const stockRows = await client.query(
        `SELECT item_id, warehouse_id, quantity FROM stock WHERE item_id = ANY($1::int[]) ORDER BY item_id ASC, warehouse_id ASC FOR UPDATE`,
        [itemIds]
      );
      const available: Record<number, number> = {};
      const byItem: Record<number, Array<{ warehouseId: number; quantity: number }>> = {};
      for (const r of stockRows.rows) {
        available[r.item_id] = (available[r.item_id] ?? 0) + r.quantity;
        (byItem[r.item_id] ??= []).push({ warehouseId: r.warehouse_id, quantity: r.quantity });
      }
      for (const line of lines.rows) {
        if ((available[line.item_id] ?? 0) < line.quantity) {
          await client.query("ROLLBACK");
          res.status(409).json({ error: `not enough stock for ${line.name}` });
          return;
        }
      }
      const orderItemsToInsert: Array<{ itemId: number; name: string; quantity: number; price: string; warehouseId: number }> = [];
      for (const line of lines.rows) {
        let remaining = line.quantity;
        for (const loc of byItem[line.item_id]) {
          if (remaining <= 0) break;
          const take = Math.min(loc.quantity, remaining);
          if (take > 0) {
            await client.query(
              `UPDATE stock SET quantity = quantity - $1 WHERE item_id = $2 AND warehouse_id = $3`,
              [take, line.item_id, loc.warehouseId]
            );
            loc.quantity -= take;
            remaining -= take;
            orderItemsToInsert.push({
              itemId: line.item_id,
              name: line.name,
              quantity: take,
              price: line.price,
              warehouseId: loc.warehouseId,
            });
          }
        }
      }
      const total = lines.rows.reduce((sum, l) => sum + Number(l.price) * l.quantity, 0);
      const orderResult = await client.query(
        `INSERT INTO orders (account_id, total, status) VALUES ($1, $2, 'pending') RETURNING id`,
        [accountId, total.toFixed(2)]
      );
      const orderId = orderResult.rows[0].id;
      for (const oi of orderItemsToInsert) {
        await client.query(
          `INSERT INTO order_item (order_id, item_id, item_name, quantity, price, warehouse_id) VALUES ($1, $2, $3, $4, $5, $6)`,
          [orderId, oi.itemId, oi.name, oi.quantity, oi.price, oi.warehouseId]
        );
      }
      await client.query(`DELETE FROM cart_item WHERE cart_id = $1`, [cartId]);
      await client.query("COMMIT");
    } catch (err) {
      await client.query("ROLLBACK");
      throw err;
    } finally {
      client.release();
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
    const client = await pool.connect();
    try {
      await client.query("BEGIN");
      const orderRow = await client.query(`SELECT id, account_id, status FROM orders WHERE id = $1 FOR UPDATE`, [orderId]);
      if (orderRow.rowCount === 0 || orderRow.rows[0].account_id !== accountId) {
        await client.query("ROLLBACK");
        res.status(404).json({ error: "order not found" });
        return;
      }
      if (orderRow.rows[0].status !== "pending") {
        await client.query("ROLLBACK");
        res.status(409).json({ error: "order has already shipped" });
        return;
      }
      const lines = await client.query(`SELECT item_id, quantity, warehouse_id FROM order_item WHERE order_id = $1`, [orderId]);
      for (const l of lines.rows) {
        await client.query(
          `INSERT INTO stock (item_id, warehouse_id, quantity) VALUES ($1, $2, $3)
           ON CONFLICT (item_id, warehouse_id) DO UPDATE SET quantity = stock.quantity + excluded.quantity`,
          [l.item_id, l.warehouse_id, l.quantity]
        );
      }
      await client.query(`UPDATE orders SET status = 'cancelled' WHERE id = $1`, [orderId]);
      await client.query("COMMIT");
    } catch (err) {
      await client.query("ROLLBACK");
      throw err;
    } finally {
      client.release();
    }

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
    const client = await pool.connect();
    try {
      await client.query("BEGIN");
      const orderRow = await client.query(`SELECT id, account_id, status FROM orders WHERE id = $1 FOR UPDATE`, [orderId]);
      if (orderRow.rowCount === 0 || orderRow.rows[0].account_id !== accountId) {
        await client.query("ROLLBACK");
        res.status(404).json({ error: "order not found" });
        return;
      }
      if (orderRow.rows[0].status !== "shipped") {
        await client.query("ROLLBACK");
        res.status(409).json({ error: "order has not shipped" });
        return;
      }
      const lineRow = await client.query(
        `SELECT id, item_id, quantity, warehouse_id, returned FROM order_item WHERE id = $1 AND order_id = $2 FOR UPDATE`,
        [orderItemId, orderId]
      );
      if (lineRow.rowCount === 0) {
        await client.query("ROLLBACK");
        res.status(404).json({ error: "item not found in order" });
        return;
      }
      if (lineRow.rows[0].returned) {
        await client.query("ROLLBACK");
        res.status(409).json({ error: "item already returned" });
        return;
      }
      const { item_id, quantity, warehouse_id } = lineRow.rows[0];
      await client.query(
        `INSERT INTO stock (item_id, warehouse_id, quantity) VALUES ($1, $2, $3)
         ON CONFLICT (item_id, warehouse_id) DO UPDATE SET quantity = stock.quantity + excluded.quantity`,
        [item_id, warehouse_id, quantity]
      );
      await client.query(`UPDATE order_item SET returned = true WHERE id = $1`, [orderItemId]);
      await client.query("COMMIT");
    } catch (err) {
      await client.query("ROLLBACK");
      throw err;
    } finally {
      client.release();
    }

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
    const purchased = await pool.query(
      `SELECT 1 FROM order_item oi JOIN orders o ON o.id = oi.order_id
       WHERE o.account_id = $1 AND oi.item_id = $2 LIMIT 1`,
      [accountId, itemId]
    );
    if (purchased.rowCount === 0) {
      res.status(403).json({ error: "you can only review items you have purchased" });
      return;
    }
    await pool.query(
      `INSERT INTO review (item_id, account_id, rating, comment) VALUES ($1, $2, $3, $4)
       ON CONFLICT (item_id, account_id) DO UPDATE SET rating = excluded.rating, comment = excluded.comment`,
      [itemId, accountId, ratingNum, String(comment ?? "")]
    );
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
    const result = await pool.query(
      `UPDATE orders SET status = 'shipped' WHERE id = $1 AND status = 'pending' RETURNING account_id`,
      [orderId]
    );
    if (result.rowCount === 0) {
      res.status(409).json({ error: "order is not waiting to ship" });
      return;
    }
    const accountId = result.rows[0].account_id;
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
    await pool.query(
      `INSERT INTO stock (item_id, warehouse_id, quantity) VALUES ($1, $2, $3)
       ON CONFLICT (item_id, warehouse_id) DO UPDATE SET quantity = stock.quantity + excluded.quantity`,
      [itemId, warehouseId, qty]
    );
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
    const client = await pool.connect();
    try {
      await client.query("BEGIN");
      // Lock both warehouse rows in a fixed order (regardless of transfer
      // direction) so two transfers of the same item between the same two
      // warehouses can never deadlock waiting on each other's locks.
      const orderedWarehouseIds = [fromWarehouseId, toWarehouseId].sort((a, b) => a - b);
      const lockedRows = await client.query(
        `SELECT warehouse_id, quantity FROM stock WHERE item_id = $1 AND warehouse_id = ANY($2::int[]) ORDER BY warehouse_id ASC FOR UPDATE`,
        [itemId, orderedWarehouseIds]
      );
      const fromRow = lockedRows.rows.find((r) => r.warehouse_id === fromWarehouseId);
      const available = fromRow ? fromRow.quantity : 0;
      if (available < qty) {
        await client.query("ROLLBACK");
        res.status(409).json({ error: "warehouse does not have enough stock to transfer" });
        return;
      }
      await client.query(`UPDATE stock SET quantity = quantity - $1 WHERE item_id = $2 AND warehouse_id = $3`, [
        qty,
        itemId,
        fromWarehouseId,
      ]);
      await client.query(
        `INSERT INTO stock (item_id, warehouse_id, quantity) VALUES ($1, $2, $3)
         ON CONFLICT (item_id, warehouse_id) DO UPDATE SET quantity = stock.quantity + excluded.quantity`,
        [itemId, toWarehouseId, qty]
      );
      await client.query("COMMIT");
    } catch (err) {
      await client.query("ROLLBACK");
      throw err;
    } finally {
      client.release();
    }

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
    await pool.query(`UPDATE item SET price = $1 WHERE id = $2`, [priceNum.toFixed(2), itemId]);
    await broadcastCatalog();
    const cartAccounts = await pool.query(
      `SELECT DISTINCT c.account_id FROM cart c JOIN cart_item ci ON ci.cart_id = c.id WHERE ci.item_id = $1`,
      [itemId]
    );
    for (const row of cartAccounts.rows) {
      await broadcastCart(row.account_id);
    }
    const state = await buildAdminState();
    res.json(state);
  })
);

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

io.on("connection", async (socket) => {
  const cookieHeader = socket.request.headers.cookie;
  const cookies = cookieHeader ? parseCookie(cookieHeader) : {};
  const token = cookies["sid"];
  const acc = await loadAccountFromToken(token);

  if (acc) {
    socket.join(`account:${acc.id}`);
    if (acc.isAdmin) socket.join("admin");
    if (acc.isAdmin || acc.isStaff) socket.join("fulfilment");
  }

  try {
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
