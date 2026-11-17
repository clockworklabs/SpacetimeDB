import { schema, table, t, SenderError } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
const queuedWork = table(
  { name: 'queued_work', public: true },
  {
    id: t.u64().primaryKey(),
    input: t.i64(),
    status: t.string(),
    attempts: t.u64(),
  }
);
const workEffect = table(
  { name: 'work_effect', public: true },
  {
    id: t.u64().primaryKey(),
    value: t.i64(),
  }
);
const workTimer = table(
  { name: 'work_timer', scheduled: (): any => execute_work },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
    workId: t.u64(),
  }
);
const spacetimedb = schema({ queuedWork, workEffect, workTimer });
export default spacetimedb;

export const enqueue = spacetimedb.reducer(
  { id: t.u64(), input: t.i64() },
  (ctx, { id, input }) => {
    const old = ctx.db.queuedWork.id.find(id);
    if (old) {
      if (old.input === input) return;
      throw new SenderError('request conflict');
    }
    if (input < -1000000n || input > 1000000n)
      throw new SenderError('invalid input');
    ctx.db.queuedWork.insert({ id, input, status: 'queued', attempts: 0n });
    ctx.db.workTimer.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + 1000n),
      workId: id,
    });
  }
);

export const execute_work = spacetimedb.reducer(
  { job: workTimer.rowType },
  (ctx, { job }) => {
    if (!ctx.sender.isEqual(ctx.databaseIdentity))
      throw new SenderError('scheduler only');
    const row = ctx.db.queuedWork.id.find(job.workId);
    if (!row || row.status !== 'queued') return;
    if (row.input >= 0n)
      ctx.db.workEffect.insert({ id: row.id, value: row.input * 2n });
    ctx.db.queuedWork.id.update({
      ...row,
      attempts: row.attempts + 1n,
      status: row.input < 0n ? 'failed' : 'complete',
    });
  }
);
