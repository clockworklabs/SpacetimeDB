use spacetimedb_sdk as spacetimedb;

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(ctx: &spacetimedb::ReducerContext);
