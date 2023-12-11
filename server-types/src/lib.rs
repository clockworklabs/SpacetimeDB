use spacetimedb::{spacetimedb, ReducerContext, SpacetimeType, Identity};

#[spacetimedb(table)]
pub struct Player {
    #[primarykey]
    owner_id: String,
    name: String,
    location: Point,
}

#[derive(SpacetimeType)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

#[spacetimedb(table)]
pub struct User {
    #[primarykey]
    pub identity: Identity,
    pub username: String,
}

#[spacetimedb(reducer)]
pub fn create_player(ctx: ReducerContext, name: String, location: Point) {
    Player::insert(Player { owner_id: ctx.sender.to_hex().to_string(), name, location });
}
