import type { Request, RequestHandler, Response } from "express";
import type { Express } from "express";
import type { Pool, PoolClient } from "pg";
import type { Server as SocketIOServer, Socket } from "socket.io";

export type ProgressionAccount = {
  id: number;
  username: string;
  isAdmin: boolean;
  isStaff: boolean;
};

type AppRequest = Request & { account?: ProgressionAccount | null };

type Dependencies = {
  pool: Pool;
  requireAuth: RequestHandler;
  requireAdmin: RequestHandler;
  requireStaff: RequestHandler;
  broadcastCatalog: () => Promise<void>;
  broadcastCart: (accountId: number) => Promise<void>;
  broadcastOrders: (accountId: number) => Promise<void>;
  broadcastFulfilment: () => Promise<void>;
};

let deps: Dependencies;
let io: SocketIOServer | null = null;

const asyncRoute = (fn: (req: AppRequest, res: Response) => Promise<void>): RequestHandler =>
  (req, res, next) => void fn(req as AppRequest, res).catch(next);

export async function initializeProgressionSchema(pool: Pool) {
  await pool.query(`
    CREATE TABLE IF NOT EXISTS promotion (
      id serial PRIMARY KEY,
      code text NOT NULL UNIQUE,
      discount_percent numeric(5,2) NOT NULL,
      start_at timestamptz NOT NULL,
      end_at timestamptz NOT NULL,
      redemption_limit integer NOT NULL,
      redemptions integer NOT NULL DEFAULT 0,
      revenue numeric(12,2) NOT NULL DEFAULT 0,
      created_by integer REFERENCES account(id),
      created_at timestamptz NOT NULL DEFAULT now()
    );
    ALTER TABLE account ADD COLUMN IF NOT EXISTS profile_name text NOT NULL DEFAULT '';
    ALTER TABLE account ADD COLUMN IF NOT EXISTS profile_address text NOT NULL DEFAULT '';
    ALTER TABLE account ADD COLUMN IF NOT EXISTS staff_role text NOT NULL DEFAULT '';
    ALTER TABLE account ADD COLUMN IF NOT EXISTS notify_order boolean NOT NULL DEFAULT false;
    ALTER TABLE account ADD COLUMN IF NOT EXISTS notify_stock boolean NOT NULL DEFAULT false;
    ALTER TABLE item ADD COLUMN IF NOT EXISTS variants text[] NOT NULL DEFAULT '{}';
    ALTER TABLE cart ADD COLUMN IF NOT EXISTS last_activity timestamptz NOT NULL DEFAULT now();
    ALTER TABLE cart ADD COLUMN IF NOT EXISTS expired_at timestamptz;
    ALTER TABLE cart ADD COLUMN IF NOT EXISTS promotion_id integer REFERENCES promotion(id);
    ALTER TABLE cart_item ADD COLUMN IF NOT EXISTS reserved_until timestamptz;
    ALTER TABLE cart_item ADD COLUMN IF NOT EXISTS expired boolean NOT NULL DEFAULT false;
    CREATE TABLE IF NOT EXISTS cart_reservation_allocation (
      cart_item_id integer NOT NULL REFERENCES cart_item(id) ON DELETE CASCADE,
      warehouse_id integer NOT NULL REFERENCES warehouse(id),
      quantity integer NOT NULL,
      PRIMARY KEY (cart_item_id, warehouse_id)
    );
    ALTER TABLE orders ADD COLUMN IF NOT EXISTS shipped_at timestamptz;
    ALTER TABLE orders ADD COLUMN IF NOT EXISTS delivered_at timestamptz;
    ALTER TABLE orders ADD COLUMN IF NOT EXISTS promotion_id integer REFERENCES promotion(id);
    ALTER TABLE orders ADD COLUMN IF NOT EXISTS discount numeric(12,2) NOT NULL DEFAULT 0;
    ALTER TABLE orders ADD COLUMN IF NOT EXISTS payment_status text NOT NULL DEFAULT 'paid';
    ALTER TABLE orders ADD COLUMN IF NOT EXISTS payment_amount numeric(12,2) NOT NULL DEFAULT 0;
    ALTER TABLE orders ADD COLUMN IF NOT EXISTS refund_total numeric(12,2) NOT NULL DEFAULT 0;

    CREATE TABLE IF NOT EXISTS support_case (
      id serial PRIMARY KEY,
      account_id integer REFERENCES account(id),
      email text NOT NULL,
      subject text NOT NULL,
      message text NOT NULL,
      reference text NOT NULL UNIQUE,
      assignee text NOT NULL DEFAULT '',
      priority text NOT NULL DEFAULT 'normal',
      status text NOT NULL DEFAULT 'new',
      order_id integer REFERENCES orders(id),
      refund_total numeric(12,2) NOT NULL DEFAULT 0,
      created_at timestamptz NOT NULL DEFAULT now(),
      updated_at timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE IF NOT EXISTS support_reply (
      id serial PRIMARY KEY,
      case_id integer NOT NULL REFERENCES support_case(id) ON DELETE CASCADE,
      account_id integer NOT NULL REFERENCES account(id),
      message text NOT NULL,
      created_at timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE IF NOT EXISTS notification (
      id serial PRIMARY KEY,
      account_id integer NOT NULL REFERENCES account(id) ON DELETE CASCADE,
      kind text NOT NULL,
      subject_key text NOT NULL,
      message text NOT NULL,
      read boolean NOT NULL DEFAULT false,
      created_at timestamptz NOT NULL DEFAULT now(),
      UNIQUE (account_id, kind, subject_key)
    );
    CREATE TABLE IF NOT EXISTS stock_alert_request (
      account_id integer NOT NULL REFERENCES account(id) ON DELETE CASCADE,
      item_id integer NOT NULL REFERENCES item(id) ON DELETE CASCADE,
      fulfilled boolean NOT NULL DEFAULT false,
      created_at timestamptz NOT NULL DEFAULT now(),
      PRIMARY KEY (account_id, item_id)
    );
    CREATE TABLE IF NOT EXISTS scheduled_restock (
      id serial PRIMARY KEY,
      item_id integer NOT NULL REFERENCES item(id),
      warehouse_id integer NOT NULL REFERENCES warehouse(id),
      quantity integer NOT NULL,
      due_at timestamptz NOT NULL,
      cancelled boolean NOT NULL DEFAULT false,
      applied boolean NOT NULL DEFAULT false,
      automatic boolean NOT NULL DEFAULT false,
      created_by integer REFERENCES account(id),
      created_at timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE IF NOT EXISTS stock_ledger (
      id serial PRIMARY KEY,
      item_id integer NOT NULL REFERENCES item(id),
      warehouse_id integer NOT NULL REFERENCES warehouse(id),
      quantity integer NOT NULL,
      source text NOT NULL,
      created_at timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE IF NOT EXISTS staff_activity (
      id serial PRIMARY KEY,
      account_id integer NOT NULL REFERENCES account(id),
      action text NOT NULL,
      subject text NOT NULL,
      created_at timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE IF NOT EXISTS reorder_rule (
      id serial PRIMARY KEY,
      item_id integer NOT NULL UNIQUE REFERENCES item(id),
      threshold integer NOT NULL,
      quantity integer NOT NULL,
      enabled boolean NOT NULL DEFAULT true,
      created_by integer NOT NULL REFERENCES account(id),
      created_at timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE IF NOT EXISTS expired_cart (
      id serial PRIMARY KEY,
      account_id integer NOT NULL REFERENCES account(id) ON DELETE CASCADE,
      items jsonb NOT NULL,
      restored boolean NOT NULL DEFAULT false,
      expired_at timestamptz NOT NULL DEFAULT now()
    );
    CREATE TABLE IF NOT EXISTS recommendation_dismissal (
      account_id integer NOT NULL REFERENCES account(id) ON DELETE CASCADE,
      item_id integer NOT NULL REFERENCES item(id) ON DELETE CASCADE,
      created_at timestamptz NOT NULL DEFAULT now(),
      PRIMARY KEY (account_id, item_id)
    );
    CREATE TABLE IF NOT EXISTS refund_entry (
      id serial PRIMARY KEY,
      order_id integer NOT NULL REFERENCES orders(id),
      support_case_id integer NOT NULL REFERENCES support_case(id),
      amount numeric(12,2) NOT NULL,
      created_at timestamptz NOT NULL DEFAULT now(),
      UNIQUE (order_id)
    );
  `);
}

