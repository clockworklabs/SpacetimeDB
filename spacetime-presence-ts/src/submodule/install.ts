import { ScheduleAt } from 'spacetimedb';
import type { InferSchema, ReducerCtx } from 'spacetimedb/server';
import {
  DEFAULT_PRESENCE_SWEEP_BATCH,
  DEFAULT_PRESENCE_TTL_SECONDS,
  installPresenceConfig,
} from '../index';
import type spacetimedb from './index';

const ONE_SECOND_MICROS = 1_000_000n;
const SWEEP_INTERVAL_SECONDS = 10n;

type Schema = InferSchema<typeof spacetimedb>;
type InstallCtx = ReducerCtx<Schema>;

export function installPresence(ctx: InstallCtx) {
  if (ctx.db.presenceAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.presenceAdminIdentity.insert({
      identity: ctx.sender,
      addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
  installPresenceConfig(ctx, {
    defaultTtlSeconds: DEFAULT_PRESENCE_TTL_SECONDS,
    sweepBatch: DEFAULT_PRESENCE_SWEEP_BATCH,
  });
  ctx.db.presenceSweepTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(
      SWEEP_INTERVAL_SECONDS * ONE_SECOND_MICROS
    ),
  });
}
