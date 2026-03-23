import { schema, table, t } from 'spacetimedb/server';

const player = table({
  name: 'player',
  public: true,
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
  score: t.u32(),
});

const spacetimedb = schema({ player });
export default spacetimedb;

export const all_players = spacetimedb.anonymousView(
  { name: 'all_players', public: true },
  t.array(player.rowType),
  (ctx) => {
    return Array.from(ctx.db.player.iter());
  }
);