async function one<T = any>(query: string, values: unknown[] = []): Promise<T | null> {
  const result = await deps.pool.query(query, values);
  return (result.rows[0] as T | undefined) ?? null;
}

async function recordActivity(client: PoolClient | Pool, accountId: number, action: string, subject: string) {
  await client.query(
    `INSERT INTO staff_activity (account_id, action, subject) VALUES ($1, $2, $3)`,
    [accountId, action, subject],
  );
}

async function visibleSupportCases(account: ProgressionAccount | null) {
  if (!account) return [];
  const where = account.isAdmin || account.isStaff ? "TRUE" : "sc.account_id = $1";
  const values = account.isAdmin || account.isStaff ? [] : [account.id];
  const cases = await deps.pool.query(
    `SELECT sc.*, o.total AS order_total
     FROM support_case sc LEFT JOIN orders o ON o.id = sc.order_id
     WHERE ${where} ORDER BY sc.created_at DESC`,
    values,
  );
  const replies = cases.rows.length === 0 ? { rows: [] } : await deps.pool.query(
    `SELECT sr.*, a.username FROM support_reply sr JOIN account a ON a.id = sr.account_id
     WHERE sr.case_id = ANY($1::int[]) ORDER BY sr.created_at ASC`,
    [cases.rows.map((entry) => entry.id)],
  );
  return cases.rows.map((entry) => ({
    id: entry.id,
    accountId: entry.account_id,
    email: entry.email,
    subject: entry.subject,
    message: entry.message,
    reference: entry.reference,
    assignee: entry.assignee,
    priority: entry.priority,
    status: entry.status,
    orderId: entry.order_id,
    refundTotal: Number(entry.refund_total),
    replies: replies.rows.filter((reply) => reply.case_id === entry.id).map((reply) => ({
      id: reply.id,
      username: reply.username,
      message: reply.message,
      createdAt: reply.created_at,
    })),
  }));
}

async function personalizedRecommendations(accountId: number) {
  const result = await deps.pool.query(
    `WITH purchased AS (
       SELECT DISTINCT oi.item_id, i.category
       FROM order_item oi JOIN orders o ON o.id = oi.order_id JOIN item i ON i.id = oi.item_id
       WHERE o.account_id = $1 AND o.status != 'cancelled' AND oi.returned = false
     ), sales AS (
       SELECT oi.item_id, COALESCE(SUM(oi.quantity), 0)::int AS units
       FROM order_item oi JOIN orders o ON o.id = oi.order_id
       WHERE o.status != 'cancelled' AND oi.returned = false GROUP BY oi.item_id
     )
     SELECT i.id, i.name, COALESCE(s.units, 0)::int AS units
     FROM item i
     LEFT JOIN sales s ON s.item_id = i.id
     WHERE i.category IN (SELECT category FROM purchased)
       AND i.id NOT IN (SELECT item_id FROM purchased)
       AND i.id NOT IN (SELECT item_id FROM recommendation_dismissal WHERE account_id = $1)
     ORDER BY units DESC, i.name ASC`,
    [accountId],
  );
  return result.rows.map((row, index) => ({ id: row.id, name: row.name, rank: index + 1 }));
}

