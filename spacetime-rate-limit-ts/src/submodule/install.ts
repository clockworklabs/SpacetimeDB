import { ScheduleAt } from 'spacetimedb';
import { DEFAULT_SWEEP_BATCH } from '../limit';
import type { ReducerModuleCtx } from './schema';

const SWEEP_INTERVAL_MICROS = 30_000_000n;

/**
 * Call from the host's init reducer. Makes the publishing identity the first
 * administrator and schedules the expired-bucket sweep. Later calls do nothing.
 */
export function install(ctx: ReducerModuleCtx) {
  if (ctx.db.rateLimitConfig.singleton.find(true)) return;
  ctx.db.rateLimitAdminIdentity.insert({
    identity: ctx.sender,
    addedAt: ctx.timestamp,
  });
  ctx.db.rateLimitConfig.insert({
    singleton: true,
    sweepBatch: DEFAULT_SWEEP_BATCH,
    updatedAt: ctx.timestamp,
  });
  ctx.db.rateLimitSweepTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(SWEEP_INTERVAL_MICROS),
  });
}
