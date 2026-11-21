import type { Express, RequestHandler } from 'express';
import mongoose, { Schema, Types } from 'mongoose';
import { Item, Order } from './models.js';
import { Payment } from './progression-models.js';
import { reserveStock } from './stock-reservations.js';

const Subscription = mongoose.model('PurchaseSubscription', new Schema({
  userId: { type: Schema.Types.ObjectId, required: true },
  itemId: { type: Schema.Types.ObjectId, required: true },
  item: { type: String, required: true }, price: { type: Number, required: true },
  quantity: { type: Number, required: true }, intervalSeconds: { type: Number, required: true },
  slots: { type: Number, required: true },
  dueAt: { type: Date, required: true }, pausedAt: { type: Date, default: null },
  status: { type: String, default: 'active' },
  deliveries: { type: [{ status: String, orderId: Schema.Types.ObjectId }], default: [] },
}));

export function installSubscriptionRoutes(app: Express, auth: RequestHandler) {
  app.get('/api/subscriptions', auth, async (req, res, next) => {
    try {
      const rows = await Subscription.find({ userId: (req as any).user._id }).sort({ _id: 1 });
      const values = [];
      for (const row of rows) {
        const payments = await Payment.find({ orderId: { $in: row.deliveries.map(slot => slot.orderId).filter(Boolean) } });
        values.push({ id: String(row._id), item: row.item, status: row.status,
          total: payments.reduce((sum, payment) => sum + payment.amount, 0),
          deliveries: row.deliveries.map(slot => ({ status: slot.status })) });
      }
      res.json(values);
    } catch (error) { next(error); }
  });
  app.post('/api/subscriptions', auth, async (req, res, next) => {
    const { item, quantity, intervalSeconds, deliveries } = req.body;
    if (typeof item !== 'string' || !Number.isSafeInteger(quantity) || quantity < 1 || quantity > 1000
      || !Number.isSafeInteger(intervalSeconds) || intervalSeconds < 30 || intervalSeconds > 31536000
      || !Number.isSafeInteger(deliveries) || deliveries < 1 || deliveries > 12) {
      return res.status(400).json({ error: 'Invalid subscription' });
    }
    try {
      const product = await Item.findOne({ name: item });
      if (!product) return res.status(400).json({ error: 'Unknown item' });
      if (product.bundleComponents.length) return res.status(400).json({ error: 'Choose an individual item, not a bundle' });
      const created = await Subscription.create({ userId: (req as any).user._id, itemId: product._id,
        item: product.name, price: product.price, quantity, intervalSeconds, slots: deliveries,
        dueAt: new Date(Date.now() + intervalSeconds * 1000) });
      res.json({ id: String(created._id) });
    } catch (error) { next(error); }
  });
  for (const action of ['pause', 'resume', 'cancel'] as const) {
    app.post(`/api/subscriptions/:id/${action}`, auth, async (req, res, next) => {
      if (!Types.ObjectId.isValid(String(req.params.id))) return res.status(400).json({ error: 'Invalid subscription' });
      try {
        const allowed = await mongoose.connection.transaction(async session => {
          const row = await Subscription.findById(req.params.id).session(session);
          if (!row || String(row.userId) !== String((req as any).user._id)) return false;
          if (action === 'pause' && row.status === 'active') {
            row.status = 'paused'; row.pausedAt = new Date();
          } else if (action === 'resume' && row.status === 'paused') {
            row.dueAt = new Date(row.dueAt.getTime() + Date.now() - row.pausedAt!.getTime());
            row.status = 'active'; row.pausedAt = null;
          } else if (action === 'cancel' && ['active', 'paused'].includes(row.status)) {
            row.status = 'cancelled'; row.pausedAt = null;
          }
          await row.save({ session });
          return true;
        });
        if (!allowed) return res.status(403).json({ error: 'Subscription access denied' });
        res.json({ ok: true });
      } catch (error) { next(error); }
    });
  }
}

export async function processSubscriptions(changed: (userId: string) => Promise<void>) {
  for (let count = 0; count < 12; count += 1) {
    const userId = await mongoose.connection.transaction(async session => {
      const row = await Subscription.findOne({ status: 'active', dueAt: { $lte: new Date() } })
        .sort({ dueAt: 1, _id: 1 }).session(session);
      if (!row) return null;
      const allocation = await reserveStock(row.itemId, row.quantity, session);
      let orderId: Types.ObjectId | undefined;
      if (allocation) {
        const total = Math.round(row.price * 100) * row.quantity / 100;
        const [order] = await Order.create([{ userId: row.userId, total, creditMinor: 0,
          externalMinor: Math.round(total * 100), status: 'pending', items: [{
          itemId: row.itemId, name: row.item, quantity: row.quantity, price: row.price,
          allocations: allocation.map(warehouseId => ({ warehouseId, quantity: 1 })),
        }] }], { session });
        orderId = order!._id;
        await Payment.create([{ userId: row.userId, orderId, amount: total, status: 'paid' }], { session });
      }
      row.deliveries.push({ status: orderId ? 'paid' : 'skipped', orderId });
      row.dueAt = new Date(row.dueAt.getTime() + row.intervalSeconds * 1000);
      if (row.deliveries.length === row.slots) row.status = 'complete';
      await row.save({ session });
      return String(row.userId);
    });
    if (userId === null) return;
    await changed(userId);
  }
}