export async function buildProgressionState(account: ProgressionAccount | null) {
  const isStaff = Boolean(account && (account.isAdmin || account.isStaff));
  const [profile, roles, promotions, preferences, notifications, completed, restocks, ledger,
    activity, reorders, expired, support, recommendations] = await Promise.all([
    account ? one(`SELECT profile_name, profile_address FROM account WHERE id = $1`, [account.id]) : null,
    isStaff ? deps.pool.query(`SELECT id, username, staff_role FROM account WHERE is_staff = true ORDER BY username`) : { rows: [] },
    isStaff ? deps.pool.query(`SELECT * FROM promotion ORDER BY created_at DESC`) : { rows: [] },
    account ? one(`SELECT notify_order, notify_stock FROM account WHERE id = $1`, [account.id]) : null,
    account ? deps.pool.query(`SELECT * FROM notification WHERE account_id = $1 ORDER BY created_at DESC`, [account.id]) : { rows: [] },
    isStaff ? deps.pool.query(
      `SELECT o.id, o.status, o.delivered_at, string_agg(oi.item_name, ', ' ORDER BY oi.id) AS items
       FROM orders o JOIN order_item oi ON oi.order_id = o.id
       WHERE o.status IN ('shipped', 'delivered') GROUP BY o.id ORDER BY o.id DESC`,
    ) : { rows: [] },
    isStaff ? deps.pool.query(
      `SELECT sr.*, i.name AS item_name, w.name AS warehouse_name
       FROM scheduled_restock sr JOIN item i ON i.id = sr.item_id JOIN warehouse w ON w.id = sr.warehouse_id
       WHERE sr.cancelled = false AND sr.applied = false ORDER BY sr.due_at`,
    ) : { rows: [] },
    isStaff ? deps.pool.query(
      `SELECT sl.*, i.name AS item_name, w.name AS warehouse_name
       FROM stock_ledger sl JOIN item i ON i.id = sl.item_id JOIN warehouse w ON w.id = sl.warehouse_id
       ORDER BY sl.created_at DESC LIMIT 30`,
    ) : { rows: [] },
    isStaff ? deps.pool.query(
      `SELECT sa.*, a.username FROM staff_activity sa JOIN account a ON a.id = sa.account_id
       ORDER BY sa.created_at DESC LIMIT 50`,
    ) : { rows: [] },
    isStaff ? deps.pool.query(
      `SELECT rr.*, i.name AS item_name,
        EXISTS (SELECT 1 FROM scheduled_restock sr WHERE sr.item_id = rr.item_id AND sr.automatic = true
          AND sr.cancelled = false AND sr.applied = false) AS pending
       FROM reorder_rule rr JOIN item i ON i.id = rr.item_id ORDER BY i.name`,
    ) : { rows: [] },
    account ? deps.pool.query(`SELECT * FROM expired_cart WHERE account_id = $1 AND restored = false ORDER BY expired_at DESC`, [account.id]) : { rows: [] },
    visibleSupportCases(account),
    account && !isStaff ? personalizedRecommendations(account.id) : [],
  ]);

  return {
    profile: profile ? { name: profile.profile_name, address: profile.profile_address } : null,
    roles: roles.rows.map((row) => ({ id: row.id, username: row.username, role: row.staff_role })),
    promotions: promotions.rows.map((row) => ({
      id: row.id, code: row.code, discount: Number(row.discount_percent),
      start: String(row.start_at).slice(0, 10), end: String(row.end_at).slice(0, 10),
      limit: row.redemption_limit, redemptions: row.redemptions, revenue: Number(row.revenue),
    })),
    preferences: preferences ? { order: preferences.notify_order, stock: preferences.notify_stock } : null,
    notifications: notifications.rows.map((row) => ({
      id: row.id, kind: row.kind, message: row.message, read: row.read,
    })),
    completedOrders: completed.rows.map((row) => ({
      id: row.id, status: row.status, items: row.items, deliveredAt: row.delivered_at,
    })),
    pendingRestocks: restocks.rows.map((row) => ({
      id: row.id, item: row.item_name, warehouse: row.warehouse_name, quantity: row.quantity,
      dueAt: row.due_at, automatic: row.automatic,
    })),
    stockLedger: ledger.rows.map((row) => ({
      id: row.id, item: row.item_name, warehouse: row.warehouse_name,
      quantity: row.quantity, source: row.source,
    })),
    activity: activity.rows.map((row) => ({
      id: row.id, actor: row.username, action: row.action, subject: row.subject,
      time: row.created_at,
    })),
    reorders: reorders.rows.map((row) => ({
      id: row.id, item: row.item_name, threshold: row.threshold,
      quantity: row.quantity, pending: row.pending,
    })),
    expiredCarts: expired.rows.map((row) => ({ id: row.id, items: row.items, expiredAt: row.expired_at })),
    support,
    recommendations,
  };
}

async function emitProgression() {
  if (!io) return;
  for (const socket of io.sockets.sockets.values()) {
    const account = (socket.data.account as ProgressionAccount | null | undefined) ?? null;
    socket.emit("progression:update", await buildProgressionState(account));
  }
}

export async function syncProgressionSocket(socket: Socket, account: ProgressionAccount | null) {
  socket.data.account = account;
  socket.emit("progression:update", await buildProgressionState(account));
}

async function findItemAndWarehouse(itemValue: unknown, warehouseValue: unknown) {
  const itemResult = await deps.pool.query(
    `SELECT id, name FROM item WHERE id = $1 OR lower(name) = lower($2) LIMIT 1`,
    [Number(itemValue) || -1, String(itemValue ?? "")],
  );
  const warehouseResult = await deps.pool.query(
    `SELECT id, name FROM warehouse WHERE id = $1 OR lower(name) = lower($2) LIMIT 1`,
    [Number(warehouseValue) || -1, String(warehouseValue ?? "")],
  );
  return { item: itemResult.rows[0], warehouse: warehouseResult.rows[0] };
}

