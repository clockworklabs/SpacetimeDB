import "dotenv/config";
import express from "express";
import cors from "cors";
import http from "http";
import { Server as SocketIOServer } from "socket.io";
import mongoose, { Types } from "mongoose";
import bcrypt from "bcryptjs";
import jwt from "jsonwebtoken";
import { Item, Warehouse, Stock, User, Cart, Order, Review } from "./models.js";

const PORT = Number(process.env.PORT) || 6401;
const DATABASE_URL = process.env.DATABASE_URL;
const JWT_SECRET = process.env.JWT_SECRET;
if (!DATABASE_URL) throw new Error("DATABASE_URL is required");
if (!JWT_SECRET || JWT_SECRET.length < 32) throw new Error("JWT_SECRET must be at least 32 characters");

const app = express();
app.use(cors());
app.use(express.json());

const server = http.createServer(app);
const io = new SocketIOServer(server, {
  cors: { origin: "*" },
});

// ---------------------------------------------------------------------------
// Seeding
// ---------------------------------------------------------------------------

const CATALOGUE: Array<{ name: string; price: number; east: number; west: number; description: string }> = [
  { name: "Air Purifier", price: 189.0, east: 60, west: 40, description: "HEPA filtration for cleaner indoor air." },
  { name: "Bluetooth Speaker", price: 79.5, east: 50, west: 50, description: "Portable speaker with rich, room-filling sound." },
  { name: "Coffee Grinder", price: 64.0, east: 70, west: 30, description: "Burr grinder for consistent, fresh grounds." },
  { name: "Desk Lamp", price: 42.0, east: 55, west: 45, description: "Adjustable LED lamp for any desk setup." },
  { name: "Espresso Machine", price: 449.0, east: 80, west: 20, description: "Café-quality espresso at home." },
  { name: "Gaming Mouse", price: 59.0, east: 50, west: 50, description: "Precision optical mouse built for gaming." },
  { name: "Headphones", price: 199.0, east: 60, west: 40, description: "Over-ear headphones with active noise cancelling." },
  { name: "Induction Cooktop", price: 329.0, east: 50, west: 50, description: "Fast, efficient induction cooking surface." },
  { name: "Keyboard", price: 89.0, east: 70, west: 30, description: "Mechanical keyboard with tactile switches." },
  { name: "Laptop Stand", price: 29.0, east: 90, west: 10, description: "Ergonomic aluminum stand for laptops." },
  { name: "Mirrorless Camera", price: 1299.0, east: 2, west: 1, description: "Compact mirrorless camera for enthusiasts." },
  { name: "Webcam", price: 69.0, east: 60, west: 40, description: "1080p webcam for calls and streaming." },
];

async function seed() {
  const itemCount = await Item.countDocuments();
  if (itemCount > 0) {
    console.log("Catalogue already seeded, skipping.");
  } else {
    console.log("Seeding catalogue...");
    const east = await Warehouse.create({ name: "East" });
    const west = await Warehouse.create({ name: "West" });
    for (const entry of CATALOGUE) {
      const item = await Item.create({ name: entry.name, price: entry.price, description: entry.description });
      await Stock.create({ item_id: item._id, warehouse_id: east._id, quantity: entry.east });
      await Stock.create({ item_id: item._id, warehouse_id: west._id, quantity: entry.west });
    }
    console.log("Catalogue seeded.");
  }

  const admin = await User.findOne({ username: "admin" });
  if (!admin) {
    console.log("Seeding admin account...");
    const passwordHash = await bcrypt.hash("stackbench-admin-2026", 10);
    const adminUser = await User.create({ username: "admin", passwordHash, isAdmin: true });
    await Cart.create({ userId: adminUser._id, items: [] });
  }
}

// ---------------------------------------------------------------------------
// Aggregation helpers — always computed live from the source-of-truth
// collections so external writes (ERP sync, warehouse scanner, nightly stock
// corrections) are reflected the moment anyone asks, with no cache to go stale.
// ---------------------------------------------------------------------------

