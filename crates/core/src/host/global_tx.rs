use crate::identity::Identity;
use spacetimedb_lib::GlobalTxId;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalTxRole {
    Coordinator,
    Participant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalTxState {
    Running,
    Preparing,
    Prepared,
    Aborting,
    Aborted,
    Committing,
    Committed,
}

#[derive(Debug)]
pub struct GlobalTxSession {
    pub tx_id: GlobalTxId,
    pub role: GlobalTxRole,
    pub coordinator_identity: Identity,
    wounded: AtomicBool,
    state: Mutex<GlobalTxState>,
    prepare_id: Mutex<Option<String>>,
    participants: Mutex<HashMap<Identity, String>>,
}

impl GlobalTxSession {
    fn new(tx_id: GlobalTxId, role: GlobalTxRole, coordinator_identity: Identity) -> Self {
        Self {
            tx_id,
            role,
            coordinator_identity,
            wounded: AtomicBool::new(false),
            state: Mutex::new(GlobalTxState::Running),
            prepare_id: Mutex::new(None),
            participants: Mutex::new(HashMap::new()),
        }
    }

    pub fn is_wounded(&self) -> bool {
        self.wounded.load(Ordering::SeqCst)
    }

    pub fn wound(&self) -> bool {
        !self.wounded.swap(true, Ordering::SeqCst)
    }

    pub fn state(&self) -> GlobalTxState {
        *self.state.lock().unwrap()
    }

    pub fn set_state(&self, state: GlobalTxState) {
        *self.state.lock().unwrap() = state;
    }

    pub fn set_prepare_id(&self, prepare_id: Option<String>) {
        *self.prepare_id.lock().unwrap() = prepare_id;
    }

    pub fn prepare_id(&self) -> Option<String> {
        self.prepare_id.lock().unwrap().clone()
    }

    pub fn add_participant(&self, db_identity: Identity, prepare_id: String) {
        self.participants.lock().unwrap().insert(db_identity, prepare_id);
    }

    pub fn participants(&self) -> Vec<(Identity, String)> {
        self.participants
            .lock()
            .unwrap()
            .iter()
            .map(|(db, pid)| (*db, pid.clone()))
            .collect()
    }
}

struct LockState {
    owner: Option<GlobalTxId>,
    waiting: HashSet<GlobalTxId>,
    wounded_owners: HashSet<GlobalTxId>,
}

impl Default for LockState {
    fn default() -> Self {
        Self {
            owner: None,
            waiting: HashSet::new(),
            wounded_owners: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquireDisposition {
    Acquired,
    Wound(GlobalTxId),
    Wait,
}

#[derive(Default)]
pub struct GlobalTxManager {
    sessions: Mutex<HashMap<GlobalTxId, Arc<GlobalTxSession>>>,
    prepare_to_tx: Mutex<HashMap<String, GlobalTxId>>,
    lock_state: Mutex<LockState>,
    lock_notify: Notify,
}

impl GlobalTxManager {
    pub fn ensure_session(
        &self,
        tx_id: GlobalTxId,
        role: GlobalTxRole,
        coordinator_identity: Identity,
    ) -> Arc<GlobalTxSession> {
        let mut sessions = self.sessions.lock().unwrap();
        sessions
            .entry(tx_id)
            .or_insert_with(|| Arc::new(GlobalTxSession::new(tx_id, role, coordinator_identity)))
            .clone()
    }

    pub fn get_session(&self, tx_id: &GlobalTxId) -> Option<Arc<GlobalTxSession>> {
        self.sessions.lock().unwrap().get(tx_id).cloned()
    }

    pub fn remove_session(&self, tx_id: &GlobalTxId) {
        self.sessions.lock().unwrap().remove(tx_id);
    }

    pub fn tx_for_prepare(&self, prepare_id: &str) -> Option<GlobalTxId> {
        self.prepare_to_tx.lock().unwrap().get(prepare_id).copied()
    }

    pub fn set_prepare_mapping(&self, tx_id: GlobalTxId, prepare_id: String) {
        self.prepare_to_tx.lock().unwrap().insert(prepare_id.clone(), tx_id);
        if let Some(session) = self.get_session(&tx_id) {
            session.set_prepare_id(Some(prepare_id));
        }
    }

    pub fn remove_prepare_mapping(&self, prepare_id: &str) -> Option<GlobalTxId> {
        let tx_id = self.prepare_to_tx.lock().unwrap().remove(prepare_id);
        if let Some(tx_id) = tx_id
            && let Some(session) = self.get_session(&tx_id)
        {
            session.set_prepare_id(None);
        }
        tx_id
    }

    pub fn add_participant(&self, tx_id: GlobalTxId, db_identity: Identity, prepare_id: String) {
        if let Some(session) = self.get_session(&tx_id) {
            session.add_participant(db_identity, prepare_id);
        }
    }

    pub fn mark_state(&self, tx_id: &GlobalTxId, state: GlobalTxState) {
        if let Some(session) = self.get_session(tx_id) {
            session.set_state(state);
        }
    }

    pub fn is_wounded(&self, tx_id: &GlobalTxId) -> bool {
        self.get_session(tx_id).map(|s| s.is_wounded()).unwrap_or(false)
    }

    pub fn wound(&self, tx_id: &GlobalTxId) -> Option<Arc<GlobalTxSession>> {
        let session = self.get_session(tx_id)?;
        let _ = session.wound();
        if !matches!(session.state(), GlobalTxState::Committed | GlobalTxState::Aborted) {
            session.set_state(GlobalTxState::Aborting);
        }
        Some(session)
    }

    pub async fn acquire(&self, tx_id: GlobalTxId) -> AcquireDisposition {
        loop {
            let waiter = {
                let mut state = self.lock_state.lock().unwrap();
                match state.owner {
                    None => {
                        state.owner = Some(tx_id);
                        state.waiting.remove(&tx_id);
                        return AcquireDisposition::Acquired;
                    }
                    Some(owner) if owner == tx_id => {
                        state.waiting.remove(&tx_id);
                        return AcquireDisposition::Acquired;
                    }
                    Some(owner) => {
                        state.waiting.insert(tx_id);
                        if tx_id < owner && state.wounded_owners.insert(owner) {
                            return AcquireDisposition::Wound(owner);
                        }
                        self.lock_notify.notified()
                    }
                }
            };
            waiter.await;
        }
    }

    pub fn release(&self, tx_id: &GlobalTxId) {
        let mut state = self.lock_state.lock().unwrap();
        if state.owner.as_ref() == Some(tx_id) {
            state.owner = None;
            state.wounded_owners.remove(tx_id);
            self.lock_notify.notify_waiters();
        }
        state.waiting.remove(tx_id);
    }
}

#[cfg(test)]
mod tests {
    use super::{AcquireDisposition, GlobalTxManager};
    use crate::identity::Identity;
    use spacetimedb_lib::{GlobalTxId, Timestamp};
    use std::sync::Arc;
    use tokio::runtime::Runtime;
    use std::time::Duration;

    fn tx_id(ts: i64, db_byte: u8, nonce: u32) -> GlobalTxId {
        GlobalTxId::new(
            Timestamp::from_micros_since_unix_epoch(ts),
            Identity::from_byte_array([db_byte; 32]),
            nonce,
        )
    }

    #[test]
    fn older_requester_wounds_younger_owner() {
        let manager = GlobalTxManager::default();
        let younger = tx_id(20, 2, 0);
        let older = tx_id(10, 1, 0);
        manager.ensure_session(
            younger,
            super::GlobalTxRole::Participant,
            younger.creator_db,
        );

        let rt = Runtime::new().unwrap();
        assert_eq!(rt.block_on(manager.acquire(younger)), AcquireDisposition::Acquired);
        assert_eq!(rt.block_on(manager.acquire(older)), AcquireDisposition::Wound(younger));
        assert!(manager.wound(&younger).is_some());
        assert!(manager.is_wounded(&younger));
    }

    #[test]
    fn younger_requester_waits_behind_older_owner() {
        let manager = GlobalTxManager::default();
        let older = tx_id(10, 1, 0);
        let younger = tx_id(20, 2, 0);
        let rt = Runtime::new().unwrap();

        assert_eq!(rt.block_on(manager.acquire(older)), AcquireDisposition::Acquired);
        let wait = rt.block_on(async {
            tokio::time::timeout(Duration::from_millis(25), manager.acquire(younger)).await
        });
        assert!(wait.is_err());
    }

    #[test]
    fn waiter_acquires_after_release() {
        let manager = Arc::new(GlobalTxManager::default());
        let owner = tx_id(10, 1, 0);
        let waiter = tx_id(20, 2, 0);
        let rt = Runtime::new().unwrap();

        assert_eq!(rt.block_on(manager.acquire(owner)), AcquireDisposition::Acquired);

        let manager_for_thread = manager.clone();
        let handle = std::thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            assert_eq!(rt.block_on(manager_for_thread.acquire(waiter)), AcquireDisposition::Acquired);
            manager_for_thread.release(&waiter);
        });

        std::thread::sleep(Duration::from_millis(25));
        manager.release(&owner);
        handle.join().unwrap();
    }

    #[test]
    fn wound_is_idempotent() {
        let manager = GlobalTxManager::default();
        let tx_id = tx_id(10, 1, 0);
        let session = manager.ensure_session(tx_id, super::GlobalTxRole::Coordinator, tx_id.creator_db);

        assert!(!session.is_wounded());
        assert!(manager.wound(&tx_id).is_some());
        assert!(session.is_wounded());
        assert!(manager.wound(&tx_id).is_some());
        assert!(session.is_wounded());
    }
}
