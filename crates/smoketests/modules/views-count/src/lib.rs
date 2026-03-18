use spacetimedb::{reducer, table, view, AnonymousViewContext};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext};

#[table(accessor = item)]
pub struct Item {
    #[primary_key]
    id: u32,
    value: u32,
}

#[derive(SpacetimeType)]
pub struct ItemCount {
    count: u64,
}

#[view(accessor = sender_table_count, public)]
pub fn sender_table_count(ctx: &ViewContext) -> Option<ItemCount> {
    Some(ItemCount {
        count: ctx.db.item().count(),
    })
}

#[view(accessor = anon_table_count, public)]
pub fn anon_table_count(ctx: &AnonymousViewContext) -> Option<ItemCount> {
    Some(ItemCount {
        count: ctx.db.item().count(),
    })
}

#[reducer]
pub fn insert_item(ctx: &ReducerContext, id: u32, value: u32) {
    ctx.db.item().insert(Item { id, value });
}

#[reducer]
pub fn replace_item(ctx: &ReducerContext, id: u32, value: u32) {
    ctx.db.item().id().delete(&id);
    ctx.db.item().insert(Item { id, value });
}

#[reducer]
pub fn delete_item(ctx: &ReducerContext, id: u32) {
    ctx.db.item().id().delete(&id);
}
