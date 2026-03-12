import { schema, t, table } from 'spacetimedb/server';

const view_pk_player = table(
  { name: 'view_pk_player', public: true },
  {
    id: t.u64().primaryKey(),
    name: t.string(),
  }
);

const view_pk_membership = table(
  { name: 'view_pk_membership', public: true },
  {
    id: t.u64().primaryKey(),
    player_id: t.u64().index('btree'),
  }
);

const view_pk_membership_secondary = table(
  { name: 'view_pk_membership_secondary', public: true },
  {
    id: t.u64().primaryKey(),
    player_id: t.u64().index('btree'),
  }
);

const spacetimedb = schema({
  view_pk_player,
  view_pk_membership,
  view_pk_membership_secondary,
});
export default spacetimedb;

export const insert_view_pk_player = spacetimedb.reducer(
  { id: t.u64(), name: t.string() },
  (ctx, { id, name }) => {
    ctx.db.view_pk_player.insert({ id, name });
  }
);

export const update_view_pk_player = spacetimedb.reducer(
  { id: t.u64(), name: t.string() },
  (ctx, { id, name }) => {
    ctx.db.view_pk_player.id.update({ id, name });
  }
);

export const insert_view_pk_membership = spacetimedb.reducer(
  { id: t.u64(), player_id: t.u64() },
  (ctx, { id, player_id }) => {
    ctx.db.view_pk_membership.insert({ id, player_id });
  }
);

export const insert_view_pk_membership_secondary = spacetimedb.reducer(
  { id: t.u64(), player_id: t.u64() },
  (ctx, { id, player_id }) => {
    ctx.db.view_pk_membership_secondary.insert({ id, player_id });
  }
);

export const all_view_pk_players = spacetimedb.view(
  { name: 'all_view_pk_players', public: true },
  t.query(view_pk_player.rowType),
  ctx => {
    return ctx.from.view_pk_player.build();
  }
);

export const sender_view_pk_players_a = spacetimedb.view(
  { name: 'sender_view_pk_players_a', public: true },
  t.query(view_pk_player.rowType),
  ctx => {
    return ctx.from.view_pk_membership
      .rightSemijoin(ctx.from.view_pk_player, (membership, player) =>
        membership.player_id.eq(player.id)
      )
      .build();
  }
);

export const sender_view_pk_players_b = spacetimedb.view(
  { name: 'sender_view_pk_players_b', public: true },
  t.query(view_pk_player.rowType),
  ctx => {
    return ctx.from.view_pk_membership_secondary
      .rightSemijoin(ctx.from.view_pk_player, (membership, player) =>
        membership.player_id.eq(player.id)
      )
      .build();
  }
);
