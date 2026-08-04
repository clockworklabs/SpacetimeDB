import { schema, table, t } from 'spacetimedb/server';

const user = table({
  name: 'user',
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
  active: t.bool(),
});

const user_stats = table({
  name: 'user_stats',
}, {
  key: t.string().primaryKey(),
  count: t.u64(),
});

const spacetimedb = schema({ user, user_stats });
export default spacetimedb;

export const compute_user_counts = spacetimedb.reducer(
  (ctx) => {
    let total = 0n;
    let active = 0n;
    for (const u of ctx.db.user.iter()) {
      total += 1n;
      if (u.active) {
        active += 1n;
      }
    }

    ctx.db.user_stats.insert({ key: 'total', count: total });
    ctx.db.user_stats.insert({ key: 'active', count: active });
  }
);
