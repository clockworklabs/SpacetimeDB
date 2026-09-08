import { ScheduleAt } from 'spacetimedb';
import type { InferSchema, ReducerCtx } from 'spacetimedb/server';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import type spacetimedb from './index';

const ONE_SECOND_MICROS = 1_000_000n;

type Schema = InferSchema<typeof spacetimedb>;
type InstallCtx = ReducerCtx<Schema>;

export function installAuth(ctx: InstallCtx) {
  rateLimit.installRateLimit(ctx.as.rateLimit);

  if (ctx.db.authAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.authAdminIdentity.insert({
      identity: ctx.sender,
      addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
  if (ctx.db.authSweeperTick.count() === 0n) {
    ctx.db.authSweeperTick.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.interval(60n * ONE_SECOND_MICROS),
    });
  }
}
