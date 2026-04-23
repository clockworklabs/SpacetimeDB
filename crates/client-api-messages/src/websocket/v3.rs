//! Binary framing for websocket protocol v3.
//!
//! Unlike v2, v3 does not define a new outer message schema.
//! A single binary websocket payload contains one or more BSATN-encoded
//! [`crate::websocket::v2::ClientMessage`] values from client to server,
//! or one or more consecutive BSATN-encoded [`crate::websocket::v2::ServerMessage`]
//! values from server to client.
//!
//! Client and server may coalesce multiple messages into one websocket payload,
//! or send them separately, regardless of what the other one does,
//! so long as logical order is preserved.

pub const BIN_PROTOCOL: &str = "v3.bsatn.spacetimedb";
