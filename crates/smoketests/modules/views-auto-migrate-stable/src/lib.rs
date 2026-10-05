use spacetimedb::{reducer, table, view, ReducerContext, Table, ViewContext};

// This is the table that gets updated during the test
#[derive(Copy, Clone)]
#[table(accessor = counter)]
pub struct Counter {
    #[primary_key]
    id: u64,
    value: u64,
}

// This table is not subscribed to or updated in the test.
// Its main purpose is to have a different row size than `Counter`.
#[table(accessor = marker)]
pub struct Marker {
    #[primary_key]
    id: u64,
    label: String,
}

#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.counter().insert(Counter { id: 0, value: 0 });
    ctx.db.marker().insert(Marker {
        id: 0,
        label: "0".to_string(),
    });
}

#[reducer]
pub fn bump_counter(ctx: &ReducerContext) {
    let mut counter = ctx.db.counter().id().find(&0).expect("counter row should exist");
    ctx.db.counter().id().delete(&0);
    counter.value += 1;
    ctx.db.counter().insert(counter);
}

#[view(accessor = z_counter, public)]
pub fn z_counter(ctx: &ViewContext) -> Option<Counter> {
    ctx.db.counter().id().find(&0)
}
