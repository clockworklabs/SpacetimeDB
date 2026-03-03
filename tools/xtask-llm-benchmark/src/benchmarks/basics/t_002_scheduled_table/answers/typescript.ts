import { ScheduleAt } from 'spacetimedb';
import { table, schema, t } from 'spacetimedb/server';

const tickTimer = table({
  name: 'tickTimer',
  scheduled: (): any => tick,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
});

const spacetimedb = schema({ tickTimer });
export default spacetimedb;

export const tick = spacetimedb.reducer({ timer: tickTimer.rowType }, (ctx, { timer }) => {
});

export const init = spacetimedb.init(ctx => {
  ctx.db.tickTimer.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(50_000n),
  });
});
