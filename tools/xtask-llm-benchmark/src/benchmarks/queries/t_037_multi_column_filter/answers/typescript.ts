import { schema, table, t } from 'spacetimedb/server';

const event_log = table({
  name: 'event_log',
  indexes: [{ accessor: 'byCategorySeverity', algorithm: 'btree', columns: ['category', 'severity'] }],
}, {
  id: t.u64().primaryKey().autoInc(),
  category: t.string(),
  severity: t.u32(),
  message: t.string(),
});

const filtered_event = table({
  name: 'filtered_event',
}, {
  eventId: t.u64().primaryKey(),
  message: t.string(),
});

const spacetimedb = schema({ event_log, filtered_event });
export default spacetimedb;

export const filter_events = spacetimedb.reducer(
  { category: t.string(), severity: t.u32() },
  (ctx, { category, severity }) => {
    for (const e of ctx.db.event_log.iter()) {
      if (e.category === category && e.severity === severity) {
        ctx.db.filtered_event.insert({
          eventId: e.id,
          message: e.message,
        });
      }
    }
  }
);
