import type { Request, RequestHandler, Response } from "express";
import type { Express } from "express";
import type { Pool, PoolClient } from "pg";
import { drizzle } from "drizzle-orm/node-postgres";
import type { Server as SocketIOServer, Socket } from "socket.io";
import { and, asc, desc, eq, gte, ilike, inArray, lt, lte, or, sql } from "drizzle-orm";
import { db } from "./db.js";
import {
  account as accountTable,
  cart,
  cartItem,
  cartReservationAllocation,
  expiredCart,
  item,
  notification,
  orderItem,
  orders,
  promotion,
  recommendationDismissal,
  refundEntry,
  reorderRule,
  scheduledRestock,
  staffActivity,
  stock,
  stockAlertRequest,
  stockLedger,
  supportCase,
  supportReply,
  warehouse,
} from "./schema.js";

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
  buildRecommended: (accountId: number | null) => Promise<Array<{ id: number; name: string }>>;
};

let deps: Dependencies;
let io: SocketIOServer | null = null;

const asyncRoute = (fn: (req: AppRequest, res: Response) => Promise<void>): RequestHandler =>
  (req, res, next) => void fn(req as AppRequest, res).catch(next);

async function visibleSupportCases(account: ProgressionAccount | null) {
  if (!account) return [];
  const cases = await db.select({
    id: supportCase.id,
    accountId: supportCase.accountId,
    email: supportCase.email,
    subject: supportCase.subject,
    message: supportCase.message,
    reference: supportCase.reference,
    assignee: supportCase.assignee,
    priority: supportCase.priority,
    status: supportCase.status,
    orderId: supportCase.orderId,
    refundTotal: supportCase.refundTotal,
  }).from(supportCase).leftJoin(orders, eq(orders.id, supportCase.orderId))
    .where(account.isAdmin || account.isStaff ? undefined : eq(supportCase.accountId, account.id))
    .orderBy(desc(supportCase.createdAt));
  const replies = cases.length === 0 ? [] : await db.select({
    id: supportReply.id,
    caseId: supportReply.caseId,
    username: accountTable.username,
    message: supportReply.message,
    createdAt: supportReply.createdAt,
  }).from(supportReply).innerJoin(accountTable, eq(accountTable.id, supportReply.accountId))
    .where(inArray(supportReply.caseId, cases.map((entry) => entry.id)))
    .orderBy(asc(supportReply.createdAt));
  return cases.map((entry) => ({
    id: entry.id,
    accountId: entry.accountId,
    email: entry.email,
    subject: entry.subject,
    message: entry.message,
    reference: entry.reference,
    assignee: entry.assignee,
    priority: entry.priority,
    status: entry.status,
    orderId: entry.orderId,
    refundTotal: Number(entry.refundTotal),
    replies: replies.filter((reply) => reply.caseId === entry.id).map((reply) => ({
      id: reply.id,
      username: reply.username,
      message: reply.message,
      createdAt: reply.createdAt,
    })),
  }));
}

