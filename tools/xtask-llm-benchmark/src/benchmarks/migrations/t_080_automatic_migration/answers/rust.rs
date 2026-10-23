use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = product, public)]
pub struct Product {
    #[primary_key]
    id: u64,
    name: String,
}

#[spacetimedb::table(accessor = category, public)]
pub struct Category {
    #[primary_key]
    id: u64,
    label: String,
}

#[spacetimedb::reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.product().insert(Product {
        id: 1,
        name: "legacy".into(),
    });
}

#[spacetimedb::reducer]
pub fn touch(_ctx: &ReducerContext) {}

#[spacetimedb::reducer]
pub fn create_category(ctx: &ReducerContext, id: u64, label: String) {
    ctx.db.category().insert(Category { id, label });
}
