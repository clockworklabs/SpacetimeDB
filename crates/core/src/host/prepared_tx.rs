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

/// A buffered durability request, held behind the persistence barrier.
pub struct BufferedDurabilityRequest {
    pub reducer_context: Option<ReducerContext>,
    pub tx_data: Arc<TxData>,
}

/// The persistence barrier prevents durability requests from being sent to the
/// durability worker while a 2PC PREPARE is pending.
///
/// When active:
/// - The PREPARE's own durability request has already been sent to the worker.
/// - All subsequent `request_durability()` calls are buffered here.
/// - Once the PREPARE is confirmed durable and a COMMIT/ABORT decision is made:
///   - COMMIT: buffered requests are flushed to the worker.
///   - ABORT: buffered requests are discarded.
#[derive(Default)]
pub struct PersistenceBarrier {
    inner: Mutex<PersistenceBarrierInner>,
}

#[derive(Default)]
struct PersistenceBarrierInner {
    /// If Some, a PREPARE is pending at this offset. All durability requests
    /// are buffered until the barrier is lifted.
    active_prepare: Option<TxOffset>,
    /// Buffered durability requests that arrived while the barrier was active.
    buffered: Vec<BufferedDurabilityRequest>,
}

impl PersistenceBarrier {
    pub fn new() -> Self {
        Self::default()
    }

    /// Activate the barrier for a PREPARE at the given offset.
    /// Subsequent calls to `try_buffer` will return `true` (buffered).
    pub fn activate(&self, prepare_offset: TxOffset) {
        let mut inner = self.inner.lock().unwrap();
        assert!(
            inner.active_prepare.is_none(),
            "persistence barrier already active at offset {:?}, cannot activate for {prepare_offset}",
            inner.active_prepare,
        );
        inner.active_prepare = Some(prepare_offset);
        inner.buffered.clear();
    }

    /// If the barrier is active, buffer the durability request and return None.
    /// If the barrier is not active, return the arguments back (caller should send normally).
    pub fn try_buffer(
        &self,
        reducer_context: Option<ReducerContext>,
        tx_data: &Arc<TxData>,
    ) -> Option<Option<ReducerContext>> {
        let mut inner = self.inner.lock().unwrap();
        if inner.active_prepare.is_some() {
            inner.buffered.push(BufferedDurabilityRequest {
                reducer_context,
                tx_data: tx_data.clone(),
            });
            None // buffered successfully
        } else {
            Some(reducer_context) // not buffered, return context back
        }
    }

    /// Deactivate the barrier and return the buffered requests.
    /// Called on COMMIT (to flush them) or ABORT (to discard them).
    pub fn deactivate(&self) -> Vec<BufferedDurabilityRequest> {
        let mut inner = self.inner.lock().unwrap();
        inner.active_prepare = None;
        std::mem::take(&mut inner.buffered)
    }

    /// Check if the barrier is currently active.
    pub fn is_active(&self) -> bool {
        self.inner.lock().unwrap().active_prepare.is_some()
    }
}
