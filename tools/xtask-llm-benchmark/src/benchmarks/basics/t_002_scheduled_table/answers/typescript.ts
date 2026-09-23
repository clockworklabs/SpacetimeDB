import { ScheduleAt } from 'spacetimedb';
import { table, schema, t } from 'spacetimedb/server';

const tick_timer = table({
  name: 'tick_timer',
  scheduled: (): any => tick,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
});

const spacetimedb = schema({ tick_timer });
export default spacetimedb;

export const tick = spacetimedb.reducer({ timer: tick_timer.rowType }, (ctx, { timer }) => {
});

export const init = spacetimedb.init(ctx => {
  ctx.db.tick_timer.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(50_000n),
  });
});
