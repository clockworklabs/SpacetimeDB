use spacetimedb::{ReducerContext, Table, ViewContext};

#[spacetimedb::table(accessor = items, public)]
pub struct Item {
    value: u8,
}

mod foo {
    use super::*;

    #[spacetimedb::view(accessor = bar, public)]
    pub(crate) fn bar(_ctx: &ViewContext) -> Option<Item> {
        Some(Item { value: 7 })
    }

    #[spacetimedb::view(accessor = item_with_value, public)]
    pub(crate) fn item_with_value(_ctx: &ViewContext, value: u8) -> Option<Item> {
        Some(Item { value })
    }
}

#[spacetimedb::reducer]
pub fn baz(ctx: &ReducerContext) {
    if let Some(item) = foo::bar(&ctx.as_read_only()) {
        ctx.db.items().insert(item);
    }
}

#[spacetimedb::reducer]
pub fn baz_with_value(ctx: &ReducerContext, value: u8) {
    if let Some(item) = foo::item_with_value(&ctx.as_read_only(), value) {
        ctx.db.items().insert(item);
    }
}
