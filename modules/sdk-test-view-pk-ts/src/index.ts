import { schema, t, table } from 'spacetimedb/server';

const ViewPkPlayer = t.row('ViewPkPlayer', {
  id: t.u64().primaryKey(),
  name: t.string(),
});

const ViewPkMembership = t.row('ViewPkMembership', {
  id: t.u64().primaryKey(),
  player_id: t.u64().index('btree'),
});

const ViewPkMembershipSecondary = t.row('ViewPkMembershipSecondary', {
  id: t.u64().primaryKey(),
  player_id: t.u64().index('btree'),
});

const view_pk_player = table({ public: true }, ViewPkPlayer);
const view_pk_membership = table({ public: true }, ViewPkMembership);
const view_pk_membership_secondary = table(
  { public: true },
  ViewPkMembershipSecondary
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
  { public: true },
  t.array(view_pk_player.rowType),
  ctx => ctx.from.view_pk_player
);

export const sender_view_pk_players_a = spacetimedb.view(
  { public: true },
  t.array(view_pk_player.rowType),
  ctx =>
    ctx.from.view_pk_membership.rightSemijoin(
      ctx.from.view_pk_player,
      (membership, player) => membership.player_id.eq(player.id)
    )
);

export const sender_view_pk_players_b = spacetimedb.view(
  { public: true },
  t.array(view_pk_player.rowType),
  ctx =>
    ctx.from.view_pk_membership_secondary.rightSemijoin(
      ctx.from.view_pk_player,
      (membership, player) => membership.player_id.eq(player.id)
    )
);