export async function buildProgressionState(account: ProgressionAccount | null) {
  const isStaff = Boolean(account && (account.isAdmin || account.isStaff));
  const [profiles, roles, promotions, preferences, notifications, completed, restocks, ledger,
    activity, reorders, expired, support, recommendations] = await Promise.all([
    account ? db.select({ name: accountTable.profileName, address: accountTable.profileAddress })
      .from(accountTable).where(eq(accountTable.id, account.id)).limit(1) : [],
    isStaff ? db.select({ id: accountTable.id, username: accountTable.username,
      role: accountTable.staffRole }).from(accountTable).where(eq(accountTable.isStaff, true))
      .orderBy(asc(accountTable.username)) : [],
    isStaff ? db.select().from(promotion).orderBy(desc(promotion.createdAt)) : [],
    account ? db.select({ order: accountTable.notifyOrder, stock: accountTable.notifyStock })
      .from(accountTable).where(eq(accountTable.id, account.id)).limit(1) : [],
    account ? db.select().from(notification).where(eq(notification.accountId, account.id))
      .orderBy(desc(notification.createdAt)) : [],
    isStaff ? db.execute<{ id: number; status: string; delivered_at: Date; items: string }>(sql`
      SELECT o.id, o.status, o.delivered_at, string_agg(oi.item_name, ', ' ORDER BY oi.id) AS items
       FROM orders o JOIN order_item oi ON oi.order_id = o.id
       WHERE o.status IN ('shipped', 'delivered') GROUP BY o.id ORDER BY o.id DESC`
    ) : { rows: [] },
    isStaff ? db.select({ id: scheduledRestock.id, item: item.name, warehouse: warehouse.name,
      quantity: scheduledRestock.quantity, dueAt: scheduledRestock.dueAt,
      automatic: scheduledRestock.automatic }).from(scheduledRestock)
      .innerJoin(item, eq(item.id, scheduledRestock.itemId))
      .innerJoin(warehouse, eq(warehouse.id, scheduledRestock.warehouseId))
      .where(and(eq(scheduledRestock.cancelled, false), eq(scheduledRestock.applied, false)))
      .orderBy(asc(scheduledRestock.dueAt)) : [],
    isStaff ? db.select({ id: stockLedger.id, item: item.name, warehouse: warehouse.name,
      quantity: stockLedger.quantity, source: stockLedger.source }).from(stockLedger)
      .innerJoin(item, eq(item.id, stockLedger.itemId))
      .innerJoin(warehouse, eq(warehouse.id, stockLedger.warehouseId))
      .orderBy(desc(stockLedger.createdAt)).limit(30) : [],
    isStaff ? db.select({ id: staffActivity.id, actor: accountTable.username,
      action: staffActivity.action, subject: staffActivity.subject, time: staffActivity.createdAt })
      .from(staffActivity).innerJoin(accountTable, eq(accountTable.id, staffActivity.accountId))
      .orderBy(desc(staffActivity.createdAt)).limit(50) : [],
    isStaff ? db.execute<{ id: number; item_name: string; threshold: number;
      quantity: number; pending: boolean }>(sql`
      SELECT rr.*, i.name AS item_name,
        EXISTS (SELECT 1 FROM scheduled_restock sr WHERE sr.item_id = rr.item_id AND sr.automatic = true
          AND sr.cancelled = false AND sr.applied = false) AS pending
       FROM reorder_rule rr JOIN item i ON i.id = rr.item_id ORDER BY i.name`
    ) : { rows: [] },
    account ? db.select().from(expiredCart)
      .where(and(eq(expiredCart.accountId, account.id), eq(expiredCart.restored, false)))
      .orderBy(desc(expiredCart.expiredAt)) : [],
    visibleSupportCases(account),
    account && !isStaff ? deps.buildRecommended(account.id) : [],
  ]);

  return {
    profile: profiles[0] ?? null,
    roles,
    promotions: promotions.map((row) => ({
      id: row.id, code: row.code, discount: Number(row.discountPercent),
      start: row.startAt.toISOString().slice(0, 10),
      end: row.endAt.toISOString().slice(0, 10),
      limit: row.redemptionLimit, redemptions: row.redemptions, revenue: Number(row.revenue),
    })),
    preferences: preferences[0] ?? null,
    notifications: notifications.map((row) => ({
      id: row.id, kind: row.kind, message: row.message, read: row.read,
    })),
    completedOrders: completed.rows.map((row) => ({
      id: row.id, status: row.status, items: row.items, deliveredAt: row.delivered_at,
    })),
    pendingRestocks: restocks,
    stockLedger: ledger,
    activity,
    reorders: reorders.rows.map((row) => ({
      id: row.id, item: row.item_name, threshold: row.threshold,
      quantity: row.quantity, pending: row.pending,
    })),
    expiredCarts: expired.map((row) => ({ id: row.id, items: row.items, expiredAt: row.expiredAt })),
    support,
    recommendations: recommendations.map((row, index) => ({
      id: row.id, name: row.name, rank: index + 1,
    })),
  };
}

export async function emitProgression() {
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
  const itemName = String(itemValue ?? "");
  const warehouseName = String(warehouseValue ?? "");
  const [items, warehouses] = await Promise.all([
    db.select({ id: item.id, name: item.name }).from(item)
      .where(or(eq(item.id, Number(itemValue) || -1), ilike(item.name, itemName))).limit(1),
    db.select({ id: warehouse.id, name: warehouse.name }).from(warehouse)
      .where(or(eq(warehouse.id, Number(warehouseValue) || -1), ilike(warehouse.name, warehouseName)))
      .limit(1),
  ]);
  return { item: items[0], warehouse: warehouses[0] };
}

