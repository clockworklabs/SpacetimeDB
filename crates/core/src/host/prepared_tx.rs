use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Information about a prepared (but not yet committed or aborted) 2PC transaction.
/// Sending `true` commits; sending `false` aborts.
pub struct PreparedTxInfo {
    pub decision_sender: std::sync::mpsc::Sender<bool>,
}

/// Thread-safe registry of prepared transactions, keyed by prepare_id.
#[derive(Clone, Default)]
pub struct PreparedTransactions {
    inner: Arc<Mutex<HashMap<String, PreparedTxInfo>>>,
}

impl PreparedTransactions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, id: String, info: PreparedTxInfo) {
        self.inner.lock().unwrap().insert(id, info);
    }

    pub fn remove(&self, id: &str) -> Option<PreparedTxInfo> {
        self.inner.lock().unwrap().remove(id)
    }
}