async function getStockByItem(): Promise<Map<string, number>> {
  const rows = await Stock.aggregate([{ $group: { _id: "$item_id", total: { $sum: "$quantity" } } }]);
  const map = new Map<string, number>();
  for (const row of rows) map.set(row._id.toString(), row.total);
  return map;
}

async function getPurchaseCounts(): Promise<Map<string, number>> {
  const rows = await Order.aggregate([
    { $unwind: "$items" },
    { $group: { _id: "$items.itemId", total: { $sum: "$items.quantity" } } },
  ]);
  const map = new Map<string, number>();
  for (const row of rows) map.set(row._id.toString(), row.total);
  return map;
}

async function getItemsLive() {
  const [items, stockMap, purchaseMap] = await Promise.all([Item.find(), getStockByItem(), getPurchaseCounts()]);
  const enriched = items.map((it) => {
    const json = it.toJSON();
    const id = it._id.toString();
    return {
      ...json,
      id,
      stock: stockMap.get(id) || 0,
      purchaseCount: purchaseMap.get(id) || 0,
    };
  });
  enriched.sort((a, b) => {
    if (b.purchaseCount !== a.purchaseCount) return b.purchaseCount - a.purchaseCount;
    return a.name.localeCompare(b.name);
  });
  return enriched;
}

async function getItemDetail(itemId: string) {
  const item = await Item.findById(itemId);
  if (!item) return null;
  const [stockMap, reviews] = await Promise.all([
    getStockByItem(),
    Review.find({ itemId }).sort({ createdAt: -1 }),
  ]);
  const average = reviews.length ? reviews.reduce((s, r) => s + r.rating, 0) / reviews.length : 0;
  return {
    ...item.toJSON(),
    stock: stockMap.get(itemId) || 0,
    reviews: reviews.map((r) => r.toJSON()),
    average,
  };
}

async function getAdminOverview() {
  const [items, warehouses, stockRows, stockMap, revenueAgg] = await Promise.all([
    Item.find(),
    Warehouse.find(),
    Stock.find(),
    getStockByItem(),
    Order.aggregate([{ $group: { _id: null, total: { $sum: "$total" } } }]),
  ]);
  const itemMap = new Map(items.map((i) => [i._id.toString(), i]));
  const warehouseMap = new Map(warehouses.map((w) => [w._id.toString(), w]));
  const itemsOut = items.map((it) => {
    const json = it.toJSON();
    const id = it._id.toString();
    return { ...json, id, stock: stockMap.get(id) || 0 };
  });
  const locations = stockRows.map((row) => {
    const itemId = row.item_id.toString();
    const warehouseId = row.warehouse_id.toString();
    return {
      id: row._id.toString(),
      itemId,
      itemName: itemMap.get(itemId)?.name || "Unknown",
      warehouseId,
      warehouseName: warehouseMap.get(warehouseId)?.name || "Unknown",
      quantity: row.quantity,
    };
  });
  const revenue = revenueAgg[0]?.total || 0;
  return {
    items: itemsOut,
    warehouses: warehouses.map((w) => w.toJSON()),
    locations,
    revenue,
  };
}

async function getCartLive(userId: string) {
  let cart = await Cart.findOne({ userId });
  if (!cart) cart = await Cart.create({ userId, items: [] });
  const stockMap = await getStockByItem();
  const itemIds = cart.items.map((l) => l.itemId);
  const items = await Item.find({ _id: { $in: itemIds } });
  const itemMap = new Map(items.map((i) => [i._id.toString(), i]));
  const lines = cart.items.map((line) => {
    const itemId = line.itemId.toString();
    const item = itemMap.get(itemId);
    return {
      itemId,
      name: item?.name || "Unknown item",
      price: item?.price || 0,
      stock: stockMap.get(itemId) || 0,
      quantity: line.quantity,
    };
  });
  const total = lines.reduce((s, l) => s + l.price * l.quantity, 0);
  return { items: lines, total };
}

// ---------------------------------------------------------------------------
// Broadcasting
// ---------------------------------------------------------------------------

