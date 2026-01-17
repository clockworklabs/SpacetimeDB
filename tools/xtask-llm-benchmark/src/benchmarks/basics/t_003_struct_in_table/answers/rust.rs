use spacetimedb::{table, SpacetimeType};

#[derive(SpacetimeType, Clone, Debug)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[table(name = entity)]
pub struct Entity {
    #[primary_key]
    pub id: i32,
    pub pos: Position,
}

