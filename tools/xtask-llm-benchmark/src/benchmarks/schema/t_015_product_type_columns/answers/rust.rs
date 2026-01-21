use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

#[derive(SpacetimeType, Clone, Debug)]
pub struct Address {
    pub street: String,
    pub zip: i32,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[table(name = profile)]
pub struct Profile {
    #[primary_key]
    pub id: i32,
    pub home: Address,
    pub work: Address,
    pub pos: Position,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.profile().insert(Profile {
        id: 1,
        home: Address { street: "1 Main".into(),  zip: 11111 },
        work: Address { street: "2 Broad".into(), zip: 22222 },
        pos:  Position { x: 7, y: 9 },
    });
}
