import { ScheduleAt } from 'spacetimedb';
import type { ReducerModuleCtx } from './schema.js';

const FLUSH_INTERVAL_MICROS = 5n * 1_000_000n;

/** Call from the host's init reducer to seed its publishing identity and start scheduled delivery. */
export function install(ctx: ReducerModuleCtx) {
  if (ctx.db.posthogAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.posthogAdminIdentity.insert({
      identity: ctx.sender,
      addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
  if (ctx.db.posthogFlushTick.count() === 0n) {
    ctx.db.posthogFlushTick.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(FLUSH_INTERVAL_MICROS),
    });
  }
}
