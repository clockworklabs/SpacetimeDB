import { schema, table, t } from 'spacetimedb/server';

const announcement = table({
  name: 'announcement',
  public: true,
}, {
  id: t.u64().primaryKey().autoInc(),
  message: t.string(),
  active: t.bool().index('btree'),
});

const spacetimedb = schema({ announcement });
export default spacetimedb;

export const activeAnnouncements = spacetimedb.anonymousView(
  { name: 'active_announcements', public: true },
  t.array(announcement.rowType),
  (ctx) => {
    return Array.from(ctx.db.announcement.active.filter(true));
  }
);
