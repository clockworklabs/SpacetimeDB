use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = product)]
pub struct Product {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
    #[index(btree)]
    pub price: u32,
}

#[table(accessor = price_range_result)]
pub struct PriceRangeResult {
    #[primary_key]
    pub product_id: u64,
    pub name: String,
    pub price: u32,
}

#[reducer]
pub fn find_in_price_range(ctx: &ReducerContext, min_price: u32, max_price: u32) {
    for p in ctx.db.product().iter() {
        if p.price >= min_price && p.price <= max_price {
            ctx.db.price_range_result().insert(PriceRangeResult {
                product_id: p.id,
                name: p.name,
                price: p.price,
            });
        }
    }
}
