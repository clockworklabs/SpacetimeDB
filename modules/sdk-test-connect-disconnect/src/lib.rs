//! This module tests that we can observe connect/disconnect events
//! for WebSocket connections.
//!
//! The test flow is:
//! - Connect once.
//! - Subscribe to `Connected`.
//! - Observe the presence of one row with the client's `Identity`.
//! - Disconnect, then reconnect again.
//! - Subscribe to `Disconnected`.
//! - Observe the presence of one row with the client's `Identity`.
use spacetimedb::{Identity, ReducerContext};

#[spacetimedb::table(name = Connected, public)]
pub struct Connected {
    identity: Identity,
}

#[spacetimedb::table(name = Disconnected, public)]
pub struct Disconnected {
    identity: Identity,
}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(ctx: ReducerContext) {
    Connected::insert(Connected { identity: ctx.sender });
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(ctx: ReducerContext) {
    Disconnected::insert(Disconnected { identity: ctx.sender });
}
