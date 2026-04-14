use spacetimedb::{SpacetimeType, ViewContext};

#[derive(SpacetimeType)]
pub enum ABC {
    A,
    B,
    C,
}

#[spacetimedb::view(accessor = person, public)]
pub fn person(ctx: &ViewContext) -> Option<ABC> {
    None
}
