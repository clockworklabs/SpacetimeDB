use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use spacetimedb_datastore::execution_context::ReducerContext;
use spacetimedb_datastore::traits::TxData;
use spacetimedb_durability::TxOffset;

/// Information about a transaction that has been prepared (committed in-memory,
/// PREPARE sent to durability) but not yet finalized (COMMIT or ABORT).
pub struct PreparedTxInfo {
    /// The offset of the PREPARE record in the commitlog.
    pub tx_offset: TxOffset,
    /// The transaction data (row changes) for potential abort inversion.
    pub tx_data: Arc<TxData>,
    /// The reducer context for the prepared transaction.
    pub reducer_context: Option<ReducerContext>,
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
