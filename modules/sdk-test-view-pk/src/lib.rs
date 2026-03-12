use spacetimedb::{reducer, table, view, Query, ReducerContext, Table, ViewContext};

#[table(accessor = view_pk_player, public)]
pub struct ViewPkPlayer {
    #[primary_key]
    pub id: u64,
    pub name: String,
}

#[table(accessor = view_pk_membership, public)]
pub struct ViewPkMembership {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub player_id: u64,
}

#[table(accessor = view_pk_membership_secondary, public)]
pub struct ViewPkMembershipSecondary {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub player_id: u64,
}

#[reducer]
pub fn insert_view_pk_player(ctx: &ReducerContext, id: u64, name: String) {
    ctx.db.view_pk_player().insert(ViewPkPlayer { id, name });
}

#[reducer]
pub fn update_view_pk_player(ctx: &ReducerContext, id: u64, name: String) {
    ctx.db.view_pk_player().id().update(ViewPkPlayer { id, name });
}

#[reducer]
pub fn insert_view_pk_membership(ctx: &ReducerContext, id: u64, player_id: u64) {
    ctx.db.view_pk_membership().insert(ViewPkMembership { id, player_id });
}

#[reducer]
pub fn insert_view_pk_membership_secondary(ctx: &ReducerContext, id: u64, player_id: u64) {
    ctx.db
        .view_pk_membership_secondary()
        .insert(ViewPkMembershipSecondary { id, player_id });
}

#[view(accessor = all_view_pk_players, public)]
pub fn all_view_pk_players(ctx: &ViewContext) -> impl Query<ViewPkPlayer> {
    ctx.from.view_pk_player()
}

#[view(accessor = sender_view_pk_players_a, public)]
pub fn sender_view_pk_players_a(ctx: &ViewContext) -> impl Query<ViewPkPlayer> {
    ctx.from
        .view_pk_membership()
        .right_semijoin(ctx.from.view_pk_player(), |membership, player| {
            membership.player_id.eq(player.id)
        })
}

#[view(accessor = sender_view_pk_players_b, public)]
pub fn sender_view_pk_players_b(ctx: &ViewContext) -> impl Query<ViewPkPlayer> {
    ctx.from
        .view_pk_membership_secondary()
        .right_semijoin(ctx.from.view_pk_player(), |membership, player| {
            membership.player_id.eq(player.id)
        })
}
