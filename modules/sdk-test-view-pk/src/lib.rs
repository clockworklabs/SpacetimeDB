use spacetimedb::{reducer, table, view, AnonymousViewContext, Query, ReducerContext, Table, ViewContext};

#[table(accessor = view_pk_player, public)]
pub struct ViewPkPlayer {
    #[primary_key]
    pub id: u64,
    pub name: String,
}

#[table(accessor = view_pk_player_scan)]
pub struct ViewPkPlayerScan {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub scan_id: u64,
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
    ctx.db
        .view_pk_player_scan()
        .insert(ViewPkPlayerScan { id, scan_id: id });
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

#[view(accessor = procedural_all_view_pk_players, public, primary_key(columns = [id]))]
pub fn procedural_all_view_pk_players(ctx: &AnonymousViewContext) -> Vec<ViewPkPlayer> {
    ctx.db
        .view_pk_player_scan()
        .scan_id()
        .filter(0u64..)
        .filter_map(|scan| ctx.db.view_pk_player().id().find(scan.id))
        .collect()
}

#[view(accessor = procedural_sender_view_pk_players_a, public, primary_key(columns = [id]))]
pub fn procedural_sender_view_pk_players_a(ctx: &ViewContext) -> Vec<ViewPkPlayer> {
    ctx.db
        .view_pk_player_scan()
        .scan_id()
        .filter(0u64..)
        .filter(|scan| ctx.db.view_pk_membership().player_id().filter(scan.id).next().is_some())
        .filter_map(|scan| ctx.db.view_pk_player().id().find(scan.id))
        .collect()
}

#[view(accessor = procedural_sender_view_pk_players_b, public, primary_key(columns = [id]))]
pub fn procedural_sender_view_pk_players_b(ctx: &ViewContext) -> Vec<ViewPkPlayer> {
    ctx.db
        .view_pk_player_scan()
        .scan_id()
        .filter(0u64..)
        .filter(|scan| {
            ctx.db
                .view_pk_membership_secondary()
                .player_id()
                .filter(scan.id)
                .next()
                .is_some()
        })
        .filter_map(|scan| ctx.db.view_pk_player().id().find(scan.id))
        .collect()
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