async function processStockAlerts(client: PoolClient | Pool, itemId: number) {
  const current = await client.query(`SELECT COALESCE(SUM(quantity), 0)::int AS total FROM stock WHERE item_id = $1`, [itemId]);
  if (current.rows[0].total <= 0) return;
  const requests = await client.query(
    `UPDATE stock_alert_request SET fulfilled = true
     WHERE item_id = $1 AND fulfilled = false RETURNING account_id`,
    [itemId],
  );
  const itemRow = await client.query(`SELECT name FROM item WHERE id = $1`, [itemId]);
  for (const request of requests.rows) {
    await client.query(
      `INSERT INTO notification (account_id, kind, subject_key, message)
       VALUES ($1, 'stock', $2, $3) ON CONFLICT DO NOTHING`,
      [request.account_id, String(itemId), `${itemRow.rows[0]?.name ?? "Item"} is back in stock`],
    );
  }
}

async function releaseReservation(client: PoolClient, cartItemId: number) {
  const allocations = await client.query(
    `DELETE FROM cart_reservation_allocation WHERE cart_item_id = $1
     RETURNING warehouse_id, quantity`,
    [cartItemId],
  );
  const item = await client.query(`SELECT item_id FROM cart_item WHERE id = $1`, [cartItemId]);
  for (const allocation of allocations.rows) {
    await client.query(
      `UPDATE stock SET quantity = quantity + $1 WHERE item_id = $2 AND warehouse_id = $3`,
      [allocation.quantity, item.rows[0].item_id, allocation.warehouse_id],
    );
  }
}

async function allocateReservation(client: PoolClient, cartItemId: number, itemId: number, quantity: number) {
  const stockRows = await client.query(
    `SELECT warehouse_id, quantity FROM stock WHERE item_id = $1 ORDER BY warehouse_id FOR UPDATE`,
    [itemId],
  );
  const available = stockRows.rows.reduce((sum, row) => sum + row.quantity, 0);
  if (available < quantity) throw new Error("not enough stock");
  let remaining = quantity;
  for (const row of stockRows.rows) {
    const take = Math.min(remaining, row.quantity);
    if (take <= 0) continue;
    await client.query(
      `UPDATE stock SET quantity = quantity - $1 WHERE item_id = $2 AND warehouse_id = $3`,
      [take, itemId, row.warehouse_id],
    );
    await client.query(
      `INSERT INTO cart_reservation_allocation (cart_item_id, warehouse_id, quantity)
       VALUES ($1, $2, $3)
       ON CONFLICT (cart_item_id, warehouse_id)
       DO UPDATE SET quantity = cart_reservation_allocation.quantity + excluded.quantity`,
      [cartItemId, row.warehouse_id, take],
    );
    remaining -= take;
  }
}

