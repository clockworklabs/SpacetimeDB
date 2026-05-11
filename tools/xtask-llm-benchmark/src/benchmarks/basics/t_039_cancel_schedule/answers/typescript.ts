import { ScheduleAt } from 'spacetimedb';
import { schema, table, t } from 'spacetimedb/server';

const cleanupJob = table({
  name: 'cleanup_job',
  scheduled: (): any => runCleanup,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
});

const spacetimedb = schema({ cleanupJob });
export default spacetimedb;

export const runCleanup = spacetimedb.reducer(
  { timer: cleanupJob.rowType },
  (_ctx, _args) => {}
);

export const init = spacetimedb.init(ctx => {
  ctx.db.cleanupJob.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(60_000_000n),
  });
});

export const cancelCleanup = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    ctx.db.cleanupJob.scheduledId.delete(scheduledId);
  }
);
