import { ScheduleAt } from 'spacetimedb';
import { schema, table, t } from 'spacetimedb/server';

const jobResult = table({ name: 'job_result', public: true }, {
  id: t.u64().primaryKey(), status: t.string(),
});
const privateJob = table({ name: 'private_job', scheduled: (): any => runPrivateJob }, {
  scheduledId: t.u64().primaryKey().autoInc(), scheduledAt: t.scheduleAt(), resultId: t.u64(),
});
const spacetimedb = schema({ jobResult, privateJob });
export default spacetimedb;

export const enqueue_private = spacetimedb.reducer({ id: t.u64() }, (ctx, { id }) => {
  ctx.db.jobResult.insert({ id, status: 'queued' });
  ctx.db.privateJob.insert({
    scheduledId: 0n, scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + 1_000n), resultId: id,
  });
});

export const runPrivateJob = spacetimedb.reducer({ job: privateJob.rowType }, (ctx, { job }) => {
  const result = ctx.db.jobResult.id.find(job.resultId);
  if (!result) throw new Error('job result missing');
  ctx.db.jobResult.id.update({ ...result, status: 'complete' });
});
