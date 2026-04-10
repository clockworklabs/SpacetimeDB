use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Information about a prepared (but not yet committed or aborted) 2PC transaction.
/// Sending `true` commits; sending `false` aborts.
pub struct PreparedTxInfo {
    pub decision_sender: std::sync::mpsc::Sender<bool>,
}

enum PreparedTxState {
    Waiting(PreparedTxInfo),
    EarlyDecision { commit: bool },
}

/// Thread-safe registry of prepared transactions, keyed by prepare_id.
#[derive(Clone, Default)]
pub struct PreparedTransactions {
    inner: Arc<Mutex<HashMap<String, PreparedTxState>>>,
}

impl PreparedTransactions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_waiter(&self, id: String, info: PreparedTxInfo) -> Option<bool> {
        let mut inner = self.inner.lock().unwrap();
        match inner.remove(&id) {
            Some(PreparedTxState::EarlyDecision { commit }) => Some(commit),
            Some(PreparedTxState::Waiting(_)) | None => {
                inner.insert(id, PreparedTxState::Waiting(info));
                None
            }
        }
    }

    pub fn deliver_or_remember_decision(&self, id: &str, commit: bool) {
        let mut inner = self.inner.lock().unwrap();
        match inner.remove(id) {
            Some(PreparedTxState::Waiting(info)) => {
                let _ = info.decision_sender.send(commit);
            }
            Some(PreparedTxState::EarlyDecision { commit: existing }) => {
                inner.insert(id.to_string(), PreparedTxState::EarlyDecision { commit: existing });
            }
            None => {
                inner.insert(id.to_string(), PreparedTxState::EarlyDecision { commit });
            }
        }
    }

    pub fn clear(&self, id: &str) {
        self.inner.lock().unwrap().remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::{PreparedTransactions, PreparedTxInfo};

    #[test]
    fn early_decision_is_returned_when_waiter_registers_later() {
        let prepared_txs = PreparedTransactions::new();
        prepared_txs.deliver_or_remember_decision("prepare-id", false);

        let (decision_tx, decision_rx) = std::sync::mpsc::channel();
        let early_decision =
            prepared_txs.register_waiter("prepare-id".to_string(), PreparedTxInfo { decision_sender: decision_tx });

        assert_eq!(early_decision, Some(false));
        assert!(decision_rx.try_recv().is_err());
    }

    #[test]
    fn waiting_entry_receives_decision_immediately() {
        let prepared_txs = PreparedTransactions::new();
        let (decision_tx, decision_rx) = std::sync::mpsc::channel();
        let early_decision =
            prepared_txs.register_waiter("prepare-id".to_string(), PreparedTxInfo { decision_sender: decision_tx });

        assert_eq!(early_decision, None);
        prepared_txs.deliver_or_remember_decision("prepare-id", false);

        assert_eq!(decision_rx.recv().unwrap(), false);
    }

    #[test]
    fn first_early_decision_wins() {
        let prepared_txs = PreparedTransactions::new();
        prepared_txs.deliver_or_remember_decision("prepare-id", false);
        prepared_txs.deliver_or_remember_decision("prepare-id", true);

        let (decision_tx, _decision_rx) = std::sync::mpsc::channel();
        let early_decision =
            prepared_txs.register_waiter("prepare-id".to_string(), PreparedTxInfo { decision_sender: decision_tx });

        assert_eq!(early_decision, Some(false));
    }
}
