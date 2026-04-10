use bytes::Bytes;
pub use spacetimedb_sats::SpacetimeType;

pub const BIN_PROTOCOL: &str = "v3.bsatn.spacetimedb";

/// Transport envelopes sent by the client over the v3 websocket protocol.
///
/// The inner bytes are BSATN-encoded v2 [`crate::websocket::v2::ClientMessage`] values.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub enum ClientFrame {
    /// A single logical client message.
    Single(Bytes),
    /// Multiple logical client messages that should be processed in-order.
    Batch(Box<[Bytes]>),
}

/// Transport envelopes sent by the server over the v3 websocket protocol.
///
/// The inner bytes are BSATN-encoded v2 [`crate::websocket::v2::ServerMessage`] values.
#[derive(SpacetimeType, Debug)]
#[sats(crate = spacetimedb_lib)]
pub enum ServerFrame {
    /// A single logical server message.
    Single(Bytes),
    /// Multiple logical server messages that should be processed in-order.
    Batch(Box<[Bytes]>),
}
