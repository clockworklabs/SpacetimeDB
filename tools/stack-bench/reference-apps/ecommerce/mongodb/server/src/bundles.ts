import type { Express, RequestHandler } from 'express';
import mongoose, { Types, type ClientSession } from 'mongoose';
import { Cart, Item, Order, Stock } from './models.js';
import { refundCredit } from './credit.js';
import { reserveStock } from './stock-reservations.js';

type Allocation = { itemId: Types.ObjectId; warehouseId: Types.ObjectId; quantity: number };

export async function releaseBundle(allocations: readonly Allocation[], session?: ClientSession) {
  for (const row of allocations) await Stock.updateOne(
    { item_id: row.itemId, warehouse_id: row.warehouseId }, { $inc: { quantity: row.quantity } }, session ? { session } : {});
}

export async function expireBundleReservations() {
  const carts = await Cart.find({ items: { $elemMatch: { reservationExpiresAt: { $lte: new Date() }, 'componentAllocations.0': { $exists: true } } } });
  for (const candidate of carts) await mongoose.connection.transaction(async session => {
    const cart = await Cart.findById(candidate._id).session(session);
    if (!cart) return;
    for (const line of cart.items) {
      if (!line.reservationExpiresAt || line.reservationExpiresAt > new Date() || !line.componentAllocations.length) continue;
      await releaseBundle(line.componentAllocations as Allocation[], session);
      line.componentAllocations = [] as any;
    }
    await cart.save({ session, timestamps: false });
  });
}

export function installBundleRoutes(app: Express, auth: RequestHandler,
  changed: (userId: string) => Promise<void>) {
  app.get('/api/bundles', async (_req, res) => {
    const rows = await Item.find({ 'bundleComponents.0': { $exists: true } });
    res.json(rows.map(row => ({ id: String(row._id), name: row.name, price: row.price,
      components: row.bundleComponents.map(value => ({ item: value.name, quantity: value.quantity })) })));
  });
  app.post('/api/bundles', auth, async (req, res) => {
    const actor = (req as any).user;
    if (!actor.isAdmin && !actor.roles?.includes('catalog')) return res.status(403).json({error:'Catalog access required'});
    const name = typeof req.body.name === 'string' ? req.body.name.trim() : '';
    const price = Number(req.body.price);
    let components: Array<{ item: string; quantity: number }>;
    try { components = JSON.parse(req.body.componentsJson); } catch { return res.status(400).json({ error: 'Invalid components' }); }
    if (!name || !Number.isFinite(price) || price <= 0 || !Array.isArray(components) || !components.length
      || components.some(value => !value || typeof value.item !== 'string' || !Number.isSafeInteger(value.quantity) || value.quantity < 1)
      || new Set(components.map(value => value.item)).size !== components.length) {
      return res.status(400).json({ error: 'Invalid bundle' });
    }
    const values = [];
    for (const component of components) {
      const item = await Item.findOne({ name: component.item, 'bundleComponents.0': { $exists: false } });
      if (!item) return res.status(400).json({ error: 'Unknown product' });
      values.push({ itemId: item._id, name: item.name, quantity: component.quantity });
    }
    const existing = await Item.findOne({ name });
    if (existing && !existing.bundleComponents.length) return res.status(409).json({ error: 'A product already uses this name' });
    try {
      const row = existing
        ? await Item.findOneAndUpdate({ _id: existing._id, 'bundleComponents.0': { $exists: true } }, { $set: { price, bundleComponents: values } }, { new: true })
        : await Item.create({ name, price, bundleComponents: values, category: 'Bundles' });
      if (!row) return res.status(409).json({error:'Bundle changed'});
      res.json({ id: String(row._id) });
    } catch(error) { res.status(409).json({error:'Bundle name already exists'}); }
  });
  app.post('/api/cart/bundles', auth, async (req, res) => {
    if (!Types.ObjectId.isValid(req.body.bundleId)) return res.status(400).json({ error: 'Invalid bundle' });
    const bundle = await Item.findById(req.body.bundleId);
    if (!bundle?.bundleComponents.length) return res.status(404).json({ error: 'Bundle not found' });
    const userId = String((req as any).user._id);
    try {
      await mongoose.connection.transaction(async session => {
        let cart = await Cart.findOne({ userId }).session(session);
        if (!cart) [cart] = await Cart.create([{ userId, items: [] }], { session });
        const existing = cart!.items.find(line => String(line.itemId) === String(bundle._id));
        if (existing && existing.reservationExpiresAt && existing.reservationExpiresAt > new Date()) throw new Error('Bundle already in cart');
        if (existing) await releaseBundle(existing.componentAllocations as Allocation[], session);
        cart!.items = cart!.items.filter(line => String(line.itemId) !== String(bundle._id)) as any;
        const allocations: Allocation[] = [];
        for (const component of [...bundle.bundleComponents].sort((a, b) => String(a.itemId).localeCompare(String(b.itemId)))) {
          const held = await reserveStock(component.itemId!, component.quantity!, session);
          if (!held) throw new Error('A component is unavailable');
          for (const warehouseId of held) allocations.push({ itemId: component.itemId!, warehouseId, quantity: 1 });
        }
        cart!.items.push({ itemId: bundle._id, quantity: 1, bundlePrice: bundle.price, bundleComponentsJson: JSON.stringify(bundle.bundleComponents), componentAllocations: allocations,
          reservationExpiresAt: new Date(Date.now() + 90_000), reservedWarehouseIds: [] } as any);
        cart!.inactiveExpiresAt = new Date(Date.now() + 300_000);
        await cart!.save({ session });
      });
    } catch (error) { return res.status(409).json({ error: String(error) }); }
    await changed(userId);
    res.json({ ok: true });
  });
  app.post('/api/bundle-orders/:orderId/return', auth, async (req, res) => {
    const userId = String((req as any).user._id);
    if (!Types.ObjectId.isValid(req.params.orderId)) return res.status(400).json({ error: 'Invalid order' });
    try {
      await mongoose.connection.transaction(async session => {
        const order = await Order.findOne({ _id: req.params.orderId, userId, status: { $in: ['shipped', 'delivered'] } }).session(session);
        const bundles = order?.items.filter(line => line.isBundle && !line.returned) ?? [];
        if (!order || !bundles.length) throw new Error('No returnable bundle on this account');
        if ((order.creditMinor || order.discount) && order.items.some(line => !line.isBundle)) throw new Error('Use a full support refund for mixed-item discounted or credit orders');
        if (order.creditMinor) await refundCredit(order, session);
        for (const line of bundles) { await releaseBundle(line.componentAllocations as Allocation[], session); line.returned = true; }
        order.refundTotal = Math.min(order.total, order.refundTotal + (order.discount ? order.total - order.refundTotal : bundles.reduce((sum, line) => sum + line.price * line.quantity, 0)));
        await order.save({ session });
      });
    } catch (error) { return res.status(403).json({ error: String(error) }); }
    await changed(userId);
    res.json({ ok: true });
  });
}
