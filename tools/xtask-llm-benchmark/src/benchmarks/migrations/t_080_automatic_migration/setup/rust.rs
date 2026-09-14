use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = product, public)]
pub struct Product {
    #[primary_key]
    id: u64,
    name: String,
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
