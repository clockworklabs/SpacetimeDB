import { ScheduleAt } from 'spacetimedb';
import { schema, table, t } from 'spacetimedb/server';
import type { InferSchema, ReducerCtx } from 'spacetimedb/server';

const workItem = table(
  { name: 'work_item', public: true },
  { id: t.u64().primaryKey(), groupId: t.u64().index('btree') }
);
const deleteJob = table(
  { name: 'delete_job', scheduled: (): any => runDeleteBatch },
  { scheduledId: t.u64().primaryKey().autoInc(), scheduledAt: t.scheduleAt(), groupId: t.u64() }
);
const spacetimedb = schema({ workItem, deleteJob });
export default spacetimedb;
type Ctx = ReducerCtx<InferSchema<typeof spacetimedb>>;

function enqueue(ctx: Ctx, groupId: bigint) {
  ctx.db.deleteJob.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(ctx.timestamp.microsSinceUnixEpoch + 1_000n),
    groupId,
  });
}

export const seed_group = spacetimedb.reducer({ groupId: t.u64(), count: t.u32() }, (ctx, { groupId, count }) => {
  for (let offset = 0; offset < count; offset++) ctx.db.workItem.insert({ id: groupId * 100n + BigInt(offset), groupId });
});
export const request_delete = spacetimedb.reducer({ groupId: t.u64() }, (ctx, { groupId }) => enqueue(ctx, groupId));
export const runDeleteBatch = spacetimedb.reducer({ timer: deleteJob.rowType }, (ctx, { timer }) => {
  let deleted = 0;
  for (const row of ctx.db.workItem.groupId.filter(timer.groupId)) {
    if (deleted === 2) break;
    ctx.db.workItem.id.delete(row.id);
    deleted++;
  }
  if (ctx.db.workItem.groupId.filter(timer.groupId)[Symbol.iterator]().next().done === false) enqueue(ctx, timer.groupId);
});
