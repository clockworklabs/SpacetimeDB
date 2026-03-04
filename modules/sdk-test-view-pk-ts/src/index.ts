import { schema, t, table } from 'spacetimedb/server';

const viewPkPlayer = table(
  { name: 'view_pk_player', public: true },
  {
    id: t.u64().primaryKey(),
    name: t.string(),
  }
);

const viewPkMembership = table(
  { name: 'view_pk_membership', public: true },
  {
    id: t.u64().primaryKey(),
    player_id: t.u64().index('btree'),
  }
);

const spacetimedb = schema({ viewPkPlayer, viewPkMembership });
export default spacetimedb;

export const all_view_pk_players = spacetimedb.view(
  { public: true },
  t.query(viewPkPlayer.rowType),
  ctx => ctx.from.viewPkPlayer
);

export const insert_view_pk_player = spacetimedb.reducer(
  { id: t.u64(), name: t.string() },
  (ctx, { id, name }) => {
    ctx.db.viewPkPlayer.insert({ id, name });
  }
);

export const update_view_pk_player = spacetimedb.reducer(
  { id: t.u64(), name: t.string() },
  (ctx, { id, name }) => {
    const old = ctx.db.viewPkPlayer.id.find(id);
    if (old !== undefined) {
      ctx.db.viewPkPlayer.id.delete(id);
    }
    ctx.db.viewPkPlayer.insert({ id, name });
  }
);

export const insert_view_pk_membership = spacetimedb.reducer(
  { id: t.u64(), player_id: t.u64() },
  (ctx, { id, player_id }) => {
    ctx.db.viewPkMembership.insert({ id, player_id });
  }
);
