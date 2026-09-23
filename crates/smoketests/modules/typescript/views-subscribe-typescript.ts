import { schema, t, table } from "spacetimedb/server";

const playerState = table(
  { name: "player_state" },
  {
    identity: t.identity().primaryKey(),
    name: t.string().unique(),
    online: t.bool(),
  }
);

const spacetimedb = schema({ playerState });
export default spacetimedb;

export const my_player = spacetimedb.view(
  { public: true },
  t.option(playerState.rowType),
  ctx => ctx.db.playerState.identity.find(ctx.sender) ?? undefined
);

export const all_players = spacetimedb.anonymousView(
  { public: true },
  t.array(playerState.rowType),
  ctx => ctx.from.playerState
);

export const online_players = spacetimedb.anonymousView(
  { public: true },
  t.array(playerState.rowType),
  ctx => ctx.from.playerState.where(row => row.online)
);

export const insert_player_proc = spacetimedb.procedure(
  { name: t.string() },
  t.unit(),
  (ctx, { name }) => {
    const sender = ctx.sender;
    ctx.withTx(tx => {
      tx.db.playerState.insert({ name, identity: sender, online: true });
    });
    return {};
  }
);
