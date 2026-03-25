import { ScheduleAt } from 'spacetimedb';
import { schema, table, t } from 'spacetimedb/server';

const reminder = table({
  name: 'reminder',
  scheduled: (): any => sendReminder,
}, {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  message: t.string(),
});

const spacetimedb = schema({ reminder });
export default spacetimedb;

export const sendReminder = spacetimedb.reducer(
  { timer: reminder.rowType },
  (_ctx, _args) => {}
);

export const init = spacetimedb.init(ctx => {
  const fireAt = ctx.timestamp.microsSinceUnixEpoch + 60_000_000n;
  ctx.db.reminder.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.time(fireAt),
    message: 'Hello!',
  });
});
