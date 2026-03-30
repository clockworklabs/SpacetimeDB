use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Information about a prepared (but not yet committed or aborted) 2PC transaction.
///
/// Pipelined 2PC uses two consecutive rounds:
///   Round 1 (Memory): `decision_sender` delivers COMMIT/ABORT.
///   Round 2 (Persist): `commit_persist_sender` delivers COMMIT_PERSIST after
///                       both sides are durable.
pub struct PreparedTxInfo {
    /// Round 1: sending `true` commits to memory; `false` aborts.
    pub decision_sender: std::sync::mpsc::Sender<bool>,
    /// Round 2: sending `()` signals that the coordinator's COMMIT_PERSIST has
    /// arrived and the participant should finalize persistence.
    /// If the sender is dropped without sending, the receiver sees `Err` = abort.
    pub commit_persist_sender: tokio::sync::oneshot::Sender<()>,
}

/// Coordinator-side: a waiter that gets signalled when a participant sends
/// PREPARED_TO_PERSIST (i.e. the participant's PREPARE_PERSIST is durable).
pub struct PersistPreparedWaiter {
    pub sender: tokio::sync::oneshot::Sender<()>,
}

/// Thread-safe registry of prepared transactions, keyed by prepare_id.
#[derive(Clone, Default)]
pub struct PreparedTransactions {
    inner: Arc<Mutex<HashMap<String, PreparedTxInfo>>>,
    /// Coordinator-side: waiters for PREPARED_TO_PERSIST signals from participants.
    persist_waiters: Arc<Mutex<HashMap<String, PersistPreparedWaiter>>>,
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

    /// Send a Round 1 COMMIT/ABORT decision without removing the entry.
    /// The entry stays for Round 2 (commit_persist).
    pub fn send_decision(&self, id: &str, commit: bool) -> Result<(), String> {
        let guard = self.inner.lock().unwrap();
        let info = guard.get(id).ok_or_else(|| format!("no such prepared transaction: {id}"))?;
        let _ = info.decision_sender.send(commit);
        Ok(())
    }

    /// Send a Round 2 COMMIT_PERSIST signal and remove the entry.
    pub fn send_commit_persist(&self, id: &str) -> Result<(), String> {
        let mut guard = self.inner.lock().unwrap();
        let info = guard.remove(id).ok_or_else(|| format!("no such prepared transaction: {id}"))?;
        let _ = info.commit_persist_sender.send(());
        Ok(())
    }

    /// Register a coordinator-side waiter for a participant's PREPARED_TO_PERSIST signal.
    pub fn register_persist_waiter(&self, id: String) -> tokio::sync::oneshot::Receiver<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.persist_waiters
            .lock()
            .unwrap()
            .insert(id, PersistPreparedWaiter { sender: tx });
        rx
    }

    /// Signal that a participant's PREPARED_TO_PERSIST has arrived (called from HTTP endpoint).
    pub fn signal_persist_prepared(&self, id: &str) -> Result<(), String> {
        let waiter = self
            .persist_waiters
            .lock()
            .unwrap()
            .remove(id)
            .ok_or_else(|| format!("no persist waiter for prepare_id: {id}"))?;
        let _ = waiter.sender.send(());
        Ok(())
    }
}
