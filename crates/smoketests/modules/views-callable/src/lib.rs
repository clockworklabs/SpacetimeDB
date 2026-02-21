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
}

#[spacetimedb::reducer]
pub fn baz(ctx: &ReducerContext) {
    if let Some(item) = foo::bar(&ctx.as_read_only()) {
        ctx.db.items().insert(item);
    }
}
