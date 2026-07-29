import { ScheduleAt } from 'spacetimedb';
import { schema, table, t } from 'spacetimedb/server';

const cleanup_job = table({
  name: 'cleanup_job',
  scheduled: (): any => runCleanup,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
});

const spacetimedb = schema({ cleanup_job });
export default spacetimedb;

export const runCleanup = spacetimedb.reducer(
  { timer: cleanup_job.rowType },
  (_ctx, _args) => {}
);

export const init = spacetimedb.init(ctx => {
  ctx.db.cleanup_job.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(60_000_000n),
  });
});

export const cancelCleanup = spacetimedb.reducer(
  { scheduledId: t.u64() },
  (ctx, { scheduledId }) => {
    ctx.db.cleanup_job.scheduledId.delete(scheduledId);
  }
);
