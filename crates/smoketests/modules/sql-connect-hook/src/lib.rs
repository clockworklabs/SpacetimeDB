use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(accessor = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.person().insert(Person {
        name: "Alice".to_string(),
    });
}

#[spacetimedb::reducer(client_connected)]
pub fn connected(ctx: &ReducerContext) {
    log::info!("sql_connect_hook: client_connected caller={}", ctx.sender());
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnected(ctx: &ReducerContext) {
    log::info!("sql_connect_hook: client_disconnected caller={}", ctx.sender());
}
