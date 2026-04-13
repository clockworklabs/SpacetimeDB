//! Messages sent over the SpacetimeDB WebSocket protocol.
//!
//! Client -> Server messages are encoded as [`ClientMessage`].
//! Server -> Client messages are encoded as [`ServerMessage`].
//!
//! Any changes to this file must be paired with a change to the WebSocket protocol identifiers
//! defined in `crates/client-api/src/routes/subscribe.rs`,
//! and be paired with changes to all of:
//!
//! - The C# SDK.
//! - The TypeScript SDK.
//! - The SpacetimeDB website.
//!
//! Changes to the Rust SDK are not necessarily required, as it depends on this crate
//! rather than using an external mirror of this schema.

pub mod common;
pub mod v1;
pub mod v2;
