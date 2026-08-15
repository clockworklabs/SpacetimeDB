use spacetimedb::{reducer, table, view, AnonymousViewContext, ReducerContext, SpacetimeType, Table};
#[table(accessor = customer, public)]
pub struct Customer {
    #[primary_key]
    pub id: u64,
    pub name: String,
}
#[table(accessor = purchase, public)]
pub struct Purchase {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub customer_id: u64,
}
#[table(accessor = line_item, public)]
pub struct LineItem {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub purchase_id: u64,
    pub sku: String,
    #[index(btree)]
    pub visible: bool,
}
#[derive(SpacetimeType)]
pub struct OrderLineDetail {
    pub line_id: u64,
    pub customer_name: String,
    pub sku: String,
}
#[reducer]
pub fn seed(ctx: &ReducerContext) {
    ctx.db.customer().insert(Customer {
        id: 1,
        name: "Ada".into(),
    });
    ctx.db.purchase().insert(Purchase { id: 10, customer_id: 1 });
    ctx.db.line_item().insert(LineItem {
        id: 100,
        purchase_id: 10,
        sku: "SKU-1".into(),
        visible: true,
    });
}
#[view(accessor = order_line_detail, public)]
pub fn order_line_detail(ctx: &AnonymousViewContext) -> Vec<OrderLineDetail> {
    ctx.db
        .line_item()
        .visible()
        .filter(true)
        .filter_map(|line| {
            let purchase = ctx.db.purchase().id().find(line.purchase_id)?;
            let customer = ctx.db.customer().id().find(purchase.customer_id)?;
            Some(OrderLineDetail {
                line_id: line.id,
                customer_name: customer.name,
                sku: line.sku,
            })
        })
        .collect()
}
