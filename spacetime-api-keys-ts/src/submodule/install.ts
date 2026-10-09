import { ScheduleAt } from 'spacetimedb';
import type { ReducerModuleCtx } from './schema.js';

const SWEEP_INTERVAL_MICROS = 60n * 1_000_000n;

/** Makes the caller the first API key administrator and schedules usage cleanup. */
export function install(ctx: ReducerModuleCtx) {
  if (ctx.db.apiKeyAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.apiKeyAdminIdentity.insert({
      identity: ctx.sender,
      addedAt: ctx.timestamp,
    });
  }
  if (ctx.db.apiKeySweepTick.count() === 0n) {
    ctx.db.apiKeySweepTick.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(SWEEP_INTERVAL_MICROS),
    });
  }
}
