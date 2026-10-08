use spacetimedb::{AnonymousViewContext, Identity, ReducerContext, Table, ViewContext};

#[spacetimedb::table(accessor = item, public)]
pub struct Item {
    #[index(btree)]
    id: u32,
    owner: Identity,
    value: u32,
}

#[spacetimedb::reducer]
pub fn add_item(ctx: &ReducerContext, id: u32, value: u32) {
    ctx.db.item().insert(Item {
        id,
        owner: ctx.sender(),
        value,
    });
}

/// An anonymous view with an argument: each `id` is its own instance.
#[spacetimedb::view(accessor = by_id, public)]
fn by_id(ctx: &AnonymousViewContext, id: u32) -> Vec<Item> {
    ctx.db.item().id().filter(id).collect()
}

/// A sender-scoped view with an argument: each (caller, `id`) is its own instance.
#[spacetimedb::view(accessor = mine_by_id, public)]
fn mine_by_id(ctx: &ViewContext, id: u32) -> Vec<Item> {
    ctx.db
        .item()
        .id()
        .filter(id)
        .filter(|item| item.owner == ctx.sender())
        .collect()
}
