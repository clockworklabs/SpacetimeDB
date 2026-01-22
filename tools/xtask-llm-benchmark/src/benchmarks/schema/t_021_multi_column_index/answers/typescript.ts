import { table, schema, t } from 'spacetimedb/server';

export const Log = table({
  name: 'log',
  indexes: [{ name: 'byUserDay', algorithm: 'btree', columns: ['userId', 'day'] }],
}, {
  id: t.i32().primaryKey(),
  userId: t.i32(),
  day: t.i32(),
  message: t.string(),
});

const spacetimedb = schema(Log);

spacetimedb.reducer('seed', {},
  ctx => {
    ctx.db.log.insert({ id: 1, userId: 7, day: 1, message: "a" });
    ctx.db.log.insert({ id: 2, userId: 7, day: 2, message: "b" });
    ctx.db.log.insert({ id: 3, userId: 9, day: 1, message: "c" });
  }
);