let lastItemsSnapshot = "";
let lastAdminSnapshot = "";

async function broadcastItems() {
  const items = await getItemsLive();
  const snapshot = JSON.stringify(items);
  if (snapshot !== lastItemsSnapshot) {
    lastItemsSnapshot = snapshot;
    io.emit("items:update", items);
  }
}

async function broadcastAdmin() {
  const overview = await getAdminOverview();
  const snapshot = JSON.stringify(overview);
  if (snapshot !== lastAdminSnapshot) {
    lastAdminSnapshot = snapshot;
    io.to("admins").emit("admin:update", overview);
  }
}

async function broadcastReviews(itemId: string) {
  const reviews = await Review.find({ itemId }).sort({ createdAt: -1 });
  const average = reviews.length ? reviews.reduce((s, r) => s + r.rating, 0) / reviews.length : 0;
  io.emit("reviews:update", { itemId, reviews: reviews.map((r) => r.toJSON()), average });
}

async function broadcastCart(userId: string) {
  const cart = await getCartLive(userId);
  io.to(`user:${userId}`).emit("cart:update", cart);
}

// Safety net for data written outside the app (ERP sync, warehouse scanner,
// nightly corrections) and for pages left open across a server restart: since
// every read above is a live aggregation with no cache, a short poll simply
// notices when the underlying numbers differ from what clients were last told
// and pushes the correction — no change-stream/replica-set dependency needed.
setInterval(() => {
  broadcastItems().catch((err) => console.error("broadcastItems poll failed", err));
  broadcastAdmin().catch((err) => console.error("broadcastAdmin poll failed", err));
}, 1500);

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

function signToken(userId: string) {
  return jwt.sign({ sub: userId }, JWT_SECRET, { expiresIn: "30d" });
}

async function userFromToken(token: string | undefined) {
  if (!token) return null;
  try {
    const payload = jwt.verify(token, JWT_SECRET) as { sub: string };
    return await User.findById(payload.sub);
  } catch {
    return null;
  }
}

function extractToken(req: express.Request): string | undefined {
  const header = req.headers.authorization;
  if (header && header.startsWith("Bearer ")) return header.slice(7);
  return undefined;
}

async function requireAuth(req: express.Request, res: express.Response, next: express.NextFunction) {
  const user = await userFromToken(extractToken(req));
  if (!user) return res.status(401).json({ error: "Sign in required" });
  (req as any).user = user;
  next();
}

async function requireAdmin(req: express.Request, res: express.Response, next: express.NextFunction) {
  const user = (req as any).user;
  if (!user || !user.isAdmin) return res.status(403).json({ error: "Admin access required" });
  next();
}

function publicUser(user: any) {
  return { id: user._id.toString(), username: user.username, isAdmin: user.isAdmin };
}

// ---------------------------------------------------------------------------
// Auth routes
// ---------------------------------------------------------------------------

app.post("/api/auth/signup", async (req, res) => {
  const { username, password } = req.body || {};
  if (typeof username !== "string" || typeof password !== "string" || !username.trim() || !password) {
    return res.status(400).json({ error: "Username and password are required" });
  }
  const existing = await User.findOne({ username: username.trim() });
  if (existing) return res.status(409).json({ error: "Username is already taken" });
  const passwordHash = await bcrypt.hash(password, 10);
  try {
    const user = await User.create({ username: username.trim(), passwordHash, isAdmin: false });
    await Cart.create({ userId: user._id, items: [] });
    const token = signToken(user._id.toString());
    res.json({ token, user: publicUser(user) });
  } catch (err: any) {
    if (err?.code === 11000) return res.status(409).json({ error: "Username is already taken" });
    throw err;
  }
});

app.post("/api/auth/signin", async (req, res) => {
  const { username, password } = req.body || {};
  if (typeof username !== "string" || typeof password !== "string") {
    return res.status(400).json({ error: "Username and password are required" });
  }
  const user = await User.findOne({ username: username.trim() });
  if (!user) return res.status(401).json({ error: "Invalid username or password" });
  const valid = await bcrypt.compare(password, user.passwordHash);
  if (!valid) return res.status(401).json({ error: "Invalid username or password" });
  const token = signToken(user._id.toString());
  res.json({ token, user: publicUser(user) });
});

