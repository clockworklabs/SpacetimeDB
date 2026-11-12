import { ScheduleAt } from 'spacetimedb';
import type { ReducerModuleCtx } from './schema.js';

const SWEEP_INTERVAL_MICROS = 15n * 1_000_000n;

/** Seeds the default config, makes the caller the first lobby administrator, and schedules cleanup. */
export function install(ctx: ReducerModuleCtx) {
  if (ctx.db.lobbyConfig.singleton.find(true) == null) {
    ctx.db.lobbyConfig.insert({
      singleton: true,
      defaultTicketTtlSeconds: 60,
      maxMatchSize: 16,
      readyTimeoutSeconds: 120,
      retentionSeconds: 60 * 60,
      updatedAt: ctx.timestamp,
    });
  }
  if (ctx.db.lobbyAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.lobbyAdminIdentity.insert({
      identity: ctx.sender,
      addedAt: ctx.timestamp,
    });
  }
  if (ctx.db.lobbySweepTick.count() === 0n) {
    ctx.db.lobbySweepTick.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(SWEEP_INTERVAL_MICROS),
    });
  }
}
