use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(name = results)]
pub struct ResultRow {
    #[primary_key]
    pub id: i32,
    pub sum: i32,
}

fn add(a: i32, b: i32) -> i32 { a + b }

#[reducer]
pub fn compute_sum(ctx: &ReducerContext, id: i32, a: i32, b: i32) {
    ctx.db.results().insert(ResultRow { id, sum: add(a, b) });
}
