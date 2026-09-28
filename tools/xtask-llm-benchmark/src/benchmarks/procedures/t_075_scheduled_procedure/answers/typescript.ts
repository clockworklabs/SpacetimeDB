import { ScheduleAt } from 'spacetimedb';
import { schema, table, t } from 'spacetimedb/server';

const procedureResult = table({ name: 'procedure_result', public: true }, {
  id: t.u64().primaryKey(), value: t.u32(),
});
const procedureJob = table({ name: 'procedure_job', scheduled: (): any => runScheduledProcedure }, {
  scheduledId: t.u64().primaryKey().autoInc(), scheduledAt: t.scheduleAt(),
  id: t.u64(), lhs: t.u32(), rhs: t.u32(),
});
const spacetimedb = schema({ procedureResult, procedureJob });
export default spacetimedb;

export const schedule_procedure = spacetimedb.reducer(
  { id: t.u64(), lhs: t.u32(), rhs: t.u32() },
  (ctx, { id, lhs, rhs }) => ctx.db.procedureJob.insert({
    scheduledId: 0n, scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + 1_000n), id, lhs, rhs,
  })
);

export const runScheduledProcedure = spacetimedb.procedure(
  { job: procedureJob.rowType }, t.unit(),
  (ctx, { job }) => {
    ctx.withTx(tx => tx.db.procedureResult.insert({ id: job.id, value: job.lhs + job.rhs }));
    return {};
  }
);
