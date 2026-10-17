import { SenderError, type InferSchema, type ReducerCtx } from 'spacetimedb/server';
import spacetimedb from './schema';

type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;
const SECOND = 1_000_000n;

export function createSubscription(ctx: Ctx, accountId: bigint, input: {
  item: string; quantity: number; intervalSeconds: number; deliveries: number;
}) {
  if (!Number.isInteger(input.quantity) || input.quantity < 1 || input.quantity > 1000
    || !Number.isInteger(input.intervalSeconds) || input.intervalSeconds < 30 || input.intervalSeconds > 31536000
    || !Number.isInteger(input.deliveries) || input.deliveries < 1 || input.deliveries > 12) {
    throw new SenderError('Invalid subscription.');
  }
  const item = [...ctx.db.item.iter()].find(item => item.name === input.item);
  if (!item) throw new SenderError('Unknown item.');
  if (ctx.db.bundleDefinition.itemId.find(item.id)) throw new SenderError('Choose an individual item, not a bundle.');
  ctx.db.purchaseSubscription.insert({ id: 0n, accountId, itemId: item.id, quantity: input.quantity,
    price: item.price, intervalSeconds: input.intervalSeconds, slots: input.deliveries, processed: 0,
    dueMicros: ctx.timestamp.microsSinceUnixEpoch + BigInt(input.intervalSeconds) * SECOND,
    pausedMicros: undefined, status: 'active' });
}

export function changeSubscription(ctx: Ctx, accountId: bigint, id: bigint, action: 'pause' | 'resume' | 'cancel') {
  const row = ctx.db.purchaseSubscription.id.find(id);
  if (!row || row.accountId !== accountId) throw new SenderError('Subscription access denied.');
  const now = ctx.timestamp.microsSinceUnixEpoch;
  if (action === 'pause' && row.status === 'active') {
    ctx.db.purchaseSubscription.id.update({ ...row, status: 'paused', pausedMicros: now });
  } else if (action === 'resume' && row.status === 'paused') {
    ctx.db.purchaseSubscription.id.update({ ...row, status: 'active', pausedMicros: undefined,
      dueMicros: row.dueMicros + now - row.pausedMicros! });
  } else if (action === 'cancel' && ['active', 'paused'].includes(row.status)) {
    ctx.db.purchaseSubscription.id.update({ ...row, status: 'cancelled', pausedMicros: undefined });
  }
}

export function processSubscriptions(ctx: Ctx,
  purchase: (accountId: bigint, itemId: bigint, quantity: number, price: number) => bigint | null) {
  const now = ctx.timestamp.microsSinceUnixEpoch;
  for (const original of ctx.db.purchaseSubscription.iter()) {
    let row = original;
    while (row.status === 'active' && row.dueMicros <= now) {
      const orderId = purchase(row.accountId, row.itemId, row.quantity, row.price);
      ctx.db.subscriptionDelivery.insert({ id: 0n, subscriptionId: row.id, slot: row.processed + 1,
        status: orderId === null ? 'skipped' : 'paid', orderId: orderId ?? undefined });
      row = { ...row, processed: row.processed + 1,
        status: row.processed + 1 === row.slots ? 'complete' : 'active',
        dueMicros: row.dueMicros + BigInt(row.intervalSeconds) * SECOND };
      ctx.db.purchaseSubscription.id.update(row);
    }
  }
}
