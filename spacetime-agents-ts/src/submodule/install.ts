import { ScheduleAt } from 'spacetimedb';
import type { AgentsTx } from './index.js';

const SWEEPER_INTERVAL_MICROS = 60_000_000n;

/** Makes the caller the first Agents administrator and schedules stale-lock cleanup. */
export function install(ctx: AgentsTx): void {
  if (ctx.db.agentAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.agentAdminIdentity.insert({
      identity: ctx.sender,
      addedAt: ctx.timestamp,
    });
  }
  ctx.db.threadLockSweeperTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(SWEEPER_INTERVAL_MICROS),
  });
}
