use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(accessor = all_u8s, public)]
pub struct AllU8s {
    number: u8,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    for i in u8::MIN..=u8::MAX {
        ctx.db.all_u8s().insert(AllU8s { number: i });
    }
}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(_ctx: &ReducerContext) -> Result<(), String> {
    Ok(())
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    panic!("This should be called, but the `st_client` row should still be deleted")
}
