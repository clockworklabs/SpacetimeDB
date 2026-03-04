use spacetimedb::{reducer, table, view, Identity, Query, ReducerContext, SpacetimeType, Table, ViewContext};

#[table(accessor = player, public)]
pub struct Player {
    #[primary_key]
    #[auto_inc]
    id: u32,
    user_id: Identity,
    name: String,
    location: Point,
}

#[derive(SpacetimeType)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

#[table(accessor = user, public)]
pub struct User {
    #[primary_key]
    pub identity: Identity,
    pub username: String,
}

#[table(accessor = unindexed_player, public)]
pub struct UnindexedPlayer {
    #[primary_key]
    #[auto_inc]
    id: u32,
    owner_id: Identity,
    name: String,
    location: Point,
}

#[table(accessor = view_pk_player, public)]
pub struct ViewPkPlayer {
    #[primary_key]
    id: u64,
    name: String,
}

#[table(accessor = view_pk_membership, public)]
pub struct ViewPkMembership {
    #[primary_key]
    id: u64,
    #[index(btree)]
    player_id: u64,
}

#[reducer]
pub fn create_player(ctx: &ReducerContext, name: String, location: Point) {
    ctx.db.user().insert(User {
        identity: ctx.sender(),
        username: name.clone(),
    });
    ctx.db.player().insert(Player {
        id: 0,
        user_id: ctx.sender(),
        name,
        location,
    });
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

#[view(accessor = all_view_pk_players, public)]
pub fn all_view_pk_players(ctx: &ViewContext) -> impl Query<ViewPkPlayer> {
    ctx.from.view_pk_player()
}