export function registerProgression(app: Express, dependencies: Dependencies) {
  deps = dependencies;

  app.get("/api/progression", asyncRoute(async (req, res) => {
    res.json(await buildProgressionState(req.account ?? null));
  }));

  app.put("/api/profile", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const name = String(req.body?.name ?? "").trim();
    const address = String(req.body?.address ?? "").trim();
    await deps.pool.query(`UPDATE account SET profile_name = $1, profile_address = $2 WHERE id = $3`,
      [name, address, req.account!.id]);
    await emitProgression();
    res.json({ name, address });
  }));

  app.put("/api/staff/:id/role", dependencies.requireAdmin, asyncRoute(async (req, res) => {
    const role = String(req.body?.role ?? "").trim();
    const targetId = Number(req.params.id);
    const updated = await deps.pool.query(
      `UPDATE account SET staff_role = $1 WHERE id = $2 AND is_staff = true RETURNING username`,
      [role, targetId],
    );
    if (updated.rowCount === 0) { res.status(404).json({ error: "staff account not found" }); return; }
    await recordActivity(deps.pool, req.account!.id, "assign role", updated.rows[0].username);
    await emitProgression();
    res.json({ ok: true });
  }));

  app.post("/api/catalog/products", dependencies.requireStaff, asyncRoute(async (req, res) => {
    const name = String(req.body?.name ?? "").trim();
    const category = String(req.body?.category ?? "").trim();
    const price = Number(req.body?.price);
    const variants = Array.isArray(req.body?.variants)
      ? req.body.variants.map(String).map((value: string) => value.trim()).filter(Boolean)
      : String(req.body?.variants ?? "").split(",").map((value) => value.trim()).filter(Boolean);
    if (!name || !category || !Number.isFinite(price) || price <= 0) {
      res.status(400).json({ error: "valid product values are required" }); return;
    }
    const client = await deps.pool.connect();
    try {
      await client.query("BEGIN");
      const created = await client.query(
        `INSERT INTO item (name, category, price, variants) VALUES ($1, $2, $3, $4) RETURNING id`,
        [name, category, price.toFixed(2), variants],
      );
      const warehouses = await client.query(`SELECT id FROM warehouse ORDER BY id`);
      for (const warehouse of warehouses.rows) {
        await client.query(`INSERT INTO stock (item_id, warehouse_id, quantity) VALUES ($1, $2, 0)`,
          [created.rows[0].id, warehouse.id]);
      }
      await recordActivity(client, req.account!.id, "create product", name);
      await client.query("COMMIT");
    } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
    await deps.broadcastCatalog();
    await emitProgression();
    res.json({ ok: true });
  }));

  app.post("/api/support/cases", asyncRoute(async (req, res) => {
    const email = String(req.body?.email ?? "").trim();
    const subject = String(req.body?.subject ?? "").trim();
    const message = String(req.body?.message ?? "").trim();
    if (!email || !subject || !message) { res.status(400).json({ error: "all ticket fields are required" }); return; }
    const reference = `SB-${Date.now().toString(36).toUpperCase()}-${Math.random().toString(36).slice(2, 6).toUpperCase()}`;
    const created = await deps.pool.query(
      `INSERT INTO support_case (account_id, email, subject, message, reference)
       VALUES ($1, $2, $3, $4, $5) RETURNING id`,
      [req.account?.id ?? null, email, subject, message, reference],
    );
    await emitProgression();
    res.json({ id: created.rows[0].id, reference });
  }));

  app.put("/api/support/cases/:id", dependencies.requireStaff, asyncRoute(async (req, res) => {
    const id = Number(req.params.id);
    const assignee = req.body?.assignee === undefined ? null : String(req.body.assignee);
    const priority = req.body?.priority === undefined ? null : String(req.body.priority);
    const status = req.body?.status === undefined ? null : String(req.body.status);
    const updated = await deps.pool.query(
      `UPDATE support_case SET assignee = COALESCE($1, assignee), priority = COALESCE($2, priority),
       status = COALESCE($3, status), updated_at = now()
       WHERE id = $4 RETURNING subject`, [assignee, priority, status, id]);
    if (updated.rowCount === 0) { res.status(404).json({ error: "case not found" }); return; }
    await recordActivity(deps.pool, req.account!.id, "update support", updated.rows[0].subject);
    await emitProgression(); res.json({ ok: true });
  }));

  app.post("/api/support/cases/:id/replies", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const id = Number(req.params.id);
    const message = String(req.body?.message ?? "").trim();
    const current = await one<any>(`SELECT account_id FROM support_case WHERE id = $1`, [id]);
    if (!current || (!req.account!.isAdmin && !req.account!.isStaff && current.account_id !== req.account!.id)) {
      res.status(404).json({ error: "case not found" }); return;
    }
    if (!message) { res.status(400).json({ error: "reply is required" }); return; }
    await deps.pool.query(`INSERT INTO support_reply (case_id, account_id, message) VALUES ($1, $2, $3)`,
      [id, req.account!.id, message]);
    await deps.pool.query(`UPDATE support_case SET updated_at = now() WHERE id = $1`, [id]);
    await emitProgression(); res.json({ ok: true });
  }));

  app.post("/api/support/cases/:id/order", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const caseId = Number(req.params.id);
    const orderId = Number(req.body?.orderId);
    const support = await one<any>(`SELECT account_id FROM support_case WHERE id = $1`, [caseId]);
    const order = await one<any>(`SELECT account_id FROM orders WHERE id = $1`, [orderId]);
    if (!support || !order || support.account_id !== order.account_id
      || (!req.account!.isAdmin && !req.account!.isStaff && order.account_id !== req.account!.id)) {
      res.status(403).json({ error: "order does not belong to this case" }); return;
    }
    await deps.pool.query(`UPDATE support_case SET order_id = $1, updated_at = now() WHERE id = $2`, [orderId, caseId]);
    await emitProgression(); res.json({ ok: true });
  }));

  app.post("/api/support/cases/:id/refund", dependencies.requireStaff, asyncRoute(async (req, res) => {
    const caseId = Number(req.params.id);
    const client = await deps.pool.connect();
    let accountId = 0;
    try {
      await client.query("BEGIN");
      const support = await client.query(`SELECT * FROM support_case WHERE id = $1 FOR UPDATE`, [caseId]);
      if (!support.rows[0]?.order_id) { await client.query("ROLLBACK"); res.status(409).json({ error: "case has no order" }); return; }
      const order = await client.query(`SELECT * FROM orders WHERE id = $1 FOR UPDATE`, [support.rows[0].order_id]);
      if (Number(order.rows[0].refund_total) > 0) { await client.query("ROLLBACK"); res.status(409).json({ error: "order already refunded" }); return; }
      const amount = Number(order.rows[0].payment_amount || order.rows[0].total);
      accountId = order.rows[0].account_id;
      await client.query(`UPDATE orders SET refund_total = $1 WHERE id = $2`, [amount, order.rows[0].id]);
      await client.query(`UPDATE support_case SET refund_total = $1, status = 'resolved', updated_at = now() WHERE id = $2`, [amount, caseId]);
      await client.query(`INSERT INTO refund_entry (order_id, support_case_id, amount) VALUES ($1, $2, $3)`, [order.rows[0].id, caseId, amount]);
      await recordActivity(client, req.account!.id, "refund order", String(order.rows[0].id));
      await client.query("COMMIT");
    } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
    await deps.broadcastOrders(accountId); await deps.broadcastCatalog(); await emitProgression();
    res.json({ ok: true });
  }));

  app.post("/api/promotions", dependencies.requireStaff, asyncRoute(async (req, res) => {
    const code = String(req.body?.code ?? "").trim().toUpperCase();
    const discount = Number(req.body?.discount);
    const start = new Date(String(req.body?.start || "2000-01-01"));
    const end = new Date(String(req.body?.end || "2100-01-01"));
    const limit = Number(req.body?.limit);
    if (!code || !(discount > 0 && discount <= 100) || Number.isNaN(start.valueOf())
      || Number.isNaN(end.valueOf()) || !Number.isInteger(limit) || limit < 1 || start >= end) {
      res.status(400).json({ error: "invalid promotion" }); return;
    }
    await deps.pool.query(
      `INSERT INTO promotion (code, discount_percent, start_at, end_at, redemption_limit, created_by)
       VALUES ($1, $2, $3, $4, $5, $6)
       ON CONFLICT (code) DO UPDATE SET discount_percent = excluded.discount_percent,
       start_at = excluded.start_at, end_at = excluded.end_at, redemption_limit = excluded.redemption_limit`,
      [code, discount, start, end, limit, req.account!.id],
    );
    await recordActivity(deps.pool, req.account!.id, "create promotion", code);
    await emitProgression(); res.json({ ok: true });
  }));

  app.post("/api/cart/promotion", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const code = String(req.body?.code ?? "").trim().toUpperCase();
    const promotion = await one<any>(
      `SELECT * FROM promotion WHERE code = $1 AND start_at <= now() AND end_at >= now()
       AND redemptions < redemption_limit`, [code]);
    if (!promotion) { res.status(409).json({ error: "promotion is expired or unavailable" }); return; }
    await deps.pool.query(
      `INSERT INTO cart (account_id, promotion_id, last_activity) VALUES ($1, $2, now())
       ON CONFLICT (account_id) DO UPDATE SET promotion_id = excluded.promotion_id, last_activity = now()`,
      [req.account!.id, promotion.id],
    );
    res.json({ ok: true, discount: Number(promotion.discount_percent) });
  }));

  app.put("/api/notifications/preferences", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const order = Boolean(req.body?.order);
    const stock = Boolean(req.body?.stock);
    await deps.pool.query(`UPDATE account SET notify_order = $1, notify_stock = $2 WHERE id = $3`,
      [order, stock, req.account!.id]);
    await emitProgression(); res.json({ order, stock });
  }));

  app.post("/api/items/:id/stock-alert", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const itemId = Number(req.params.id);
    const total = await one<any>(`SELECT COALESCE(SUM(quantity), 0)::int AS total FROM stock WHERE item_id = $1`, [itemId]);
    if (!total || total.total > 0) { res.status(409).json({ error: "item is already available" }); return; }
    await deps.pool.query(
      `INSERT INTO stock_alert_request (account_id, item_id) VALUES ($1, $2)
       ON CONFLICT (account_id, item_id) DO UPDATE SET fulfilled = false, created_at = now()`,
      [req.account!.id, itemId],
    );
    res.json({ ok: true });
  }));

  app.post("/api/admin/scheduled-restocks", dependencies.requireStaff, asyncRoute(async (req, res) => {
    const quantity = Number(req.body?.quantity);
    const delaySeconds = Number(req.body?.delaySeconds);
    const found = await findItemAndWarehouse(req.body?.item, req.body?.warehouse);
    if (!found.item || !found.warehouse || !Number.isInteger(quantity) || quantity < 1
      || !Number.isFinite(delaySeconds) || delaySeconds < 1) {
      res.status(400).json({ error: "invalid scheduled restock" }); return;
    }
    const created = await deps.pool.query(
      `INSERT INTO scheduled_restock (item_id, warehouse_id, quantity, due_at, created_by)
       VALUES ($1, $2, $3, now() + ($4 * interval '1 second'), $5) RETURNING id`,
      [found.item.id, found.warehouse.id, quantity, delaySeconds, req.account!.id],
    );
    await recordActivity(deps.pool, req.account!.id, "schedule restock", found.item.name);
    await emitProgression(); res.json({ id: created.rows[0].id });
  }));

  app.delete("/api/admin/scheduled-restocks/:id", dependencies.requireStaff, asyncRoute(async (req, res) => {
    const updated = await deps.pool.query(
      `UPDATE scheduled_restock SET cancelled = true WHERE id = $1 AND applied = false RETURNING item_id`,
      [Number(req.params.id)],
    );
    if (updated.rowCount === 0) { res.status(404).json({ error: "pending restock not found" }); return; }
    await emitProgression(); res.json({ ok: true });
  }));

  app.put("/api/reorders/:itemId", dependencies.requireStaff, asyncRoute(async (req, res) => {
    const itemId = Number(req.params.itemId);
    const threshold = Number(req.body?.threshold);
    const quantity = Number(req.body?.quantity);
    if (!Number.isInteger(itemId) || !Number.isInteger(threshold) || threshold < 0
      || !Number.isInteger(quantity) || quantity < 1) {
      res.status(400).json({ error: "invalid reorder rule" }); return;
    }
    await deps.pool.query(
      `INSERT INTO reorder_rule (item_id, threshold, quantity, created_by) VALUES ($1, $2, $3, $4)
       ON CONFLICT (item_id) DO UPDATE SET threshold = excluded.threshold, quantity = excluded.quantity,
       enabled = true, created_by = excluded.created_by`, [itemId, threshold, quantity, req.account!.id]);
    await recordActivity(deps.pool, req.account!.id, "save reorder rule", String(itemId));
    await emitProgression(); res.json({ ok: true });
  }));

  app.post("/api/cart/recover/:id", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const id = Number(req.params.id);
    const client = await deps.pool.connect();
    const unavailable: string[] = [];
    try {
      await client.query("BEGIN");
      const expired = await client.query(
        `SELECT * FROM expired_cart WHERE id = $1 AND account_id = $2 AND restored = false FOR UPDATE`,
        [id, req.account!.id],
      );
      if (!expired.rows[0]) { await client.query("ROLLBACK"); res.status(404).json({ error: "expired cart not found" }); return; }
      const cartResult = await client.query(
        `INSERT INTO cart (account_id, last_activity, expired_at) VALUES ($1, now(), null)
         ON CONFLICT (account_id) DO UPDATE SET last_activity = now(), expired_at = null RETURNING id`,
        [req.account!.id],
      );
      for (const saved of expired.rows[0].items as Array<{ itemId: number; name: string; quantity: number }>) {
        const inserted = await client.query(
          `INSERT INTO cart_item (cart_id, item_id, quantity, reserved_until, expired)
           VALUES ($1, $2, $3, now() + interval '90 seconds', false)
           ON CONFLICT (cart_id, item_id) DO UPDATE SET quantity = excluded.quantity,
           reserved_until = excluded.reserved_until, expired = false RETURNING id`,
          [cartResult.rows[0].id, saved.itemId, saved.quantity],
        );
        try {
          await allocateReservation(client, inserted.rows[0].id, saved.itemId, saved.quantity);
        } catch {
          await client.query(`DELETE FROM cart_item WHERE id = $1`, [inserted.rows[0].id]);
          unavailable.push(saved.name);
        }
      }
      await client.query(`UPDATE expired_cart SET restored = true WHERE id = $1`, [id]);
      await client.query("COMMIT");
    } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
    await deps.broadcastCart(req.account!.id); await deps.broadcastCatalog(); await emitProgression();
    res.json({ ok: true, unavailable });
  }));

  app.post("/api/recommendations/:itemId/dismiss", dependencies.requireAuth, asyncRoute(async (req, res) => {
    await deps.pool.query(
      `INSERT INTO recommendation_dismissal (account_id, item_id) VALUES ($1, $2) ON CONFLICT DO NOTHING`,
      [req.account!.id, Number(req.params.itemId)],
    );
    await emitProgression(); res.json({ ok: true });
  }));
}

