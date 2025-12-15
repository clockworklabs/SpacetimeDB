use spacetimedb::{reducer, table, Identity, ReducerContext, SpacetimeType, Table};

#[table(name = player, public)]
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

#[table(name = user, public)]
pub struct User {
    #[primary_key]
    pub identity: Identity,
    pub username: String,
}

#[table(name = unindexed_player, public)]
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
        identity: ctx.sender,
        username: name.clone(),
    });
    ctx.db.player().insert(Player {
        id: 0,
        user_id: ctx.sender,
        name,
        location,
    });
}
