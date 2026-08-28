use spacetimedb::{table, SpacetimeType};

#[derive(SpacetimeType)]
struct SomeProduct {
    a: u32,
    b: u64,
}

#[table(accessor = some_event, public, event)]
struct SomeEvent {
    foo: String,
    prod: SomeProduct,
}
