use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = counter, public)]
pub struct Counter {
    #[primary_key]
    id: u64,
    value: i64,
}

#[spacetimedb::table(accessor = release)]
pub struct Release {
    #[primary_key]
    version: u32,
}

#[spacetimedb::reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.counter().insert(Counter { id: 1, value: 1 });
}

#[spacetimedb::reducer]
pub fn increment(ctx: &ReducerContext, id: u64, amount: i64) {
    let mut row = ctx.db.counter().id().find(id).expect("counter");
    row.value += amount;
    ctx.db.counter().id().update(row);
}

#[spacetimedb::reducer]
pub fn record_release(ctx: &ReducerContext, version: u32) {
    ctx.db.release().insert(Release { version });
}
