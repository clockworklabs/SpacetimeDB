use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = t1)]
pub struct T1 { #[index(btree)] id: u64 }

#[spacetimedb::table(name = t2)]
pub struct T2 { #[index(btree)] id: u64 }

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    for id in 0..1_000 {
        ctx.db.t1().insert(T1 { id });
        ctx.db.t2().insert(T2 { id });
    }
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext) {
    let id = 1_001;
    ctx.db.t1().insert(T1 { id });
    ctx.db.t2().insert(T2 { id });
}
