use spacetimedb::{reducer, table, Identity, ReducerContext, SpacetimeType, Table};

#[table(name = player, public)]
pub struct Player {
    #[primary_key]
    owner_id: String,
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

#[reducer]
pub fn create_player(ctx: &ReducerContext, name: String, location: Point) {
    ctx.db.player().insert(Player {
        owner_id: ctx.sender.to_hex().to_string(),
        name,
        location,
    });
}
