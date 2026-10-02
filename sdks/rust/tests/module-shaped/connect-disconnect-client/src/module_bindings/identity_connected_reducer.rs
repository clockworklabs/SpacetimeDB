use spacetimedb_sdk as spacetimedb;

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(ctx: &spacetimedb::ReducerContext);