app.get("/api/auth/me", requireAuth, async (req, res) => {
  res.json({ user: publicUser((req as any).user) });
});

// ---------------------------------------------------------------------------
// Item routes
// ---------------------------------------------------------------------------

app.get("/api/items", async (_req, res) => {
  const items = await getItemsLive();
  res.json({ items });
});

app.get("/api/items/:id", async (req, res) => {
  if (!Types.ObjectId.isValid(req.params.id)) return res.status(404).json({ error: "Item not found" });
  const detail = await getItemDetail(req.params.id);
  if (!detail) return res.status(404).json({ error: "Item not found" });
  res.json({ item: detail });
});

app.post("/api/items/:id/buy", requireAuth, async (req, res) => {
  const itemId = req.params.id;
  if (!Types.ObjectId.isValid(itemId)) return res.status(404).json({ error: "Item not found" });
  const item = await Item.findById(itemId);
  if (!item) return res.status(404).json({ error: "Item not found" });

  const decremented = await Stock.findOneAndUpdate(
    { item_id: item._id, quantity: { $gte: 1 } },
    { $inc: { quantity: -1 } },
    { sort: { quantity: -1 } }
  );
  if (!decremented) return res.status(400).json({ error: "Item is out of stock" });

  const user = (req as any).user;
  const order = await Order.create({
    userId: user._id,
    items: [{ itemId: item._id, name: item.name, price: item.price, quantity: 1 }],
    total: item.price,
  });

  await Promise.all([broadcastItems(), broadcastAdmin()]);
  res.json({ order: order.toJSON() });
});

// ---------------------------------------------------------------------------
// Cart routes
// ---------------------------------------------------------------------------

app.get("/api/cart", requireAuth, async (req, res) => {
  const cart = await getCartLive((req as any).user._id.toString());
  res.json(cart);
});

// Adds `qty` to an existing line or pushes a new one, entirely via atomic
// Mongo updates (no read-modify-write) so concurrent requests for the same
// item from different tabs/clients can never both observe "no line yet" and
// both push a duplicate line — MongoDB serializes per-document writes, so at
// most one of the two update filters below can match at a time.
async function addToCart(userId: string, itemId: Types.ObjectId, qty: number) {
  try {
    await Cart.updateOne({ userId }, { $setOnInsert: { userId, items: [] } }, { upsert: true });
  } catch (err: any) {
    if (err?.code !== 11000) throw err;
  }

  for (let attempt = 0; attempt < 10; attempt++) {
    const incResult = await Cart.updateOne(
      { userId, "items.itemId": itemId },
      { $inc: { "items.$.quantity": qty } }
    );
    if (incResult.matchedCount > 0) return;

    const pushResult = await Cart.updateOne(
      { userId, "items.itemId": { $ne: itemId } },
      { $push: { items: { itemId, quantity: qty } } }
    );
    if (pushResult.matchedCount > 0) return;
  }
  throw new Error("Failed to update cart line after repeated concurrent conflicts");
}

app.post("/api/cart", requireAuth, async (req, res) => {
  const { itemId, quantity } = req.body || {};
  const qty = quantity === undefined ? 1 : Number(quantity);
  if (!Types.ObjectId.isValid(itemId)) return res.status(400).json({ error: "Invalid item" });
  if (!Number.isInteger(qty) || qty < 1) return res.status(400).json({ error: "Quantity must be at least 1" });
  const item = await Item.findById(itemId);
  if (!item) return res.status(404).json({ error: "Item not found" });

  const userId = (req as any).user._id.toString();
  await addToCart(userId, item._id, qty);

  await broadcastCart(userId);
  res.json(await getCartLive(userId));
});