export function attachProgressionSocket(server: SocketIOServer) {
  io = server;
}

export async function processProgressionTimers() {
  if (!deps) return;
  const client = await deps.pool.connect();
  const changedAccounts = new Set<number>();
  let catalogChanged = false;
  try {
    await client.query("BEGIN");
    const timerLock = await client.query(`SELECT pg_try_advisory_xact_lock(7392015) AS locked`);
    if (!timerLock.rows[0].locked) {
      await client.query("ROLLBACK");
      return;
    }

    const expiredReservations = await client.query(
      `SELECT ci.id, ci.item_id, c.account_id
       FROM cart_item ci JOIN cart c ON c.id = ci.cart_id
       WHERE ci.expired = false AND ci.reserved_until IS NOT NULL AND ci.reserved_until <= now()
       FOR UPDATE OF ci`,
    );
    for (const line of expiredReservations.rows) {
      await releaseReservation(client, line.id);
      await client.query(`UPDATE cart_item SET expired = true WHERE id = $1`, [line.id]);
      changedAccounts.add(line.account_id); catalogChanged = true;
    }

    const expiredCarts = await client.query(
      `SELECT c.id, c.account_id FROM cart c
       WHERE c.expired_at IS NULL AND c.last_activity <= now() - interval '5 minutes'
       AND EXISTS (SELECT 1 FROM cart_item ci WHERE ci.cart_id = c.id) FOR UPDATE OF c`,
    );
    for (const cart of expiredCarts.rows) {
      const lines = await client.query(
        `SELECT ci.id, ci.item_id, i.name, ci.quantity, ci.expired FROM cart_item ci JOIN item i ON i.id = ci.item_id
         WHERE ci.cart_id = $1 FOR UPDATE OF ci`, [cart.id]);
      for (const line of lines.rows.filter((entry) => !entry.expired)) {
        await releaseReservation(client, line.id);
      }
      await client.query(`INSERT INTO expired_cart (account_id, items) VALUES ($1, $2)`,
        [cart.account_id, JSON.stringify(lines.rows.map((line) => ({ itemId: line.item_id, name: line.name, quantity: line.quantity })))]);
      await client.query(`DELETE FROM cart_item WHERE cart_id = $1`, [cart.id]);
      await client.query(`UPDATE cart SET expired_at = now() WHERE id = $1`, [cart.id]);
      changedAccounts.add(cart.account_id); catalogChanged = true;
    }

    const delivered = await client.query(
      `UPDATE orders SET status = 'delivered', delivered_at = now()
       WHERE status = 'shipped' AND shipped_at <= now() - interval '60 seconds'
       RETURNING id, account_id`,
    );
    for (const order of delivered.rows) {
      changedAccounts.add(order.account_id);
      const preference = await client.query(`SELECT notify_order FROM account WHERE id = $1`, [order.account_id]);
      if (preference.rows[0]?.notify_order) {
        const items = await client.query(`SELECT string_agg(item_name, ', ') AS names FROM order_item WHERE order_id = $1`, [order.id]);
        await client.query(
          `INSERT INTO notification (account_id, kind, subject_key, message)
           VALUES ($1, 'delivery', $2, $3) ON CONFLICT DO NOTHING`,
          [order.account_id, String(order.id), `${items.rows[0].names} delivered`],
        );
      }
    }

    const due = await client.query(
      `SELECT * FROM scheduled_restock WHERE due_at <= now() AND cancelled = false AND applied = false FOR UPDATE`,
    );
    for (const restock of due.rows) {
      await client.query(
        `INSERT INTO stock (item_id, warehouse_id, quantity) VALUES ($1, $2, $3)
         ON CONFLICT (item_id, warehouse_id) DO UPDATE SET quantity = stock.quantity + excluded.quantity`,
        [restock.item_id, restock.warehouse_id, restock.quantity]);
      await client.query(`UPDATE scheduled_restock SET applied = true WHERE id = $1`, [restock.id]);
      await client.query(
        `INSERT INTO stock_ledger (item_id, warehouse_id, quantity, source) VALUES ($1, $2, $3, $4)`,
        [restock.item_id, restock.warehouse_id, restock.quantity, restock.automatic ? "automatic reorder" : "scheduled restock"]);
      await processStockAlerts(client, restock.item_id); catalogChanged = true;
    }

    const rules = await client.query(
      `SELECT rr.*, COALESCE(SUM(s.quantity), 0)::int AS available
       FROM reorder_rule rr LEFT JOIN stock s ON s.item_id = rr.item_id
       WHERE rr.enabled = true GROUP BY rr.id`,
    );
    for (const rule of rules.rows) {
      if (rule.available > rule.threshold) continue;
      const pending = await client.query(
        `SELECT 1 FROM scheduled_restock WHERE item_id = $1 AND automatic = true
         AND cancelled = false AND applied = false LIMIT 1`, [rule.item_id]);
      if (pending.rowCount) continue;
      const warehouse = await client.query(`SELECT id FROM warehouse ORDER BY id LIMIT 1`);
      await client.query(
        `INSERT INTO scheduled_restock (item_id, warehouse_id, quantity, due_at, automatic, created_by)
         VALUES ($1, $2, $3, now() + interval '90 seconds', true, $4)`,
        [rule.item_id, warehouse.rows[0].id, rule.quantity, rule.created_by]);
    }

    await client.query("COMMIT");
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }

  if (catalogChanged) await deps.broadcastCatalog();
  for (const accountId of changedAccounts) {
    await deps.broadcastCart(accountId);
    await deps.broadcastOrders(accountId);
  }
  if (changedAccounts.size || catalogChanged) await deps.broadcastFulfilment();
  await emitProgression();
}

