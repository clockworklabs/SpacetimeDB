use spacetimedb::{reducer, table, view, Identity, ReducerContext, Table, ViewContext};

#[table(accessor = left_source, public)]
pub struct LeftSource {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub sender: Identity,
    pub filter: u64,
}

#[table(accessor = right_source, public)]
pub struct RightSource {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub sender: Identity,
    pub filter: u64,
}

#[reducer]
pub fn insert_left(ctx: &ReducerContext, id: u64, filter: u64) {
    ctx.db.left_source().insert(LeftSource {
        id,
        sender: ctx.sender(),
        filter,
    });
}

#[reducer]
pub fn update_left(ctx: &ReducerContext, id: u64, filter: u64) {
    ctx.db.left_source().id().update(LeftSource {
        id,
        sender: ctx.sender(),
        filter,
    });
}

#[reducer]
pub fn insert_right(ctx: &ReducerContext, id: u64, filter: u64) {
    ctx.db.right_source().insert(RightSource {
        id,
        sender: ctx.sender(),
        filter,
    });
}

#[view(accessor = sender_left_view, public, primary_key = id)]
pub fn sender_left_view(ctx: &ViewContext) -> Vec<LeftSource> {
    ctx.db.left_source().sender().filter(ctx.sender()).collect()
}

#[view(accessor = sender_right_view, public, primary_key = id)]
pub fn sender_right_view(ctx: &ViewContext) -> Vec<RightSource> {
    ctx.db.right_source().sender().filter(ctx.sender()).collect()
}
