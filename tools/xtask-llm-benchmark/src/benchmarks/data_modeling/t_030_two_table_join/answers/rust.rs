use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = customer)]
pub struct Customer {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub name: String,
}

#[table(accessor = order)]
pub struct Order {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub customer_id: u64,
    pub product: String,
    pub amount: u32,
}

#[table(accessor = order_detail)]
pub struct OrderDetail {
    #[primary_key]
    pub order_id: u64,
    pub customer_name: String,
    pub product: String,
    pub amount: u32,
}

#[reducer]
pub fn build_order_details(ctx: &ReducerContext) {
    for o in ctx.db.order().iter() {
        if let Some(c) = ctx.db.customer().id().find(o.customer_id) {
            ctx.db.order_detail().insert(OrderDetail {
                order_id: o.id,
                customer_name: c.name,
                product: o.product,
                amount: o.amount,
            });
        }
    }
}
