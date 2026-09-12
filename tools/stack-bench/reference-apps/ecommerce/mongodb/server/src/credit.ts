import type { Express, RequestHandler } from 'express';
import mongoose, { Schema, type ClientSession } from 'mongoose';
import { Cart, Item, Order, User } from './models.js';
import { Payment, Promotion } from './progression-models.js';

const creditEntrySchema = new Schema({ userId: Schema.Types.ObjectId, reference: String, amountMinor: Number });
creditEntrySchema.index({ userId: 1, reference: 1 }, { unique: true });
export const CreditEntry = mongoose.model('CreditEntry', creditEntrySchema);

export async function refundCredit(order: any, session: ClientSession, refundedTotal = order.total) {
  if (!order.creditMinor) return;
  const reference = `refund:${order._id}`;
  const entry = await CreditEntry.findOne({ userId: order.userId, reference }).session(session);
  const amountMinor = Math.min(order.creditMinor, Math.round(order.creditMinor * refundedTotal / order.total));
  const delta = amountMinor - (entry?.amountMinor ?? 0);
  if (delta <= 0) return;
  await User.updateOne({ _id: order.userId }, { $inc: { creditMinor: delta } }, { session });
  await CreditEntry.updateOne({ userId: order.userId, reference }, { $set: { amountMinor } }, { upsert: true, session });
}

export async function checkoutAtomic(userId: string, useCredit = false) {
  return mongoose.connection.transaction(async session => {
    const cart = await Cart.findOne({ userId }).session(session);
    if (!cart?.items.length) throw new Error('Cart is empty');
    const now = new Date();
    const active = cart.items.filter(line => !line.reservationExpiresAt || line.reservationExpiresAt > now);
    if (!active.length) throw new Error('Reservation expired');
    const lines: any[] = [];
    for (const line of active) {
      const item = await Item.findById(line.itemId).session(session);
      if (!item) throw new Error('Product not found');
      const isBundle = line.bundlePrice !== null;
      if (isBundle ? !line.componentAllocations.length : line.reservedWarehouseIds.length !== line.quantity) throw new Error('Reservation incomplete');
      const allocations = new Map<string, { warehouseId: any; quantity: number }>();
      for (const warehouseId of line.reservedWarehouseIds) {
        const key = String(warehouseId);
        const current = allocations.get(key);
        if (current) current.quantity += 1;
        else allocations.set(key, { warehouseId, quantity: 1 });
      }
      lines.push({ itemId: item._id, name: item.name, price: line.bundlePrice ?? item.price, quantity: line.quantity,
        allocations: [...allocations.values()], isBundle, componentAllocations: [...line.componentAllocations] });
    }
    const subtotal = lines.reduce((sum, line) => sum + line.price * line.quantity, 0);
    const discount = Math.round(subtotal * Number(cart.discount || 0)) / 100;
    const totalMinor = Math.round((subtotal - discount) * 100);
    if (!Number.isSafeInteger(totalMinor) || totalMinor < 0) throw new Error('Invalid payment total');
    if (cart.promotionCode) {
      const promotion = await Promotion.findOneAndUpdate({ code: cart.promotionCode, start: { $lte: new Date() }, end: { $gte: new Date() },
        $expr: { $lt: ['$redemptions', '$limit'] } }, { $inc: { redemptions: 1, revenue: totalMinor / 100 } }, { session });
      if (!promotion) throw new Error('Promotion unavailable');
    }
    const user = await User.findById(userId).session(session);
    if (!user) throw new Error('Account not found');
    const creditMinor = useCredit ? Math.min(user.creditMinor, totalMinor) : 0;
    const [order] = await Order.create([{ userId, items: lines, total: totalMinor / 100, discount,
      creditMinor, externalMinor: totalMinor - creditMinor }], { session });
    if (creditMinor) {
      user.creditMinor -= creditMinor;
      await user.save({ session });
      await CreditEntry.create([{ userId, reference: `order:${order!._id}`, amountMinor: -creditMinor }], { session });
    }
    await Payment.create([{ userId, orderId: order!._id, amount: totalMinor / 100, status: 'paid' }], { session });
    cart.items = cart.items.filter(line => line.reservationExpiresAt && line.reservationExpiresAt <= now) as any;
    cart.discount = 0; cart.promotionCode = '';
    await cart.save({ session });
    return order;
  });
}

export function registerCredit(app: Express, auth: RequestHandler, staff: RequestHandler,
  changed: (userId: string) => Promise<void>) {
  app.get('/api/credit', auth, async (req, res) => {
    const user = await User.findById((req as any).user._id);
    if (!user) return res.status(401).json({ error: 'Account not found' });
    const entries = await CreditEntry.find({ userId: user._id }).sort({ _id: 1 });
    const customers = user.isAdmin || user.isStaff ? await User.find({ isAdmin: false, isStaff: false }).select('_id username') : [];
    res.json({ accountId: String(user._id), balance: user.creditMinor / 100,
      entries: entries.map(row => ({ id: String(row._id), reference: row.reference, amount: Number(row.amountMinor) / 100 })),
      customers: customers.map(row => ({ id: String(row._id), name: row.username })) });
  });
  app.post('/api/staff/credit', auth, staff, async (req, res) => {
    const { accountId, amountMinor, reference } = req.body;
    if (!mongoose.isValidObjectId(accountId) || !Number.isSafeInteger(amountMinor) || amountMinor <= 0
      || typeof reference !== 'string' || !reference.trim()) return res.status(400).json({ error: 'Invalid credit grant' });
    try {
      await mongoose.connection.transaction(async session => {
        const existing = await CreditEntry.findOne({ userId: accountId, reference }).session(session);
        if (existing) {
          if (existing.amountMinor !== amountMinor) throw new Error('Reference already identifies another grant');
          return;
        }
        const target = await User.findById(accountId).session(session);
        if (!target || !Number.isSafeInteger(target.creditMinor + amountMinor)) throw new Error('Invalid customer balance');
        target.creditMinor += amountMinor; await target.save({ session });
        await CreditEntry.create([{ userId: accountId, reference, amountMinor }], { session });
      });
      res.json({ ok: true });
    } catch (error) { res.status(409).json({ error: String(error) }); }
  });
  app.post('/api/checkout/credit', auth, async (req, res) => {
    const userId = String((req as any).user._id);
    try { const order = await checkoutAtomic(userId, true); await changed(userId); res.json({ order }); }
    catch (error) { res.status(409).json({ error: String(error) }); }
  });
}
