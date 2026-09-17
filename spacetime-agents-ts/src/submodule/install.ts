import { ScheduleAt } from 'spacetimedb';
import type { InferSchema, ReducerCtx } from 'spacetimedb/server';
import type spacetimedb from './index';

const ONE_SECOND_MICROS = 1_000_000n;
const SWEEPER_INTERVAL_MICROS = 60n * ONE_SECOND_MICROS;

type Schema = InferSchema<typeof spacetimedb>;
type InstallCtx = ReducerCtx<Schema>;

export function installAgents(ctx: InstallCtx) {
  if (ctx.db.agentAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.agentAdminIdentity.insert({
      identity: ctx.sender,
      addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
  ctx.db.threadLockSweeperTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(SWEEPER_INTERVAL_MICROS),
  });
}
