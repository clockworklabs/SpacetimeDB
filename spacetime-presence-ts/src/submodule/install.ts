import { ScheduleAt } from 'spacetimedb';
import type { InferSchema, ReducerCtx } from 'spacetimedb/server';
import { installPresenceConfig } from '../presence.js';
import type spacetimedb from './index.js';

const SWEEP_INTERVAL_MICROS = 10n * 1_000_000n;

type InstallCtx = ReducerCtx<InferSchema<typeof spacetimedb>>;

/** Makes the caller the first presence administrator, seeds config, and schedules expiry sweeps. */
export function install(ctx: InstallCtx) {
  if (ctx.db.presenceAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.presenceAdminIdentity.insert({
      identity: ctx.sender,
      addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
  installPresenceConfig(ctx);
  if (ctx.db.presenceSweepTick.count() === 0n) {
    ctx.db.presenceSweepTick.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(SWEEP_INTERVAL_MICROS),
    });
  }
}
