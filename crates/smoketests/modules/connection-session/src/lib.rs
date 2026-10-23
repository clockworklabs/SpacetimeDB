//! Logs the lifecycle reducers with their connection ids, so tests can assert
//! the order in which connections are established and torn down.

use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer(client_connected)]
pub fn connected(ctx: &ReducerContext) {
    log::info!(
        "connected {}",
        ctx.connection_id()
            .map(|id| id.to_hex().to_string())
            .unwrap_or_default()
    );
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnected(ctx: &ReducerContext) {
    log::info!(
        "disconnected {}",
        ctx.connection_id()
            .map(|id| id.to_hex().to_string())
            .unwrap_or_default()
    );
}