async function processStockAlerts(client: PoolClient | Pool, itemId: number) {
  const database = drizzle(client);
  const current = await database.select({
    total: sql<number>`coalesce(sum(${stock.quantity}), 0)::int`,
  }).from(stock).where(eq(stock.itemId, itemId));
  if (current[0].total <= 0) return;
  const requests = await database.update(stockAlertRequest).set({ fulfilled: true })
    .where(and(eq(stockAlertRequest.itemId, itemId), eq(stockAlertRequest.fulfilled, false)))
    .returning({ accountId: stockAlertRequest.accountId });
  const itemRows = await database.select({ name: item.name }).from(item).where(eq(item.id, itemId))
    .limit(1);
  for (const request of requests) {
    await database.insert(notification).values({ accountId: request.accountId, kind: "stock",
      subjectKey: String(itemId), message: `${itemRows[0]?.name ?? "Item"} is back in stock` })
      .onConflictDoNothing();
  }
}

async function releaseReservation(client: PoolClient, cartItemId: number) {
  const database = drizzle(client);
  const allocations = await database.delete(cartReservationAllocation)
    .where(eq(cartReservationAllocation.cartItemId, cartItemId))
    .returning({ warehouseId: cartReservationAllocation.warehouseId,
      quantity: cartReservationAllocation.quantity });
  const lines = await database.select({ itemId: cartItem.itemId }).from(cartItem)
    .where(eq(cartItem.id, cartItemId)).limit(1);
  for (const allocation of allocations) {
    await database.update(stock).set({ quantity: sql`${stock.quantity} + ${allocation.quantity}` })
      .where(and(eq(stock.itemId, lines[0].itemId), eq(stock.warehouseId, allocation.warehouseId)));
  }
}

