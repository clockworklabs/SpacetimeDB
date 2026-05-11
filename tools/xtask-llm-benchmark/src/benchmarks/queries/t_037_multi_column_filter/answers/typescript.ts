import { schema, table, t } from 'spacetimedb/server';

const eventLog = table({
  name: 'event_log',
  indexes: [{ name: 'byCategorySeverity', algorithm: 'btree', columns: ['category', 'severity'] }],
}, {
  id: t.u64().primaryKey().autoInc(),
  category: t.string(),
  severity: t.u32(),
  message: t.string(),
});

const filteredEvent = table({
  name: 'filtered_event',
}, {
  eventId: t.u64().primaryKey(),
  message: t.string(),
});

const spacetimedb = schema({ eventLog, filteredEvent });
export default spacetimedb;

export const filter_events = spacetimedb.reducer(
  { category: t.string(), severity: t.u32() },
  (ctx, { category, severity }) => {
    for (const e of ctx.db.eventLog.iter()) {
      if (e.category === category && e.severity === severity) {
        ctx.db.filteredEvent.insert({
          eventId: e.id,
          message: e.message,
        });
      }
    }
  }
);
