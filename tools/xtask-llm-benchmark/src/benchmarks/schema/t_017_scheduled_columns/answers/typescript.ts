import { ScheduleAt } from 'spacetimedb';
import { table, schema, t } from 'spacetimedb/server';

export const TickTimer = table({
  name: 'tickTimer',
  scheduled: 'tick',
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
});

const spacetimedb = schema(TickTimer);

spacetimedb.reducer('tick', { schedule: TickTimer.rowType }, (ctx, { schedule }) => {
});

spacetimedb.init(ctx => {
  ctx.db.tickTimer.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(50_000n),
  });
});
