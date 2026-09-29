import { ScheduleAt } from 'spacetimedb';
import type { ReducerModuleCtx } from './schema.js';
import { WEBHOOK_PRUNE_INTERVAL_MICROS } from './limits.js';

/** Call from the host's init reducer to seed its publishing identity and the webhook retention sweep. */
export function install(ctx: ReducerModuleCtx) {
  if (ctx.db.stripeAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.stripeAdminIdentity.insert({
      identity: ctx.sender,
      addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
  if (ctx.db.stripeWebhookPruneTick.count() === 0n) {
    ctx.db.stripeWebhookPruneTick.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(WEBHOOK_PRUNE_INTERVAL_MICROS),
    });
  }
}
