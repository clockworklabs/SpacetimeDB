import { schema, table, t } from 'spacetimedb/server';

const player = table({
  name: 'player',
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
  nickname: t.option(t.string()),
  highScore: t.option(t.u32()),
});

const spacetimedb = schema({ player });
export default spacetimedb;

export const create_player = spacetimedb.reducer(
  { name: t.string(), nickname: t.option(t.string()), highScore: t.option(t.u32()) },
  (ctx, { name, nickname, highScore }) => {
    ctx.db.player.insert({
      id: 0n,
      name,
      nickname,
      highScore,
    });
  }
);