app.patch("/api/cart/:itemId", requireAuth, async (req, res) => {
  const { itemId } = req.params;
  const { quantity } = req.body || {};
  const qty = Number(quantity);
  if (!Number.isInteger(qty) || qty < 1) {
    return res.status(400).json({ error: "Quantity must be at least 1" });
  }
  const userId = (req as any).user._id.toString();
  const cart = await Cart.findOne({ userId });
  const line = cart?.items.find((l) => l.itemId.toString() === itemId);
  if (!cart || !line) return res.status(404).json({ error: "Item is not in your cart" });
  line.quantity = qty;
  await cart.save();

  await broadcastCart(userId);
  res.json(await getCartLive(userId));
});

app.delete("/api/cart/:itemId", requireAuth, async (req, res) => {
  const { itemId } = req.params;
  const userId = (req as any).user._id.toString();
  const cart = await Cart.findOne({ userId });
  if (cart) {
    cart.items = cart.items.filter((l) => l.itemId.toString() !== itemId) as any;
    await cart.save();
  }
  await broadcastCart(userId);
  res.json(await getCartLive(userId));
});

// ---------------------------------------------------------------------------
// Checkout
// ---------------------------------------------------------------------------

// Reserve `qty` units of an item across whatever warehouses have stock,
// one atomic unit at a time. Returns the list of warehouse ids the units were
// taken from (for compensating rollback) or null if the full quantity could
// not be reserved, having already rolled back its own partial progress.
async function reserveUnits(itemId: Types.ObjectId, qty: number): Promise<Types.ObjectId[] | null> {
  const taken: Types.ObjectId[] = [];
  for (let i = 0; i < qty; i++) {
    const doc = await Stock.findOneAndUpdate(
      { item_id: itemId, quantity: { $gte: 1 } },
      { $inc: { quantity: -1 } },
      { sort: { quantity: -1 } }
    );
    if (!doc) {
      await releaseUnits(itemId, taken);
      return null;
    }
    taken.push(doc.warehouse_id as any);
  }
  return taken;
}

async function releaseUnits(itemId: Types.ObjectId, warehouseIds: Types.ObjectId[]) {
  for (const warehouseId of warehouseIds) {
    await Stock.updateOne({ item_id: itemId, warehouse_id: warehouseId }, { $inc: { quantity: 1 } });
  }
}

app.post("/api/checkout", requireAuth, async (req, res) => {
  const userId = (req as any).user._id.toString();

  // Atomically claim the cart's current contents so a concurrent double-click
  // can't both see a full cart and both produce an order.
  const claimedCart = await Cart.findOneAndUpdate({ userId }, { $set: { items: [] } });
  const originalLines = claimedCart?.items || [];
  if (originalLines.length === 0) {
    return res.status(400).json({ error: "Your cart is empty" });
  }

  const items = await Item.find({ _id: { $in: originalLines.map((l) => l.itemId) } });
  const itemMap = new Map(items.map((i) => [i._id.toString(), i]));

  const reservations: Array<{ itemId: Types.ObjectId; warehouseIds: Types.ObjectId[] }> = [];
  let failure: string | null = null;
  const orderLines: Array<{ itemId: Types.ObjectId; name: string; price: number; quantity: number }> = [];

  for (const line of originalLines) {
    const item = itemMap.get(line.itemId.toString());
    if (!item) {
      failure = "An item in your cart no longer exists";
      break;
    }
    const warehouseIds = await reserveUnits(item._id, line.quantity);
    if (!warehouseIds) {
      failure = `Not enough stock of ${item.name}`;
      break;
    }
    reservations.push({ itemId: item._id, warehouseIds });
    orderLines.push({ itemId: item._id, name: item.name, price: item.price, quantity: line.quantity });
  }

  if (failure) {
    for (const r of reservations) await releaseUnits(r.itemId, r.warehouseIds);
    await Cart.updateOne({ userId }, { $set: { items: originalLines } });
    return res.status(400).json({ error: failure });
  }

  const total = orderLines.reduce((s, l) => s + l.price * l.quantity, 0);
  const order = await Order.create({ userId, items: orderLines, total });

  await Promise.all([broadcastItems(), broadcastAdmin(), broadcastCart(userId)]);
  res.json({ order: order.toJSON() });
});

