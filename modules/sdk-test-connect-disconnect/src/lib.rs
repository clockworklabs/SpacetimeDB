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
use spacetimedb::{spacetimedb, Identity, ReducerContext};

#[spacetimedb(table)]
pub struct Connected {
    identity: Identity,
}

#[spacetimedb(table)]
pub struct Disconnected {
    identity: Identity,
}

#[spacetimedb(connect)]
pub fn identity_connected(ctx: ReducerContext) {
    Connected::insert(Connected { identity: ctx.sender });
}

#[spacetimedb(disconnect)]
pub fn identity_disconnected(ctx: ReducerContext) {
    Disconnected::insert(Disconnected { identity: ctx.sender });
}

#[spacetimedb(reducer)]
/// Due to a bug in SATS' `derive(Desrialize)`
/// https://github.com/clockworklabs/SpacetimeDB/issues/325 ,
/// Rust module bindings fail to compile for modules which define zero reducers
/// (not counting init, update, connect, disconnect).
/// Adding this useless empty reducer causes the module bindings to compile.
pub fn useless_empty_reducer() {}
