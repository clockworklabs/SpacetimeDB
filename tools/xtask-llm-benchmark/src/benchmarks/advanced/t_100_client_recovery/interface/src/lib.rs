use spacetimedb::{ReducerContext, Table};
#[spacetimedb::table(accessor = counter, public)]
pub struct Counter { #[primary_key] id: u32, value: i32 }
#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) { ctx.db.counter().insert(Counter { id: 0, value: 0 }); }
#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, delta: i32) {
    let mut row = ctx.db.counter().id().find(0).unwrap();
    row.value += delta;
    ctx.db.counter().id().update(row);
}
