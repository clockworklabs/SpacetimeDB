use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = order)]
pub struct Order {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub category: String,
    pub amount: u64,
    pub fulfilled: bool,
}

#[table(accessor = category_stats)]
pub struct CategoryStats {
    #[primary_key]
    pub category: String,
    pub total_amount: u64,
    pub order_count: u32,
}

#[reducer]
pub fn compute_stats(ctx: &ReducerContext, category: String) {
    let mut total_amount: u64 = 0;
    let mut order_count: u32 = 0;

    for o in ctx.db.order().category().filter(&category) {
        total_amount += o.amount;
        order_count += 1;
    }

    // Upsert: delete existing then insert
    let _ = ctx.db.category_stats().category().delete(category.clone());
    ctx.db.category_stats().insert(CategoryStats {
        category,
        total_amount,
        order_count,
    });
}
