#[spacetimedb::reducer(init)]
fn lifecycle_init(_ctx: &spacetimedb::ReducerContext) {}

#[spacetimedb::reducer(client_connected)]
fn lifecycle_client_connected(_ctx: &spacetimedb::ReducerContext) {}

#[spacetimedb::reducer(client_disconnected)]
fn lifecycle_client_disconnected(_ctx: &spacetimedb::ReducerContext) {}
