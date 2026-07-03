use spacetimedb::{reducer, table, view, Identity, ReducerContext, SpacetimeType, Table, ViewContext};

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
pub fn set_player_alias(ctx: &ReducerContext, name: String, alias: Option<String>) {
    ctx.db.user().insert(User {
        identity: ctx.sender(),
        username: alias.unwrap_or(name),
    });
}

#[view(accessor = my_user_procedural, public, primary_key = id)]
pub fn my_user_procedural(ctx: &ViewContext) -> Vec<Player> {
    ctx.db.player().id().find(1u32).into_iter().collect()
}
