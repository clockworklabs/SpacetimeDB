use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

use super::{ClientName, ClientSessionIndex};

#[derive(Default)]
pub struct ClientActorIndex {
    client_name_auto_increment_state: AtomicU64,
    sessions: Arc<ClientSessionIndex>,
}

impl ClientActorIndex {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn next_client_name(&self) -> ClientName {
        ClientName(self.client_name_auto_increment_state.fetch_add(1, Relaxed))
    }

    /// The map of live client sessions, used to replace a connection
    /// which a reconnect supersedes.
    ///
    /// Returns an owned handle, since the websocket handler needs one which
    /// outlives the request.
    pub fn sessions(&self) -> Arc<ClientSessionIndex> {
        self.sessions.clone()
    }
}
