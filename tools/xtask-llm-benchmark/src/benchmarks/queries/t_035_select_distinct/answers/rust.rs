use spacetimedb::{reducer, table, ReducerContext, Table};
use std::collections::BTreeSet;

#[table(accessor = order)]
pub struct Order {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub category: String,
    pub amount: u32,
}

#[table(accessor = distinct_category)]
pub struct DistinctCategory {
    #[primary_key]
    pub category: String,
}

#[reducer]
pub fn collect_distinct_categories(ctx: &ReducerContext) {
    let categories: BTreeSet<String> = ctx.db.order().iter().map(|o| o.category).collect();
    for category in categories {
        ctx.db.distinct_category().insert(DistinctCategory { category });
    }
}
