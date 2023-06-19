use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use super::ClientName;

#[derive(Default)]
pub struct ClientActorIndex {
    client_name_auto_increment_state: AtomicU64,
}

impl ClientActorIndex {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn next_client_name(&self) -> ClientName {
        ClientName(self.client_name_auto_increment_state.fetch_add(1, Relaxed))
    }
}
