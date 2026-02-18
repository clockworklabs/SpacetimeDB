use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = t1)]
pub struct T1 { id: u64 }

#[spacetimedb::table(accessor = t2)]
pub struct T2 { id: u64 }

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    for id in 0..1_000 {
        ctx.db.t1().insert(T1 { id });
        ctx.db.t2().insert(T2 { id });
    }
}
