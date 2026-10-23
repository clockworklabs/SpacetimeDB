import { Timestamp } from 'spacetimedb';
import { Range, schema, table, t } from 'spacetimedb/server';

const event = table({ name: 'event', public: true }, {
  id: t.u64().primaryKey(),
  occurredAt: t.timestamp().index('btree'),
  label: t.string(),
});

const spacetimedb = schema({ event });
export default spacetimedb;

export const seed = spacetimedb.reducer(ctx => {
  [100n, 200n, 300n, 400n, 500n].forEach((micros, index) => {
    ctx.db.event.insert({ id: BigInt(index + 1), occurredAt: new Timestamp(micros), label: `event-${micros}` });
  });
});

export const window_event = spacetimedb.anonymousView(
  { name: 'window_event', public: true },
  t.array(event.rowType),
  ctx => Array.from(ctx.db.event.occurredAt.filter(new Range(
    { tag: 'included', value: new Timestamp(200n) },
    { tag: 'included', value: new Timestamp(400n) },
  )))
);