async function allocateReservation(client: PoolClient, cartItemId: number, itemId: number, quantity: number) {
  const database = drizzle(client);
  const stockRows = await database.select({ warehouseId: stock.warehouseId,
    quantity: stock.quantity }).from(stock).where(eq(stock.itemId, itemId))
    .orderBy(asc(stock.warehouseId)).for("update");
  const available = stockRows.reduce((sum, row) => sum + row.quantity, 0);
  if (available < quantity) throw new Error("not enough stock");
  let remaining = quantity;
  for (const row of stockRows) {
    const take = Math.min(remaining, row.quantity);
    if (take <= 0) continue;
    await database.update(stock).set({ quantity: sql`${stock.quantity} - ${take}` })
      .where(and(eq(stock.itemId, itemId), eq(stock.warehouseId, row.warehouseId)));
    await database.insert(cartReservationAllocation)
      .values({ cartItemId, warehouseId: row.warehouseId, quantity: take })
      .onConflictDoUpdate({ target: [cartReservationAllocation.cartItemId,
        cartReservationAllocation.warehouseId],
      set: { quantity: sql`${cartReservationAllocation.quantity} + ${take}` } });
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
    await db.update(accountTable).set({ profileName: name, profileAddress: address })
      .where(eq(accountTable.id, req.account!.id));
    await emitProgression();
    res.json({ name, address });
  }));

  app.put("/api/staff/:id/role", dependencies.requireAdmin, asyncRoute(async (req, res) => {
    const role = String(req.body?.role ?? "").trim();
    const targetId = Number(req.params.id);
    const updated = await db.update(accountTable).set({ staffRole: role })
      .where(and(eq(accountTable.id, targetId), eq(accountTable.isStaff, true)))
      .returning({ username: accountTable.username });
    if (updated.length === 0) { res.status(404).json({ error: "staff account not found" }); return; }
    await db.insert(staffActivity).values({ accountId: req.account!.id,
      action: "assign role", subject: updated[0].username });
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
    await db.transaction(async (tx) => {
      const created = await tx.insert(item).values({ name, category, price: price.toFixed(2),
        variants }).returning({ id: item.id });
      const warehouses = await tx.select({ id: warehouse.id }).from(warehouse)
        .orderBy(asc(warehouse.id));
      if (warehouses.length) {
        await tx.insert(stock).values(warehouses.map(({ id }) => ({
          itemId: created[0].id, warehouseId: id, quantity: 0,
        })));
      }
      await tx.insert(staffActivity).values({ accountId: req.account!.id,
        action: "create product", subject: name });
    });
    await deps.broadcastCatalog();
    await emitProgression();
    res.json({ ok: true });
  }));

  app.post("/api/support/cases", asyncRoute(async (req, res) => {
    const suppliedEmail = String(req.body?.email ?? "").trim();
    const email = suppliedEmail || (req.account ? `${req.account.username}@stackbench.local` : "");
    const subject = String(req.body?.subject ?? "").trim();
    const message = String(req.body?.message ?? "").trim();
    if (!email || !subject || !message) { res.status(400).json({ error: "all ticket fields are required" }); return; }
    const reference = `SB-${Date.now().toString(36).toUpperCase()}-${Math.random().toString(36).slice(2, 6).toUpperCase()}`;
    const created = await db.insert(supportCase).values({ accountId: req.account?.id ?? null,
      email, subject, message, reference }).returning({ id: supportCase.id });
    await emitProgression();
    res.json({ id: created[0].id, reference });
  }));

  app.put("/api/support/cases/:id", dependencies.requireStaff, asyncRoute(async (req, res) => {
    const id = Number(req.params.id);
    const assignee = req.body?.assignee === undefined ? null : String(req.body.assignee);
    const priority = req.body?.priority === undefined ? null : String(req.body.priority);
    const status = req.body?.status === undefined ? null : String(req.body.status);
    const updated = await db.update(supportCase).set({
      ...(assignee === null ? {} : { assignee }),
      ...(priority === null ? {} : { priority }),
      ...(status === null ? {} : { status }),
      updatedAt: new Date(),
    }).where(eq(supportCase.id, id)).returning({ subject: supportCase.subject });
    if (updated.length === 0) { res.status(404).json({ error: "case not found" }); return; }
    await db.insert(staffActivity).values({ accountId: req.account!.id,
      action: "update support", subject: updated[0].subject });
    await emitProgression(); res.json({ ok: true });
  }));

  app.post("/api/support/cases/:id/replies", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const id = Number(req.params.id);
    const message = String(req.body?.message ?? "").trim();
    const current = await db.select({ accountId: supportCase.accountId }).from(supportCase)
      .where(eq(supportCase.id, id)).limit(1);
    if (!current[0] || (!req.account!.isAdmin && !req.account!.isStaff
      && current[0].accountId !== req.account!.id)) {
      res.status(404).json({ error: "case not found" }); return;
    }
    if (!message) { res.status(400).json({ error: "reply is required" }); return; }
    await db.insert(supportReply).values({ caseId: id, accountId: req.account!.id, message });
    await db.update(supportCase).set({ updatedAt: new Date() }).where(eq(supportCase.id, id));
    await emitProgression(); res.json({ ok: true });
  }));

  app.post("/api/support/cases/:id/order", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const caseId = Number(req.params.id);
    const orderId = Number(req.body?.orderId);
    const [support, order] = await Promise.all([
      db.select({ accountId: supportCase.accountId }).from(supportCase)
        .where(eq(supportCase.id, caseId)).limit(1),
      db.select({ accountId: orders.accountId }).from(orders).where(eq(orders.id, orderId)).limit(1),
    ]);
    if (!support[0] || !order[0] || support[0].accountId !== order[0].accountId
      || (!req.account!.isAdmin && !req.account!.isStaff && order[0].accountId !== req.account!.id)) {
      res.status(403).json({ error: "order does not belong to this case" }); return;
    }
    await db.update(supportCase).set({ orderId, updatedAt: new Date() })
      .where(eq(supportCase.id, caseId));
    await emitProgression(); res.json({ ok: true });
  }));

  app.post("/api/support/cases/:id/refund", dependencies.requireStaff, asyncRoute(async (req, res) => {
    const caseId = Number(req.params.id);
    const result = await db.transaction(async (tx) => {
      const support = await tx.select({ orderId: supportCase.orderId }).from(supportCase)
        .where(eq(supportCase.id, caseId)).for("update");
      if (!support[0]?.orderId) return { error: "case has no order" } as const;
      const order = await tx.select().from(orders).where(eq(orders.id, support[0].orderId))
        .for("update");
      if (Number(order[0].refundTotal) > 0) return { error: "order already refunded" } as const;
      const amount = Number(order[0].paymentAmount || order[0].total);
      await tx.update(orders).set({ refundTotal: String(amount) }).where(eq(orders.id, order[0].id));
      await tx.update(supportCase).set({ refundTotal: String(amount), status: "resolved",
        updatedAt: new Date() }).where(eq(supportCase.id, caseId));
      await tx.insert(refundEntry).values({ orderId: order[0].id, supportCaseId: caseId,
        amount: String(amount) });
      await tx.insert(staffActivity).values({ accountId: req.account!.id,
        action: "refund order", subject: String(order[0].id) });
      return { accountId: order[0].accountId } as const;
    });
    if ("error" in result) { res.status(409).json({ error: result.error }); return; }
    await deps.broadcastOrders(result.accountId); await deps.broadcastCatalog(); await emitProgression();
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
    await db.insert(promotion).values({ code, discountPercent: String(discount), startAt: start,
      endAt: end, redemptionLimit: limit, createdBy: req.account!.id })
      .onConflictDoUpdate({ target: promotion.code,
        set: { discountPercent: String(discount), startAt: start, endAt: end,
          redemptionLimit: limit } });
    await db.insert(staffActivity).values({ accountId: req.account!.id,
      action: "create promotion", subject: code });
    await emitProgression(); res.json({ ok: true });
  }));

  app.post("/api/cart/promotion", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const code = String(req.body?.code ?? "").trim().toUpperCase();
    const available = await db.select().from(promotion).where(and(eq(promotion.code, code),
      lte(promotion.startAt, new Date()), gte(promotion.endAt, new Date()),
      lt(promotion.redemptions, promotion.redemptionLimit))).limit(1);
    if (!available[0]) { res.status(409).json({ error: "promotion is expired or unavailable" }); return; }
    await db.insert(cart).values({ accountId: req.account!.id, promotionId: available[0].id })
      .onConflictDoUpdate({ target: cart.accountId,
        set: { promotionId: available[0].id, lastActivity: new Date() } });
    res.json({ ok: true, discount: Number(available[0].discountPercent) });
  }));

  app.put("/api/notifications/preferences", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const order = Boolean(req.body?.order);
    const stock = Boolean(req.body?.stock);
    await db.update(accountTable).set({ notifyOrder: order, notifyStock: stock })
      .where(eq(accountTable.id, req.account!.id));
    await emitProgression(); res.json({ order, stock });
  }));

  app.post("/api/items/:id/stock-alert", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const itemId = Number(req.params.id);
    const total = await db.select({ value: sql<number>`coalesce(sum(${stock.quantity}), 0)::int` })
      .from(stock).where(eq(stock.itemId, itemId));
    if (total[0].value > 0) { res.status(409).json({ error: "item is already available" }); return; }
    await db.insert(stockAlertRequest).values({ accountId: req.account!.id, itemId })
      .onConflictDoUpdate({ target: [stockAlertRequest.accountId, stockAlertRequest.itemId],
        set: { fulfilled: false, createdAt: new Date() } });
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
    const created = await db.insert(scheduledRestock).values({ itemId: found.item.id,
      warehouseId: found.warehouse.id, quantity,
      dueAt: new Date(Date.now() + delaySeconds * 1000), createdBy: req.account!.id })
      .returning({ id: scheduledRestock.id });
    await db.insert(staffActivity).values({ accountId: req.account!.id,
      action: "schedule restock", subject: found.item.name });
    await emitProgression(); res.json({ id: created[0].id });
  }));

  app.delete("/api/admin/scheduled-restocks/:id", dependencies.requireStaff, asyncRoute(async (req, res) => {
    const updated = await db.update(scheduledRestock).set({ cancelled: true })
      .where(and(eq(scheduledRestock.id, Number(req.params.id)), eq(scheduledRestock.applied, false)))
      .returning({ itemId: scheduledRestock.itemId });
    if (updated.length === 0) { res.status(404).json({ error: "pending restock not found" }); return; }
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
    await db.insert(reorderRule).values({ itemId, threshold, quantity, createdBy: req.account!.id })
      .onConflictDoUpdate({ target: reorderRule.itemId,
        set: { threshold, quantity, enabled: true, createdBy: req.account!.id } });
    await db.insert(staffActivity).values({ accountId: req.account!.id,
      action: "save reorder rule", subject: String(itemId) });
    await emitProgression(); res.json({ ok: true });
  }));

  app.post("/api/cart/recover/:id", dependencies.requireAuth, asyncRoute(async (req, res) => {
    const id = Number(req.params.id);
    const client = await deps.pool.connect();
    const unavailable: string[] = [];
    try {
      await client.query("BEGIN");
      const database = drizzle(client);
      const expired = await database.select().from(expiredCart).where(and(eq(expiredCart.id, id),
        eq(expiredCart.accountId, req.account!.id), eq(expiredCart.restored, false))).for("update");
      if (!expired[0]) { await client.query("ROLLBACK"); res.status(404).json({ error: "expired cart not found" }); return; }
      const cartResult = await database.insert(cart).values({ accountId: req.account!.id,
        expiredAt: null }).onConflictDoUpdate({ target: cart.accountId,
        set: { lastActivity: new Date(), expiredAt: null } }).returning({ id: cart.id });
      for (const saved of expired[0].items as Array<{ itemId: number; name: string; quantity: number }>) {
        const reservedUntil = new Date(Date.now() + 90_000);
        const inserted = await database.insert(cartItem).values({ cartId: cartResult[0].id,
          itemId: saved.itemId, quantity: saved.quantity, reservedUntil, expired: false })
          .onConflictDoUpdate({ target: [cartItem.cartId, cartItem.itemId],
            set: { quantity: saved.quantity, reservedUntil, expired: false } })
          .returning({ id: cartItem.id });
        try {
          await allocateReservation(client, inserted[0].id, saved.itemId, saved.quantity);
        } catch {
          await database.delete(cartItem).where(eq(cartItem.id, inserted[0].id));
          unavailable.push(saved.name);
        }
      }
      await database.update(expiredCart).set({ restored: true }).where(eq(expiredCart.id, id));
      await client.query("COMMIT");
    } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
    await deps.broadcastCart(req.account!.id); await deps.broadcastCatalog(); await emitProgression();
    res.json({ ok: true, unavailable });
  }));

  app.post("/api/recommendations/:itemId/dismiss", dependencies.requireAuth, asyncRoute(async (req, res) => {
    await db.insert(recommendationDismissal).values({ accountId: req.account!.id,
      itemId: Number(req.params.itemId) }).onConflictDoNothing();
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
    const database = drizzle(client);

    const expiredReservations = await database.select({ id: cartItem.id, itemId: cartItem.itemId,
      accountId: cart.accountId }).from(cartItem).innerJoin(cart, eq(cart.id, cartItem.cartId))
      .where(and(eq(cartItem.expired, false), sql`${cartItem.reservedUntil} IS NOT NULL`,
        lte(cartItem.reservedUntil, new Date()))).for("update");
    for (const line of expiredReservations) {
      await releaseReservation(client, line.id);
      await database.update(cartItem).set({ expired: true }).where(eq(cartItem.id, line.id));
      changedAccounts.add(line.accountId); catalogChanged = true;
    }

    const expiredCarts = await database.select({ id: cart.id, accountId: cart.accountId }).from(cart)
      .where(and(sql`${cart.expiredAt} IS NULL`, lte(cart.lastActivity, new Date(Date.now() - 300_000)),
        sql`EXISTS (SELECT 1 FROM ${cartItem} WHERE ${cartItem.cartId} = ${cart.id})`)).for("update");
    for (const staleCart of expiredCarts) {
      const lines = await database.select({ id: cartItem.id, itemId: cartItem.itemId,
        name: item.name, quantity: cartItem.quantity, expired: cartItem.expired }).from(cartItem)
        .innerJoin(item, eq(item.id, cartItem.itemId)).where(eq(cartItem.cartId, staleCart.id))
        .for("update");
      for (const line of lines.filter((entry) => !entry.expired)) {
        await releaseReservation(client, line.id);
      }
      await database.insert(expiredCart).values({ accountId: staleCart.accountId,
        items: lines.map((line) => ({ itemId: line.itemId, name: line.name, quantity: line.quantity })) });
      await database.delete(cartItem).where(eq(cartItem.cartId, staleCart.id));
      await database.update(cart).set({ expiredAt: new Date() }).where(eq(cart.id, staleCart.id));
      changedAccounts.add(staleCart.accountId); catalogChanged = true;
    }

    const delivered = await database.update(orders).set({ status: "delivered", deliveredAt: new Date() })
      .where(and(eq(orders.status, "shipped"), lte(orders.shippedAt, new Date(Date.now() - 60_000))))
      .returning({ id: orders.id, accountId: orders.accountId });
    for (const order of delivered) {
      changedAccounts.add(order.accountId);
      const preference = await database.select({ notifyOrder: accountTable.notifyOrder })
        .from(accountTable).where(eq(accountTable.id, order.accountId)).limit(1);
      if (preference[0]?.notifyOrder) {
        const names = await database.select({ value: sql<string>`string_agg(${orderItem.itemName}, ', ')` })
          .from(orderItem).where(eq(orderItem.orderId, order.id));
        await database.insert(notification).values({ accountId: order.accountId, kind: "delivery",
          subjectKey: String(order.id), message: `${names[0].value} delivered` }).onConflictDoNothing();
      }
    }

    const due = await database.select().from(scheduledRestock)
      .where(and(lte(scheduledRestock.dueAt, new Date()), eq(scheduledRestock.cancelled, false),
        eq(scheduledRestock.applied, false))).for("update");
    for (const restock of due) {
      await database.insert(stock).values({ itemId: restock.itemId, warehouseId: restock.warehouseId,
        quantity: restock.quantity }).onConflictDoUpdate({ target: [stock.itemId, stock.warehouseId],
        set: { quantity: sql`${stock.quantity} + ${restock.quantity}` } });
      await database.update(scheduledRestock).set({ applied: true }).where(eq(scheduledRestock.id, restock.id));
      await database.insert(stockLedger).values({ itemId: restock.itemId, warehouseId: restock.warehouseId,
        quantity: restock.quantity, source: restock.automatic ? "automatic reorder" : "scheduled restock" });
      await processStockAlerts(client, restock.itemId); catalogChanged = true;
    }

    const rules = await database.select({ itemId: reorderRule.itemId, threshold: reorderRule.threshold,
      quantity: reorderRule.quantity, createdBy: reorderRule.createdBy,
      available: sql<number>`coalesce(sum(${stock.quantity}), 0)::int` }).from(reorderRule)
      .leftJoin(stock, eq(stock.itemId, reorderRule.itemId)).where(eq(reorderRule.enabled, true))
      .groupBy(reorderRule.id);
    for (const rule of rules) {
      if (rule.available > rule.threshold) continue;
      const pending = await database.select({ id: scheduledRestock.id }).from(scheduledRestock)
        .where(and(eq(scheduledRestock.itemId, rule.itemId), eq(scheduledRestock.automatic, true),
          eq(scheduledRestock.cancelled, false), eq(scheduledRestock.applied, false))).limit(1);
      if (pending.length > 0) continue;
      const firstWarehouse = await database.select({ id: warehouse.id }).from(warehouse)
        .orderBy(asc(warehouse.id)).limit(1);
      await database.insert(scheduledRestock).values({ itemId: rule.itemId,
        warehouseId: firstWarehouse[0].id, quantity: rule.quantity,
        dueAt: new Date(Date.now() + 90_000), automatic: true, createdBy: rule.createdBy });
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
    const database = drizzle(client);
    const carts = await database.insert(cart).values({ accountId, expiredAt: null })
      .onConflictDoUpdate({ target: cart.accountId,
        set: { lastActivity: new Date(), expiredAt: null } }).returning({ id: cart.id });
    const existing = await database.select({ id: cartItem.id, expired: cartItem.expired })
      .from(cartItem).where(and(eq(cartItem.cartId, carts[0].id), eq(cartItem.itemId, itemId)))
      .for("update");
    if (existing[0]?.expired) await releaseReservation(client, existing[0].id);
    const reservedUntil = new Date(Date.now() + 90_000);
    const inserted = await database.insert(cartItem).values({ cartId: carts[0].id, itemId,
      quantity, reservedUntil, expired: false }).onConflictDoUpdate({
        target: [cartItem.cartId, cartItem.itemId],
        set: { quantity: sql`case when ${cartItem.expired} then ${quantity} else ${cartItem.quantity} + ${quantity} end`,
          reservedUntil, expired: false },
      }).returning({ id: cartItem.id });
    await allocateReservation(client, inserted[0].id, itemId, quantity);
    await client.query("COMMIT");
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}

export async function setReservedCartQuantity(accountId: number, itemId: number, quantity: number) {
  const client = await deps.pool.connect();
  try {
    await client.query("BEGIN");
    const database = drizzle(client);
    const lines = await database.select({ id: cartItem.id }).from(cart)
      .innerJoin(cartItem, eq(cartItem.cartId, cart.id))
      .where(and(eq(cart.accountId, accountId), eq(cartItem.itemId, itemId))).for("update");
    if (!lines[0]) throw new Error("item not in cart");
    await releaseReservation(client, lines[0].id);
    await allocateReservation(client, lines[0].id, itemId, quantity);
    await database.update(cartItem).set({ quantity, reservedUntil: new Date(Date.now() + 90_000),
      expired: false }).where(eq(cartItem.id, lines[0].id));
    await database.update(cart).set({ lastActivity: new Date(), expiredAt: null })
      .where(eq(cart.accountId, accountId));
    await client.query("COMMIT");
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}

export async function removeReservedCartItem(accountId: number, itemId: number) {
  const client = await deps.pool.connect();
  try {
    await client.query("BEGIN");
    const database = drizzle(client);
    const lines = await database.select({ id: cartItem.id }).from(cart)
      .innerJoin(cartItem, eq(cartItem.cartId, cart.id))
      .where(and(eq(cart.accountId, accountId), eq(cartItem.itemId, itemId))).for("update");
    if (lines[0]) {
      await releaseReservation(client, lines[0].id);
      await database.delete(cartItem).where(eq(cartItem.id, lines[0].id));
    }
    await database.update(cart).set({ lastActivity: new Date() }).where(eq(cart.accountId, accountId));
    await client.query("COMMIT");
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}

export async function checkoutReservedCart(accountId: number) {
  const client = await deps.pool.connect();
  try {
    await client.query("BEGIN");
    const database = drizzle(client);
    const carts = await database.select({ id: cart.id, promotionId: cart.promotionId }).from(cart)
      .where(eq(cart.accountId, accountId)).for("update");
    if (!carts[0]) throw new Error("cart is empty");
    const lines = await database.select({ id: cartItem.id, itemId: cartItem.itemId,
      quantity: cartItem.quantity, expired: cartItem.expired,
      reservedUntil: cartItem.reservedUntil, name: item.name, price: item.price })
      .from(cartItem).innerJoin(item, eq(item.id, cartItem.itemId))
      .where(eq(cartItem.cartId, carts[0].id)).orderBy(asc(cartItem.itemId)).for("update");
    if (lines.length === 0) throw new Error("cart is empty");
    if (lines.some((line) => line.expired || !line.reservedUntil || line.reservedUntil <= new Date())) {
      throw new Error("cart contains an expired reservation");
    }
    const subtotal = lines.reduce((sum, line) => sum + Number(line.price) * line.quantity, 0);
    let discount = 0;
    let promotionId: number | null = null;
    if (carts[0].promotionId) {
      const promotions = await database.select().from(promotion)
        .where(eq(promotion.id, carts[0].promotionId)).for("update");
      const current = promotions[0];
      if (!current || current.startAt > new Date() || current.endAt < new Date()
        || current.redemptions >= current.redemptionLimit) throw new Error("promotion is expired or unavailable");
      promotionId = current.id;
      discount = Number((subtotal * Number(current.discountPercent) / 100).toFixed(2));
    }
    const total = Number((subtotal - discount).toFixed(2));
    const created = await database.insert(orders).values({ accountId, total: String(total),
      promotionId, discount: String(discount), paymentAmount: String(total) })
      .returning({ id: orders.id });
    for (const line of lines) {
      const allocations = await database.select({ warehouseId: cartReservationAllocation.warehouseId,
        quantity: cartReservationAllocation.quantity }).from(cartReservationAllocation)
        .where(eq(cartReservationAllocation.cartItemId, line.id))
        .orderBy(asc(cartReservationAllocation.warehouseId));
      if (allocations.reduce((sum, row) => sum + row.quantity, 0) !== line.quantity) {
        throw new Error("cart reservation is incomplete");
      }
      for (const allocation of allocations) {
        await database.insert(orderItem).values({ orderId: created[0].id, itemId: line.itemId,
          itemName: line.name, quantity: allocation.quantity, price: line.price,
          warehouseId: allocation.warehouseId });
      }
    }
    if (promotionId) {
      await database.update(promotion).set({ redemptions: sql`${promotion.redemptions} + 1`,
        revenue: sql`${promotion.revenue} + ${total}` }).where(eq(promotion.id, promotionId));
    }
    await database.delete(cartItem).where(eq(cartItem.cartId, carts[0].id));
    await database.update(cart).set({ promotionId: null, lastActivity: new Date() })
      .where(eq(cart.id, carts[0].id));
    await client.query("COMMIT");
    return created[0].id;
  } catch (error) { await client.query("ROLLBACK"); throw error; } finally { client.release(); }
}

export async function processImmediateRestock(itemId: number) {
  await processStockAlerts(deps.pool, itemId);
  await emitProgression();
}
