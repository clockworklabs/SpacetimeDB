import { schema, table, t } from 'spacetimedb/server';

const player = table({
  name: 'player',
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
  score: t.u64(),
});

const leaderboard = table({
  name: 'leaderboard',
}, {
  rank: t.u32().primaryKey(),
  playerName: t.string(),
  score: t.u64(),
});

const spacetimedb = schema({ player, leaderboard });
export default spacetimedb;

export const build_leaderboard = spacetimedb.reducer(
  { limit: t.u32() },
  (ctx, { limit }) => {
    const players = Array.from(ctx.db.player.iter());
    players.sort((a, b) => (b.score > a.score ? 1 : b.score < a.score ? -1 : 0));

    for (let i = 0; i < Math.min(limit, players.length); i++) {
      const p = players[i];
      ctx.db.leaderboard.insert({
        rank: i + 1,
        playerName: p.name,
        score: p.score,
      });
    }
  }
);
