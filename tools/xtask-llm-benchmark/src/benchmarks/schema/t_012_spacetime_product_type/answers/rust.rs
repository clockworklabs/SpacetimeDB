use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

#[derive(SpacetimeType, Clone, Debug)]
pub struct Score {
    pub left: i32,
    pub right: i32,
}

#[table(name = result)]
pub struct ResultRow {
    #[primary_key]
    pub id: i32,
    pub value: Score,
}

#[reducer]
pub fn set_score(ctx: &ReducerContext, id: i32, left: i32, right: i32) {
    ctx.db.result().insert(ResultRow { id, value: Score { left, right } });
}
