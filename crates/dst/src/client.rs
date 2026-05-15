//! Logical client and topology identifiers shared by DST workloads and targets.
//!
//! These IDs are part of the generated workload language. Targets translate
//! them into concrete handles such as direct database transaction slots,
//! `ClientConnection`s, websocket sessions, or simulated-node connections.

use std::fmt;

/// Stable logical client identity within one DST run.
///
/// A `ClientId` is an actor/user identity, not a live network connection. One
/// client may own zero, one, or many [`SessionId`]s at the same time.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ClientId(u32);

impl ClientId {
    pub const ZERO: Self = Self(0);

    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

impl fmt::Display for ClientId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "client{}", self.0)
    }
}

/// Logical live connection/session for a client.
///
/// Current single-process targets use `SessionId` anywhere old DST code said
/// "connection": transaction slots, read snapshots, reducer-call handles, and
/// property observations. A target translates this logical session into its
/// concrete handle, such as a `RelTx` slot or `ClientConnection`.
///
/// The `generation` field is the per-client session ordinal. Workloads can keep
/// several generations active concurrently to model one client with multiple
/// open connections, or allocate a later generation after a reconnect.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SessionId {
    pub client: ClientId,
    pub generation: u32,
}

impl SessionId {
    pub const ZERO: Self = Self::new(ClientId::ZERO, 0);

    pub const fn new(client: ClientId, generation: u32) -> Self {
        Self { client, generation }
    }

    /// Compatibility helper for today's fixed-size session pools.
    ///
    /// A run with `N` connections starts as one logical client with `N`
    /// sessions: `client0/session0`, `client0/session1`, ...
    pub(crate) const fn from_index(index: usize) -> Self {
        Self::new(ClientId::ZERO, index as u32)
    }

    pub(crate) const fn as_index(self) -> usize {
        self.generation as usize
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.session{}", self.client, self.generation)
    }
}
