import type express from "express";
import { Router } from "express";
import type { Server as SocketIOServer } from "socket.io";
import jwt from "jsonwebtoken";
import { Types } from "mongoose";

import { Cart, Item, Order, Stock, User, Warehouse } from "./models.js";
import {
  Activity, CartArchive, Dismissal, Notification, Payment, Preference, Profile, Promotion,
  ReorderRule, ScheduledRestock, StockAlert, StockLedger, SupportTicket,
} from "./progression-models.js";

type Request = express.Request & { progressionUser?: any };

function cleanText(value: unknown) {
  return typeof value === "string" ? value.trim() : "";
}

function number(value: unknown) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : NaN;
}

function publicUser(user: any) {
  return { id: String(user._id), username: user.username, isAdmin: user.isAdmin,
    isStaff: user.isStaff, roles: user.roles || [] };
}

export function installProgressionRoutes(app: express.Express, io: SocketIOServer,
  { jwtSecret }: { jwtSecret: string }) {
  const router = Router();
  const supportRouter = Router();
  const adminRouter = Router();

  async function userFromRequest(req: express.Request) {
    const header = req.headers.authorization;
    if (!header?.startsWith("Bearer ")) return null;
    try {
      const payload = jwt.verify(header.slice(7), jwtSecret) as { sub: string };
      return await User.findById(payload.sub);
    } catch {
      return null;
    }
  }

  async function optionalAuth(req: Request, _res: express.Response, next: express.NextFunction) {
    req.progressionUser = await userFromRequest(req);
    next();
  }

  async function auth(req: Request, res: express.Response, next: express.NextFunction) {
    req.progressionUser = await userFromRequest(req);
    if (!req.progressionUser) return res.status(401).json({ error: "Sign in required" });
    next();
  }

  function staff(req: Request, res: express.Response, next: express.NextFunction) {
    if (!(req.progressionUser?.isStaff || req.progressionUser?.isAdmin)) {
      return res.status(403).json({ error: "Staff access required" });
    }
    next();
  }

  function admin(req: Request, res: express.Response, next: express.NextFunction) {
    if (!req.progressionUser?.isAdmin) return res.status(403).json({ error: "Admin access required" });
    next();
  }

  function changed(userId?: string) {
    io.emit("progression:update", { userId: userId || null });
  }

  async function recordActivity(user: any, action: string, subject: string) {
    await Activity.create({ actorId: user._id, actor: user.username, action, subject });
  }

  async function ownedTicket(req: Request, id: string) {
    const ticket = await SupportTicket.findById(id);
    if (!ticket) return null;
    const user = req.progressionUser;
    if (user?.isStaff || user?.isAdmin || (ticket.userId && String(ticket.userId) === String(user?._id))) {
      return ticket;
    }
    return null;
  }

  async function ensurePayments(userId: Types.ObjectId) {
    const orders = await Order.find({ userId, status: { $nin: ["cancelled"] } });
    for (const order of orders) {
      await Payment.updateOne({ orderId: order._id }, {
        $setOnInsert: { orderId: order._id, userId, amount: order.total, status: "paid" },
      }, { upsert: true });
    }
  }

  router.get("/state", optionalAuth, async (req: Request, res) => {
    const user = req.progressionUser;
    const isStaff = Boolean(user?.isStaff || user?.isAdmin);
    if (user) await ensurePayments(user._id);
    const ticketFilter = isStaff ? {} : user ? { userId: user._id } : { _id: null };
    const orderIds = user ? (await Order.find({ userId: user._id }).select("_id")).map(order => order._id) : [];
    const [profile, tickets, promotions, preference, notifications, scheduledRestocks,
      ledger, reorderRules, activities, payments, dismissals, archive, staffUsers, ownOrders,
      warehouses] = await Promise.all([
      user ? Profile.findOne({ userId: user._id }) : null,
      SupportTicket.find(ticketFilter).sort({ createdAt: -1 }),
      isStaff ? Promotion.find().sort({ createdAt: -1 }) : [],
      user ? Preference.findOne({ userId: user._id }) : null,
      user ? Notification.find({ userId: user._id }).sort({ createdAt: -1 }) : [],
      isStaff ? ScheduledRestock.find({ status: "pending" }).sort({ dueAt: 1 }) : [],
      isStaff ? StockLedger.find().sort({ createdAt: -1 }).limit(20) : [],
      isStaff ? ReorderRule.find() : [],
      isStaff ? Activity.find().sort({ createdAt: -1 }).limit(30) : [],
      user ? Payment.find({ orderId: { $in: orderIds } }) : [],
      user ? Dismissal.find({ userId: user._id }) : [],
      user ? CartArchive.findOne({ userId: user._id }) : null,
      user?.isAdmin ? User.find({ isStaff: true }).sort({ username: 1 }) : [],
      isStaff ? Order.find().sort({ createdAt: -1 })
        : user ? Order.find({ userId: user._id }).sort({ createdAt: -1 }) : [],
      isStaff ? Warehouse.find().sort({ name: 1 }) : [],
    ]);
    const ticketJson = await Promise.all(tickets.map(async ticket => {
      const value = ticket.toJSON() as any;
      value.order = ticket.orderId ? await Order.findById(ticket.orderId) : null;
      return value;
    }));
    res.json({
      user: user ? publicUser(user) : null,
      profile, tickets: ticketJson, promotions, preference: preference || { order: false, stock: false },
      notifications, scheduledRestocks, ledger, reorderRules, activities, payments,
      dismissals: dismissals.map(item => String(item.itemId)), archive,
      staffUsers: staffUsers.map(publicUser), orders: ownOrders, warehouses,
    });
  });

  router.put("/profile", auth, async (req: Request, res) => {
    const name = cleanText(req.body?.name);
    const address = cleanText(req.body?.address);
    if (!name || !address) return res.status(400).json({ error: "Name and address are required" });
    const profile = await Profile.findOneAndUpdate({ userId: req.progressionUser._id },
      { userId: req.progressionUser._id, name, address }, { upsert: true, new: true });
    changed(String(req.progressionUser._id));
    res.json({ profile });
  });

  router.post("/staff/roles", auth, admin, async (req: Request, res) => {
    const username = cleanText(req.body?.username);
    const roles = Array.isArray(req.body?.roles) ? req.body.roles.map(cleanText).filter(Boolean) : [];
    const target = await User.findOne({ username });
    if (!target) return res.status(404).json({ error: "Account not found" });
    target.roles = roles;
    target.isStaff = roles.length > 0 || target.isStaff;
    await target.save();
    await recordActivity(req.progressionUser, "updated roles", username);
    changed();
    res.json({ user: publicUser(target) });
  });

  router.post("/catalog", auth, staff, async (req: Request, res) => {
    const name = cleanText(req.body?.name);
    const price = number(req.body?.price);
    const category = cleanText(req.body?.category) || "Uncategorized";
    const variants = cleanText(req.body?.variants).split(",").map(value => value.trim()).filter(Boolean);
    if (!name || !(price >= 0)) return res.status(400).json({ error: "Name and price are required" });
    const item = await Item.create({ name, price, category, variants });
    const warehouses = await Warehouse.find();
    await Stock.insertMany(warehouses.map(warehouse => ({ item_id: item._id,
      warehouse_id: warehouse._id, quantity: 0 })));
    await recordActivity(req.progressionUser, "created catalog item", name);
    changed();
    res.json({ item });
  });

  router.post("/support", optionalAuth, async (req: Request, res) => {
    const subject = cleanText(req.body?.subject);
    const message = cleanText(req.body?.message);
    const email = cleanText(req.body?.email);
    if (!subject || !message || (!req.progressionUser && !email)) {
      return res.status(400).json({ error: "Contact, subject, and message are required" });
    }
    const ticket = await SupportTicket.create({
      reference: `SUP-${Date.now().toString(36).toUpperCase()}`,
      userId: req.progressionUser?._id || null, email, subject, message,
    });
    changed(req.progressionUser ? String(req.progressionUser._id) : undefined);
    res.status(201).json({ ticket });
  });

  router.patch("/support/:id", auth, staff, async (req: Request, res) => {
    const update: Record<string, string> = {};
    for (const key of ["assignee", "priority", "status"]) {
      const value = cleanText(req.body?.[key]);
      if (value) update[key] = value;
    }
    const ticket = await SupportTicket.findByIdAndUpdate(req.params.id, { $set: update }, { new: true });
    if (!ticket) return res.status(404).json({ error: "Case not found" });
    changed(ticket.userId ? String(ticket.userId) : undefined);
    res.json({ ticket });
  });

  router.post("/support/:id/replies", auth, async (req: Request, res) => {
    const ticket = await ownedTicket(req, req.params.id);
    if (!ticket) return res.status(404).json({ error: "Case not found" });
    const body = cleanText(req.body?.body);
    if (!body) return res.status(400).json({ error: "Reply is required" });
    ticket.replies.push({ userId: req.progressionUser._id, username: req.progressionUser.username,
      body } as any);
    await ticket.save();
    changed(ticket.userId ? String(ticket.userId) : undefined);
    res.json({ ticket });
  });

  supportRouter.post("/cases/:caseId/order", auth, async (req: Request, res) => {
    const ticket = await SupportTicket.findOne({ _id: req.params.caseId,
      userId: req.progressionUser._id });
    if (!ticket) return res.status(404).json({ error: "Case not found" });
    const orderId = cleanText(req.body?.orderId);
    const order = await Order.findOne({ _id: orderId, userId: req.progressionUser._id });
    if (!order) return res.status(403).json({ error: "Order does not belong to this account" });
    ticket.orderId = order._id;
    await ticket.save();
    changed(String(req.progressionUser._id));
    res.json({ ticket });
  });

  supportRouter.post("/cases/:caseId/refund", auth, staff, async (req: Request, res) => {
    const ticket = await SupportTicket.findById(req.params.caseId);
    if (!ticket?.orderId) return res.status(404).json({ error: "Order-linked case not found" });
    if (ticket.refundTotal > 0) {
      return res.status(409).json({ error: "Order has already been refunded" });
    }
    const order = await Order.findOneAndUpdate({ _id: ticket.orderId,
      status: { $nin: ["refunded", "cancelled"] } }, { $set: { status: "refunded" } }, { new: true });
    if (!order) return res.status(409).json({ error: "Order has already been refunded" });
    for (const line of order.items) {
      if (line.returned) continue;
      for (const allocation of line.allocations) {
        await Stock.updateOne({ item_id: line.itemId, warehouse_id: allocation.warehouseId },
          { $inc: { quantity: allocation.quantity } });
      }
    }
    order.refundTotal = order.total;
    ticket.status = "resolved";
    ticket.refundTotal = order.total;
    await Promise.all([order.save(), ticket.save(), Payment.updateOne({ orderId: order._id },
      { $set: { status: "refunded" } })]);
    await Notification.updateOne({ userId: order.userId, key: `refund:${order._id}` }, {
      $setOnInsert: { userId: order.userId, key: `refund:${order._id}`, type: "refund",
        message: `Refunded ${order.items.map(item => item.name).join(", ")}` },
    }, { upsert: true });
    await recordActivity(req.progressionUser, "refunded support case", ticket.reference);
    changed(String(order.userId));
    const ownerOrders = await Order.find({ userId: order.userId }).sort({ createdAt: -1 });
    io.to(`user:${order.userId}`).emit("orders:update", ownerOrders.map(value => value.toJSON()));
    res.json({ ticket, order });
  });

  router.post("/promotions", auth, staff, async (req: Request, res) => {
    const code = cleanText(req.body?.code).toUpperCase();
    const discount = number(req.body?.discount);
    const limit = number(req.body?.limit);
    const start = new Date(req.body?.start);
    const end = new Date(req.body?.end);
    if (!code || !(discount > 0 && discount <= 100) || !(limit >= 1)
      || !Number.isFinite(start.getTime()) || !Number.isFinite(end.getTime()) || end <= start) {
      return res.status(400).json({ error: "Promotion values are invalid" });
    }
    const promotion = await Promotion.findOneAndUpdate({ code },
      { code, discount, limit, start, end }, { upsert: true, new: true });
    await recordActivity(req.progressionUser, "created promotion", code);
    changed();
    res.json({ promotion });
  });

  router.post("/cart/promotion", auth, async (req: Request, res) => {
    const code = cleanText(req.body?.code).toUpperCase();
    const promotion = await Promotion.findOne({ code });
    const now = new Date();
    if (!promotion || promotion.start > now || promotion.end < now
      || promotion.redemptions >= promotion.limit) {
      return res.status(400).json({ error: "Promotion is expired or unavailable" });
    }
    const cart = await Cart.findOneAndUpdate({ userId: req.progressionUser._id },
      { $set: { promotionCode: code, discount: promotion.discount } }, { new: true });
    changed(String(req.progressionUser._id));
    res.json({ promotion, cart });
  });

  router.put("/preferences", auth, async (req: Request, res) => {
    const preference = await Preference.findOneAndUpdate({ userId: req.progressionUser._id }, {
      userId: req.progressionUser._id, order: Boolean(req.body?.order), stock: Boolean(req.body?.stock),
    }, { upsert: true, new: true });
    changed(String(req.progressionUser._id));
    res.json({ preference });
  });

  router.post("/stock-alerts", auth, async (req: Request, res) => {
    const item = await Item.findById(req.body?.itemId);
    if (!item) return res.status(404).json({ error: "Item not found" });
    const alert = await StockAlert.findOneAndUpdate({ userId: req.progressionUser._id,
      itemId: item._id }, { $setOnInsert: { userId: req.progressionUser._id, itemId: item._id,
      sent: false } }, { upsert: true, new: true });
    res.json({ alert });
  });

  const scheduleRestock = async (req: Request, res: express.Response) => {
    const item = await Item.findById(req.body?.itemId);
    const warehouse = await Warehouse.findById(req.body?.warehouseId);
    const quantity = number(req.body?.quantity);
    const delaySeconds = number(req.body?.delaySeconds);
    if (!item || !warehouse || !(quantity >= 1) || !(delaySeconds >= 1)) {
      return res.status(400).json({ error: "Scheduled restock values are invalid" });
    }
    const restock = await ScheduledRestock.create({ itemId: item._id, warehouseId: warehouse._id,
      quantity, dueAt: new Date(Date.now() + delaySeconds * 1000) });
    changed();
    res.status(201).json({ restock });
  };

  const cancelScheduledRestock = async (req: Request, res: express.Response) => {
    const restock = await ScheduledRestock.findOneAndUpdate({ _id: req.params.id, status: "pending" },
      { $set: { status: "cancelled" } }, { new: true });
    if (!restock) return res.status(404).json({ error: "Pending restock not found" });
    changed();
    res.json({ restock });
  };
  router.post("/scheduled-restocks", auth, staff, scheduleRestock);
  router.delete("/scheduled-restocks/:id", auth, staff, cancelScheduledRestock);
  adminRouter.post("/scheduled-restocks", auth, staff, scheduleRestock);
  adminRouter.delete("/scheduled-restocks/:id", auth, staff, cancelScheduledRestock);

  router.post("/reorder-rules", auth, staff, async (req: Request, res) => {
    const item = await Item.findById(req.body?.itemId);
    const threshold = number(req.body?.threshold);
    const quantity = number(req.body?.quantity);
    if (!item || !(threshold >= 0) || !(quantity >= 1)) {
      return res.status(400).json({ error: "Reorder rule values are invalid" });
    }
    const rule = await ReorderRule.findOneAndUpdate({ itemId: item._id },
      { itemId: item._id, threshold, quantity }, { upsert: true, new: true });
    await recordActivity(req.progressionUser, "updated reorder rule", item.name);
    changed();
    res.json({ rule });
  });

  router.post("/recommendations/:itemId/dismiss", auth, async (req: Request, res) => {
    await Dismissal.updateOne({ userId: req.progressionUser._id, itemId: req.params.itemId },
      { $setOnInsert: { userId: req.progressionUser._id, itemId: req.params.itemId } },
      { upsert: true });
    changed(String(req.progressionUser._id));
    res.json({ dismissed: true });
  });

  router.post("/cart/restore", auth, async (req: Request, res) => {
    const archive = await CartArchive.findOne({ userId: req.progressionUser._id });
    if (!archive) return res.status(404).json({ error: "No expired cart is available" });
    const restored: any[] = [];
    const unavailable: string[] = [];
    for (const archived of archive.items) {
      const item = await Item.findById(archived.itemId);
      const total = await Stock.aggregate([{ $match: { item_id: archived.itemId } },
        { $group: { _id: null, total: { $sum: "$quantity" } } }]);
      const quantity = Math.min(Number(archived.quantity), total[0]?.total || 0);
      if (!item || quantity < 1) unavailable.push(item?.name || "Unavailable item");
      else restored.push({ itemId: archived.itemId, quantity,
        reservationExpiresAt: new Date(Date.now() + 90_000) });
    }
    await Cart.updateOne({ userId: req.progressionUser._id }, { $set: { items: restored } },
      { upsert: true });
    await CartArchive.deleteOne({ _id: archive._id });
    changed(String(req.progressionUser._id));
    res.json({ restored: restored.length, unavailable });
  });

  app.use("/api/progression", router);
  app.use("/api/support", supportRouter);
  app.use("/api/admin", adminRouter);

  async function processTimers() {
    const now = new Date();
    const due = await ScheduledRestock.find({ status: "pending", dueAt: { $lte: now } });
    for (const restock of due) {
      const claimed = await ScheduledRestock.findOneAndUpdate({ _id: restock._id, status: "pending" },
        { $set: { status: "applied" } }, { new: true });
      if (!claimed) continue;
      await Stock.updateOne({ item_id: claimed.itemId, warehouse_id: claimed.warehouseId },
        { $inc: { quantity: claimed.quantity } });
      await StockLedger.updateOne({ restockId: claimed._id }, { $setOnInsert: {
        restockId: claimed._id, itemId: claimed.itemId, warehouseId: claimed.warehouseId,
        quantity: claimed.quantity,
      } }, { upsert: true });
    }

    const shipped = await Order.find({ status: "shipped", updatedAt: { $lte: new Date(Date.now() - 60_000) } });
    for (const order of shipped) {
      order.status = "delivered";
      await order.save();
      const preference = await Preference.findOne({ userId: order.userId });
      if (preference?.order !== false) {
        await Notification.updateOne({ userId: order.userId, key: `delivery:${order._id}` },
          { $setOnInsert: { userId: order.userId, key: `delivery:${order._id}`, type: "delivery",
            message: `Delivered ${order.items.map(item => item.name).join(", ")}` } }, { upsert: true });
      }
      const ownerOrders = await Order.find({ userId: order.userId }).sort({ createdAt: -1 });
      io.to(`user:${order.userId}`).emit("orders:update", ownerOrders.map(value => value.toJSON()));
    }

    const expiredCarts = await Cart.find({ items: { $ne: [] },
      updatedAt: { $lte: new Date(Date.now() - 300_000) } });
    for (const cart of expiredCarts) {
      const expired = cart.items.map(item => ({ itemId: item.itemId, quantity: item.quantity }));
      if (!expired.length) continue;
      await CartArchive.updateOne({ userId: cart.userId },
        { $set: { userId: cart.userId, items: expired } },
        { upsert: true });
      cart.items = [] as any;
      await cart.save();
    }

    const rules = await ReorderRule.find();
    for (const rule of rules) {
      const total = await Stock.aggregate([{ $match: { item_id: rule.itemId } },
        { $group: { _id: null, total: { $sum: "$quantity" } } }]);
      if ((total[0]?.total || 0) > rule.threshold) continue;
      const pending = await ScheduledRestock.exists({ itemId: rule.itemId, status: "pending" });
      if (!pending) {
        const warehouse = await Warehouse.findOne().sort({ name: 1 });
        if (warehouse) await ScheduledRestock.create({ itemId: rule.itemId,
          warehouseId: warehouse._id, quantity: rule.quantity,
          dueAt: new Date(Date.now() + 60_000), source: "automatic" });
      }
    }

    const alerts = await StockAlert.find({ sent: false });
    for (const alert of alerts) {
      const total = await Stock.aggregate([{ $match: { item_id: alert.itemId } },
        { $group: { _id: null, total: { $sum: "$quantity" } } }]);
      if ((total[0]?.total || 0) < 1) continue;
      const preference = await Preference.findOne({ userId: alert.userId });
      if (preference?.stock !== false) {
        const item = await Item.findById(alert.itemId);
        await Notification.updateOne({ userId: alert.userId, key: `stock:${alert.itemId}` },
          { $setOnInsert: { userId: alert.userId, key: `stock:${alert.itemId}`, type: "stock",
            message: `${item?.name || "Item"} is back in stock` } }, { upsert: true });
      }
      alert.sent = true;
      await alert.save();
    }
    if (due.length || shipped.length || expiredCarts.length) changed();
  }

  const timer = setInterval(() => processTimers().catch(error =>
    console.error("progression timer failed", error)), 1000);
  timer.unref();
}
