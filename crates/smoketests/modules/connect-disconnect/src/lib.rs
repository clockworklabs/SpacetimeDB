use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer(client_connected)]
pub fn connected(_ctx: &ReducerContext) {
    log::info!("_connect called");
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnected(_ctx: &ReducerContext) {
    log::info!("disconnect called");
}

#[spacetimedb::reducer]
pub fn say_hello(_ctx: &ReducerContext) {
    log::info!("Hello, World!");
}