export async function reserveCartItem(accountId: number, itemId: number, quantity: number) {
  const client = await deps.pool.connect();
  try {
    await client.query("BEGIN");
    const cart = await client.query(
      `INSERT INTO cart (account_id, last_activity, expired_at) VALUES ($1, now(), null)
       ON CONFLICT (account_id) DO UPDATE SET last_activity = now(), expired_at = null RETURNING id`, [accountId]);
    const cartId = cart.rows[0].id;
    const existing = await client.query(
      `SELECT id, quantity, expired FROM cart_item WHERE cart_id = $1 AND item_id = $2 FOR UPDATE`, [cartId, itemId]);
    if (existing.rows[0]?.expired) await releaseReservation(client, existing.rows[0].id);
    const inserted = await client.query(
      `INSERT INTO cart_item (cart_id, item_id, quantity, reserved_until, expired)
       VALUES ($1, $2, $3, now() + interval '90 seconds', false)
       ON CONFLICT (cart_id, item_id) DO UPDATE SET quantity = CASE WHEN cart_item.expired THEN excluded.quantity ELSE cart_item.quantity + excluded.quantity END,
       reserved_until = excluded.reserved_until, expired = false RETURNING id`, [cartId, itemId, quantity]);
    await allocateReservation(client, inserted.rows[0].id, itemId, quantity);
    await client.query("COMMIT");
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}

export async function setReservedCartQuantity(accountId: number, itemId: number, quantity: number) {
  const client = await deps.pool.connect();
  try {
    await client.query("BEGIN");
    const line = await client.query(
      `SELECT ci.id, ci.quantity, ci.expired FROM cart c JOIN cart_item ci ON ci.cart_id = c.id
       WHERE c.account_id = $1 AND ci.item_id = $2 FOR UPDATE OF ci`, [accountId, itemId]);
    if (!line.rows[0]) throw new Error("item not in cart");
    await releaseReservation(client, line.rows[0].id);
    await allocateReservation(client, line.rows[0].id, itemId, quantity);
    await client.query(
      `UPDATE cart_item SET quantity = $1, reserved_until = now() + interval '90 seconds', expired = false
       WHERE id = $2`, [quantity, line.rows[0].id]);
    await client.query(`UPDATE cart SET last_activity = now(), expired_at = null WHERE account_id = $1`, [accountId]);
    await client.query("COMMIT");
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}

export async function removeReservedCartItem(accountId: number, itemId: number) {
  const client = await deps.pool.connect();
  try {
    await client.query("BEGIN");
    const line = await client.query(
      `SELECT ci.id FROM cart c JOIN cart_item ci ON ci.cart_id = c.id
       WHERE c.account_id = $1 AND ci.item_id = $2 FOR UPDATE OF ci`, [accountId, itemId]);
    if (line.rows[0]) {
      await releaseReservation(client, line.rows[0].id);
      await client.query(`DELETE FROM cart_item WHERE id = $1`, [line.rows[0].id]);
    }
    await client.query(`UPDATE cart SET last_activity = now() WHERE account_id = $1`, [accountId]);
    await client.query("COMMIT");
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}

export async function checkoutReservedCart(accountId: number) {
  const client = await deps.pool.connect();
  try {
    await client.query("BEGIN");
    const cart = await client.query(
      `SELECT id, promotion_id FROM cart WHERE account_id = $1 FOR UPDATE`, [accountId]);
    if (!cart.rows[0]) throw new Error("cart is empty");
    const lines = await client.query(
      `SELECT ci.id, ci.item_id, ci.quantity, ci.expired, ci.reserved_until, i.name, i.price
       FROM cart_item ci JOIN item i ON i.id = ci.item_id
       WHERE ci.cart_id = $1 ORDER BY ci.item_id FOR UPDATE OF ci`, [cart.rows[0].id]);
    if (lines.rows.length === 0) throw new Error("cart is empty");
    if (lines.rows.some((line) => line.expired || !line.reserved_until || new Date(line.reserved_until) <= new Date())) {
      throw new Error("cart contains an expired reservation");
    }
    const subtotal = lines.rows.reduce((sum, line) => sum + Number(line.price) * line.quantity, 0);
    let discount = 0;
    let promotionId: number | null = null;
    if (cart.rows[0].promotion_id) {
      const promotion = await client.query(
        `SELECT * FROM promotion WHERE id = $1 FOR UPDATE`, [cart.rows[0].promotion_id]);
      const current = promotion.rows[0];
      if (!current || new Date(current.start_at) > new Date() || new Date(current.end_at) < new Date()
        || current.redemptions >= current.redemption_limit) throw new Error("promotion is expired or unavailable");
      promotionId = current.id;
      discount = Number((subtotal * Number(current.discount_percent) / 100).toFixed(2));
    }
    const total = Number((subtotal - discount).toFixed(2));
    const created = await client.query(
      `INSERT INTO orders
       (account_id, total, status, promotion_id, discount, payment_status, payment_amount)
       VALUES ($1, $2, 'pending', $3, $4, 'paid', $2) RETURNING id`,
      [accountId, total, promotionId, discount],
    );
    for (const line of lines.rows) {
      const allocations = await client.query(
        `SELECT warehouse_id, quantity FROM cart_reservation_allocation WHERE cart_item_id = $1 ORDER BY warehouse_id`,
        [line.id],
      );
      if (allocations.rows.reduce((sum, row) => sum + row.quantity, 0) !== line.quantity) {
        throw new Error("cart reservation is incomplete");
      }
      for (const allocation of allocations.rows) {
        await client.query(
          `INSERT INTO order_item (order_id, item_id, item_name, quantity, price, warehouse_id)
           VALUES ($1, $2, $3, $4, $5, $6)`,
          [created.rows[0].id, line.item_id, line.name, allocation.quantity, line.price, allocation.warehouse_id],
        );
      }
    }
    if (promotionId) {
      await client.query(
        `UPDATE promotion SET redemptions = redemptions + 1, revenue = revenue + $1 WHERE id = $2`,
        [total, promotionId],
      );
    }
    await client.query(`DELETE FROM cart_item WHERE cart_id = $1`, [cart.rows[0].id]);
    await client.query(`UPDATE cart SET promotion_id = null, last_activity = now() WHERE id = $1`, [cart.rows[0].id]);
    await client.query("COMMIT");
    return created.rows[0].id as number;
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}

export async function processImmediateRestock(itemId: number) {
  await processStockAlerts(deps.pool, itemId);
  await emitProgression();
}
