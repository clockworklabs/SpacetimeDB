import { ScheduleAt } from 'spacetimedb';
import { schema, table, t } from 'spacetimedb/server';

const materializedState = table({ name: 'materialized_state', public: true }, {
  id: t.u64().primaryKey(), status: t.string(), version: t.u64(), refreshedAt: t.timestamp(),
});
const refreshJob = table({ name: 'refresh_job', scheduled: (): any => refreshMaterialized }, {
  scheduledId: t.u64().primaryKey().autoInc(), scheduledAt: t.scheduleAt(), stateId: t.u64(),
});
const spacetimedb = schema({ materializedState, refreshJob });
export default spacetimedb;

export const start_refresh = spacetimedb.reducer(ctx => {
  const pending = { id: 1n, status: 'pending', version: 0n, refreshedAt: ctx.timestamp };
  if (ctx.db.materializedState.id.find(1n)) ctx.db.materializedState.id.update(pending);
  else ctx.db.materializedState.insert(pending);
  ctx.db.refreshJob.insert({
    scheduledId: 0n, scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + 1_000n), stateId: 1n,
  });
});

export const refreshMaterialized = spacetimedb.reducer({ job: refreshJob.rowType }, (ctx, { job }) => {
  const state = ctx.db.materializedState.id.find(job.stateId);
  if (!state) throw new Error('materialized state missing');
  ctx.db.materializedState.id.update({ ...state, status: 'ready', version: 1n, refreshedAt: ctx.timestamp });
});