// ---------------------------------------------------------------------------
// Orders
// ---------------------------------------------------------------------------

app.get("/api/orders", requireAuth, async (req, res) => {
  const orders = await Order.find({ userId: (req as any).user._id }).sort({ createdAt: -1 });
  res.json({ orders: orders.map((o) => o.toJSON()) });
});

// ---------------------------------------------------------------------------
// Reviews
// ---------------------------------------------------------------------------

app.post("/api/items/:id/reviews", requireAuth, async (req, res) => {
  const itemId = req.params.id;
  if (!Types.ObjectId.isValid(itemId)) return res.status(404).json({ error: "Item not found" });
  const { rating, comment } = req.body || {};
  const ratingNum = Number(rating);
  if (!Number.isInteger(ratingNum) || ratingNum < 1 || ratingNum > 5) {
    return res.status(400).json({ error: "Rating must be between 1 and 5" });
  }
  const user = (req as any).user;
  const hasPurchased = await Order.exists({ userId: user._id, "items.itemId": new Types.ObjectId(itemId) });
  if (!hasPurchased) {
    return res.status(403).json({ error: "You can only review items you have purchased" });
  }

  await Review.findOneAndUpdate(
    { itemId, userId: user._id },
    { itemId, userId: user._id, username: user.username, rating: ratingNum, comment: comment || "" },
    { upsert: true, new: true, setDefaultsOnInsert: true }
  );

  await broadcastReviews(itemId);
  const detail = await getItemDetail(itemId);
  res.json({ item: detail });
});

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

app.get("/api/admin/overview", requireAuth, requireAdmin, async (_req, res) => {
  res.json(await getAdminOverview());
});

app.post("/api/admin/restock", requireAuth, requireAdmin, async (req, res) => {
  const { itemId, warehouseId, quantity } = req.body || {};
  const qty = Number(quantity);
  if (!Types.ObjectId.isValid(itemId) || !Types.ObjectId.isValid(warehouseId)) {
    return res.status(400).json({ error: "Invalid item or warehouse" });
  }
  if (!Number.isInteger(qty) || qty < 1) {
    return res.status(400).json({ error: "Restock quantity must be a positive integer" });
  }
  const [item, warehouse] = await Promise.all([Item.findById(itemId), Warehouse.findById(warehouseId)]);
  if (!item || !warehouse) return res.status(404).json({ error: "Item or warehouse not found" });

  await Stock.findOneAndUpdate(
    { item_id: item._id, warehouse_id: warehouse._id },
    { $inc: { quantity: qty } },
    { upsert: true }
  );

  await Promise.all([broadcastItems(), broadcastAdmin()]);
  res.json(await getAdminOverview());
});

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

app.use((err: any, _req: express.Request, res: express.Response, _next: express.NextFunction) => {
  console.error(err);
  res.status(500).json({ error: "Internal server error" });
});

// ---------------------------------------------------------------------------
// Socket.io
// ---------------------------------------------------------------------------

io.on("connection", async (socket) => {
  const token = socket.handshake.auth?.token as string | undefined;
  const user = await userFromToken(token);
  if (user) {
    socket.join(`user:${user._id.toString()}`);
    if (user.isAdmin) socket.join("admins");
  }

  try {
    socket.emit("items:update", await getItemsLive());
    if (user) socket.emit("cart:update", await getCartLive(user._id.toString()));
    if (user?.isAdmin) socket.emit("admin:update", await getAdminOverview());
  } catch (err) {
    console.error("Failed to send initial snapshot", err);
  }
});

// ---------------------------------------------------------------------------
// Startup
// ---------------------------------------------------------------------------

async function main() {
  await mongoose.connect(DATABASE_URL);
  console.log("Connected to MongoDB");
  await seed();
  server.listen(PORT, () => {
    console.log(`API server listening on port ${PORT}`);
  });
}

main().catch((err) => {
  console.error("Fatal startup error", err);
  process.exit(1);
});
