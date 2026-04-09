use crate::identity::Identity;
use crate::worker_metrics::WORKER_METRICS;
use spacetimedb_lib::GlobalTxId;
use std::cmp::Ordering as CmpOrdering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{watch, Notify};

const DEFAULT_WOUND_GRACE_PERIOD: Duration = Duration::from_millis(30);

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
    wounded_tx: watch::Sender<bool>,
    state: Mutex<GlobalTxState>,
    prepare_id: Mutex<Option<String>>,
    participants: Mutex<Vec<(Identity, String)>>,
}

impl GlobalTxSession {
    fn new(tx_id: GlobalTxId, role: GlobalTxRole, coordinator_identity: Identity) -> Self {
        let (wounded_tx, _) = watch::channel(false);
        Self {
            tx_id,
            role,
            coordinator_identity,
            wounded: AtomicBool::new(false),
            wounded_tx,
            state: Mutex::new(GlobalTxState::Running),
            prepare_id: Mutex::new(None),
            participants: Mutex::new(Vec::new()),
        }
    }

    pub fn is_wounded(&self) -> bool {
        self.wounded.load(Ordering::SeqCst)
    }

    pub fn wound(&self) -> bool {
        let was_fresh = !self.wounded.swap(true, Ordering::SeqCst);
        if was_fresh {
            let _ = self.wounded_tx.send(true);
        }
        was_fresh
    }

    pub fn subscribe_wounded(&self) -> watch::Receiver<bool> {
        self.wounded_tx.subscribe()
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
        self.participants.lock().unwrap().push((db_identity, prepare_id));
    }

    pub fn participants(&self) -> Vec<(Identity, String)> {
        self.participants.lock().unwrap().clone()
    }
}

struct LockState {
    owner: Option<GlobalTxId>,
    // An set of waiters ordered by tx_id with the oldest first.
    waiting: BTreeSet<WaitKey>,
    // A map from wait_id to the corresponding wait entry, which contains the notify object to wake up the waiter when its turn comes.
    wait_entries: HashMap<u64, WaitEntry>,
    waiter_ids_by_tx: HashMap<GlobalTxId, u64>,
    wounded_owners: HashSet<GlobalTxId>,
    next_wait_id: u64,
}

impl Default for LockState {
    fn default() -> Self {
        Self {
            owner: None,
            waiting: BTreeSet::new(),
            wait_entries: HashMap::new(),
            waiter_ids_by_tx: HashMap::new(),
            wounded_owners: HashSet::new(),
            next_wait_id: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WaitKey {
    tx_id: GlobalTxId,
    wait_id: u64,
}

impl Ord for WaitKey {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        self.tx_id
            .cmp(&other.tx_id)
            .then_with(|| self.wait_id.cmp(&other.wait_id))
    }
}

impl PartialOrd for WaitKey {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
struct WaitEntry {
    tx_id: GlobalTxId,
    notify: Arc<Notify>,
}

pub enum AcquireDisposition {
    Acquired(GlobalTxLockGuard),
    Cancelled,
}

pub struct GlobalTxLockGuard {
    manager: Arc<GlobalTxManager>,
    tx_id: Option<GlobalTxId>,
}

struct WaitRegistration<'a> {
    manager: &'a GlobalTxManager,
    wait_id: Option<u64>,
}

impl<'a> WaitRegistration<'a> {
    fn new(manager: &'a GlobalTxManager, wait_id: u64) -> Self {
        Self {
            manager,
            wait_id: Some(wait_id),
        }
    }

    fn wait_id(&self) -> u64 {
        self.wait_id.expect("registered waiter must still have a wait id")
    }

    fn disarm(mut self, ls: &mut std::sync::MutexGuard<'_, LockState>) {
        self.remove_waiter(ls);
    }

    fn remove_waiter(&mut self, ls: &mut LockState) {
        if let Some(wait_id) = self.wait_id.take() {
            self.manager.remove_waiter_by_id(ls, wait_id);
        }
    }
}

impl Drop for WaitRegistration<'_> {
    fn drop(&mut self) {
        if self.wait_id.is_none() {
            return;
        }
        let mut ls = self.manager.lock_state.lock().unwrap();
        self.remove_waiter(&mut ls);
    }
}

impl GlobalTxLockGuard {
    fn new(manager: Arc<GlobalTxManager>, tx_id: GlobalTxId) -> Self {
        Self {
            manager,
            tx_id: Some(tx_id),
        }
    }

    pub fn tx_id(&self) -> GlobalTxId {
        self.tx_id.expect("lock guard must always have a tx_id before drop")
    }
}

impl Drop for GlobalTxLockGuard {
    fn drop(&mut self) {
        if let Some(tx_id) = self.tx_id.take() {
            self.manager.release(&tx_id);
        }
    }
}

pub struct GlobalTxManager {
    sessions: Mutex<HashMap<GlobalTxId, Arc<GlobalTxSession>>>,
    prepare_to_tx: Mutex<HashMap<String, GlobalTxId>>,
    lock_state: Mutex<LockState>,
    wound_grace_period: Duration,
}

impl Default for GlobalTxManager {
    fn default() -> Self {
        Self::new(DEFAULT_WOUND_GRACE_PERIOD)
    }
}

impl GlobalTxManager {
    fn session_metric_labels(&self, tx_id: &GlobalTxId) -> Option<(Identity, &'static str)> {
        let session = self.get_session(tx_id)?;
        let role = match session.role {
            GlobalTxRole::Coordinator => "coordinator",
            GlobalTxRole::Participant => "participant",
        };
        Some((session.coordinator_identity, role))
    }

    pub fn new(wound_grace_period: Duration) -> Self {
        Self {
            sessions: Mutex::default(),
            prepare_to_tx: Mutex::default(),
            lock_state: Mutex::default(),
            wound_grace_period,
        }
    }

    pub fn wound_grace_period(&self) -> Duration {
        self.wound_grace_period
    }

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

    pub fn subscribe_wounded(&self, tx_id: &GlobalTxId) -> Option<watch::Receiver<bool>> {
        self.get_session(tx_id).map(|s| s.subscribe_wounded())
    }

    // This should only be called by the coordinator.
    // Arguably we should have a separate state for wounded and aborted, in case we wound a remote tx before we send write the prepare.
    pub fn wound(&self, tx_id: &GlobalTxId) -> Option<Arc<GlobalTxSession>> {
        let session = self.get_session(tx_id)?;
        let was_fresh = session.wound();
        if !matches!(session.state(), GlobalTxState::Committed | GlobalTxState::Aborted) {
            session.set_state(GlobalTxState::Aborting);
        }
        if was_fresh {
            let role = match session.role {
                GlobalTxRole::Coordinator => "coordinator",
                GlobalTxRole::Participant => "participant",
            };
            log::info!(
                "global transaction {tx_id} marked wounded; role={:?} coordinator={}",
                session.role,
                session.coordinator_identity
            );
            WORKER_METRICS
                .transactions_wounded_total
                .with_label_values(&session.coordinator_identity, &role)
                .inc();
        }
        Some(session)
    }

    pub async fn acquire<F, Fut>(self: &Arc<Self>, tx_id: GlobalTxId, mut on_wound: F) -> AcquireDisposition
    where
        F: FnMut(GlobalTxId) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let acquire_started_at = Instant::now();
        let observe_acquire_time = || {
            if let Some((coordinator_identity, role)) = self.session_metric_labels(&tx_id) {
                WORKER_METRICS
                    .global_tx_lock_acquire_time
                    .with_label_values(&coordinator_identity, &role)
                    .observe(acquire_started_at.elapsed().as_secs_f64());
            }
        };
        let mut wounded_rx = match self.subscribe_wounded(&tx_id) {
            Some(rx) => rx,
            None => return AcquireDisposition::Cancelled,
        };
        let mut registration: Option<WaitRegistration<'_>> = None;
        loop {
            if *wounded_rx.borrow() {
                return AcquireDisposition::Cancelled;
            }
            if self.is_terminalish(&tx_id) {
                return AcquireDisposition::Cancelled;
            }

            let (notify, owner_to_wound, new_registration, cancelled): (
                Arc<Notify>,
                Option<GlobalTxId>,
                Option<WaitRegistration<'_>>,
                bool,
            ) = {
                let mut state = self.lock_state.lock().unwrap();
                if state.owner.is_none() {
                    self.prune_stale_head_waiters_locked(&mut state);
                }
                match state.owner {
                    None if self.is_next_waiter_locked(&state, tx_id) => {
                        log::info!("setting owner to {tx_id}");
                        state.owner = Some(tx_id);
                        self.remove_waiter_locked(&mut state, &tx_id);
                        if let Some(registration) = registration.take() {
                            registration.disarm(&mut state);
                        }
                        log::info!("global transaction {tx_id} acquired the lock");
                        observe_acquire_time();
                        return AcquireDisposition::Acquired(GlobalTxLockGuard::new(self.clone(), tx_id));
                    }
                    None => {
                        let waiter = match registration.as_ref() {
                            Some(registration) => match self.registered_waiter_locked(&state, tx_id, registration) {
                                Ok(registered_waiter) => Some(registered_waiter),
                                Err(()) => None,
                            },
                            None => Some(self.ensure_waiter_locked(&mut state, tx_id)),
                        };
                        let Some((wait_id, notify)) = waiter else {
                            return AcquireDisposition::Cancelled;
                        };
                        let head_waiter = state.waiting.first().map(|wait_key| wait_key.tx_id);
                        if let Some(head_waiter) = head_waiter
                            && head_waiter != tx_id
                        {
                            log::info!(
                                "global transaction {tx_id} observed ownerless lock while queued behind head waiter {head_waiter}; nudging head waiter"
                            );
                            self.notify_next_waiter_locked(&state);
                        }

                        log::info!(
                            "global transaction {tx_id} is waiting for the lock; no current owner; head waiter: {:?}",
                            head_waiter
                        );
                        let new_registration = registration.is_none().then(|| WaitRegistration::new(self, wait_id));
                        (notify, None, new_registration, false)
                    }
                    Some(owner) if owner == tx_id => {
                        log::warn!("global transaction {tx_id} is trying to acquire the lock it already holds. This should not happen and may indicate a bug in the caller's logic, but we'll allow it to proceed without deadlocking on itself.");
                        self.remove_waiter_locked(&mut state, &tx_id);
                        if let Some(registration) = registration.take() {
                            registration.disarm(&mut state);
                        }
                        observe_acquire_time();
                        return AcquireDisposition::Acquired(GlobalTxLockGuard::new(self.clone(), tx_id));
                    }
                    Some(owner) => {
                        let waiter = match registration.as_ref() {
                            Some(registration) => match self.registered_waiter_locked(&state, tx_id, registration) {
                                Ok(registered_waiter) => Some(registered_waiter),
                                Err(()) => None,
                            },
                            None => Some(self.ensure_waiter_locked(&mut state, tx_id)),
                        };
                        let Some((wait_id, notify)) = waiter else {
                            return AcquireDisposition::Cancelled;
                        };
                        let owner_to_wound = (tx_id < owner && state.wounded_owners.insert(owner)).then_some(owner);
                        let new_registration = registration.is_none().then(|| WaitRegistration::new(self, wait_id));
                        (notify, owner_to_wound, new_registration, false)
                    }
                }
            };
            if cancelled {
                return AcquireDisposition::Cancelled;
            }
            if let Some(new_registration) = new_registration {
                registration = Some(new_registration);
            }

            if let Some(owner) = owner_to_wound {
                if let Some((coordinator_identity, role)) = self.session_metric_labels(&tx_id) {
                    WORKER_METRICS
                        .global_tx_waiting_on_younger_owner_total
                        .with_label_values(&coordinator_identity, &role)
                        .inc();
                }
                let wound_grace_period = self.wound_grace_period;
                log::info!(
                    "global transaction {tx_id} is waiting behind younger owner {owner}; giving it {:?} to finish before wound flow",
                    wound_grace_period
                );
                let owner_finished = tokio::select! {
                    changed = wounded_rx.changed(), if !*wounded_rx.borrow() => {
                        if changed.is_ok() && *wounded_rx.borrow() {
                            return AcquireDisposition::Cancelled;
                        }
                        false
                    }
                    _ = notify.notified() => true,
                    _ = tokio::time::sleep(wound_grace_period) => false,
                };
                if owner_finished {
                    if let Some((coordinator_identity, role)) = self.session_metric_labels(&tx_id) {
                        WORKER_METRICS
                            .global_tx_younger_owner_finished_within_grace_period_total
                            .with_label_values(&coordinator_identity, &role)
                            .inc();
                    }
                    log::info!("global transaction {tx_id} observed owner {owner} finish within grace period; not triggering wound",);
                    continue;
                }

                let should_trigger_wound = {
                    let state = self.lock_state.lock().unwrap();
                    state.owner == Some(owner)
                };
                if should_trigger_wound {
                    log::info!(
                        "global transaction {tx_id} is still waiting behind younger owner {owner} after {:?}; triggering wound flow",
                        wound_grace_period
                    );
                    if self.should_wound_locally(&owner) {
                        let _ = self.wound(&owner);
                    } else {
                        log::info!(
                            "global transaction {tx_id} observed prepared participant owner {owner}; notifying coordinator without local wound"
                        );
                    }
                    tokio::spawn(on_wound(owner));
                }
            }

            tokio::select! {
                changed = wounded_rx.changed(), if !*wounded_rx.borrow() => {
                    if changed.is_ok() && *wounded_rx.borrow() {
                        return AcquireDisposition::Cancelled;
                    }
                }
                _ = notify.notified() => {
                    log::info!(
                        "global transaction {tx_id} was notified of a potential lock availability change; re-checking lock state"
                    );
                }
            }
        }
    }

    pub fn release(&self, tx_id: &GlobalTxId) {
        let mut state = self.lock_state.lock().unwrap();
        if state.owner.as_ref() == Some(tx_id) {
            log::info!("Releasing lock for tx_id {}", tx_id);
            state.owner = None;
            state.wounded_owners.remove(tx_id);
            self.notify_next_waiter_locked(&state);
        } else {
            log::warn!("Release a lock that isn't actually held. This should not happen");
        }
        self.remove_waiter_locked(&mut state, tx_id);
    }

    fn ensure_waiter_locked(&self, state: &mut LockState, tx_id: GlobalTxId) -> (u64, Arc<Notify>) {
        if let Some(wait_id) = state.waiter_ids_by_tx.get(&tx_id).copied() {
            let notify = state
                .wait_entries
                .get(&wait_id)
                .expect("wait entry must exist for registered waiter")
                .notify
                .clone();
            return (wait_id, notify);
        }

        let wait_id = state.next_wait_id;
        state.next_wait_id += 1;
        let notify = Arc::new(Notify::new());
        state.wait_entries.insert(
            wait_id,
            WaitEntry {
                tx_id,
                notify: notify.clone(),
            },
        );
        state.waiter_ids_by_tx.insert(tx_id, wait_id);
        state.waiting.insert(WaitKey { tx_id, wait_id });
        (wait_id, notify)
    }

    fn registered_waiter_locked(
        &self,
        state: &LockState,
        tx_id: GlobalTxId,
        registration: &WaitRegistration<'_>,
    ) -> Result<(u64, Arc<Notify>), ()> {
        let wait_id = registration.wait_id();
        if let Some(wait_entry) = state.wait_entries.get(&wait_id) {
            return Ok((wait_id, wait_entry.notify.clone()));
        }

        log::warn!(
            "global transaction {tx_id} lost its waiter registration while still waiting; treating acquire as cancelled"
        );
        Err(())
    }

    fn is_next_waiter_locked(&self, state: &LockState, tx_id: GlobalTxId) -> bool {
        match state.waiting.first() {
            None => true,
            Some(wait_key) => wait_key.tx_id == tx_id,
        }
    }

    fn notify_next_waiter_locked(&self, state: &LockState) {
        if let Some(wait_key) = state.waiting.first()
            && let Some(wait_entry) = state.wait_entries.get(&wait_key.wait_id)
        {
            log::info!("Notifying next waiter for tx_id {}", wait_entry.tx_id);
            wait_entry.notify.notify_one();
        }
    }

    fn remove_waiter_locked(&self, state: &mut LockState, tx_id: &GlobalTxId) {
        if let Some(wait_id) = state.waiter_ids_by_tx.remove(tx_id) {
            state.wait_entries.remove(&wait_id);
            state.waiting.remove(&WaitKey { tx_id: *tx_id, wait_id });
        }
    }

    fn remove_waiter_by_id(&self, state: &mut LockState, wait_id: u64) {
        log::info!("Removing waiter with wait_id {}", wait_id);
        let was_head = state.waiting.first().map(|w| w.wait_id) == Some(wait_id);
        if let Some(wait_entry) = state.wait_entries.remove(&wait_id) {
            log::info!("Removing waiter with wait_id {}, tx_id {}", wait_id, wait_entry.tx_id);
            state.waiter_ids_by_tx.remove(&wait_entry.tx_id);
            state.waiting.remove(&WaitKey {
                tx_id: wait_entry.tx_id,
                wait_id,
            });
            if was_head && state.owner.is_none() {
                self.notify_next_waiter_locked(state);
            }
        } else {
            log::warn!(
                "Trying to remove non-existent waiter with wait_id {}, current_owner: {:?}",
                wait_id,
                state.owner
            );
        }
    }

    fn should_wound_locally(&self, tx_id: &GlobalTxId) -> bool {
        self.get_session(tx_id)
            .map(|session| !(session.role == GlobalTxRole::Participant && session.state() == GlobalTxState::Prepared))
            .unwrap_or(true)
    }

    fn is_terminalish(&self, tx_id: &GlobalTxId) -> bool {
        let Some(session) = self.get_session(tx_id) else {
            return true;
        };
        session.is_wounded()
            || matches!(
                session.state(),
                GlobalTxState::Committed | GlobalTxState::Aborted | GlobalTxState::Aborting
            )
    }

    fn prune_stale_head_waiters_locked(&self, state: &mut LockState) {
        while let Some(wait_key) = state.waiting.first().copied() {
            let tx_id = wait_key.tx_id;
            if self.is_terminalish(&tx_id) {
                let session_state = self.get_session(&tx_id).map(|session| session.state());
                let wounded = self.is_wounded(&tx_id);
                log::warn!(
                    "pruning stale head waiter {tx_id}: state={session_state:?} wounded={wounded} while owner is None"
                );
                self.remove_waiter_by_id(state, wait_key.wait_id);
                continue;
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AcquireDisposition, GlobalTxManager};
    use crate::identity::Identity;
    use spacetimedb_lib::{GlobalTxId, Timestamp};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    fn tx_id(ts: i64, db_byte: u8, nonce: u32) -> GlobalTxId {
        GlobalTxId::new(
            Timestamp::from_micros_since_unix_epoch(ts),
            Identity::from_byte_array([db_byte; 32]),
            nonce,
            0,
        )
    }

    #[test]
    fn manager_uses_configured_wound_grace_period() {
        let manager = GlobalTxManager::new(Duration::from_millis(42));
        assert_eq!(manager.wound_grace_period(), Duration::from_millis(42));
    }

    #[test]
    fn older_requester_wounds_younger_owner() {
        let manager = Arc::new(GlobalTxManager::default());
        let younger = tx_id(20, 2, 0);
        let older = tx_id(10, 1, 0);
        manager.ensure_session(younger, super::GlobalTxRole::Participant, younger.creator_db);
        manager.ensure_session(older, super::GlobalTxRole::Participant, older.creator_db);

        let rt = Runtime::new().unwrap();
        let younger_guard = match rt.block_on(manager.acquire(younger, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("younger tx should acquire immediately"),
        };

        let manager_for_task = manager.clone();
        let older_task = rt.spawn(async move {
            match manager_for_task.acquire(older, |_| async {}).await {
                AcquireDisposition::Acquired(_guard) => true,
                AcquireDisposition::Cancelled => false,
            }
        });
        std::thread::sleep(Duration::from_millis(25));
        assert!(manager.is_wounded(&younger));
        drop(younger_guard);
        assert!(matches!(rt.block_on(older_task).expect("task should complete"), true));
    }

    #[test]
    fn younger_owner_finishing_within_grace_period_is_not_wounded() {
        let manager = Arc::new(GlobalTxManager::default());
        let younger = tx_id(20, 2, 0);
        let older = tx_id(10, 1, 0);
        manager.ensure_session(younger, super::GlobalTxRole::Participant, younger.creator_db);
        manager.ensure_session(older, super::GlobalTxRole::Participant, older.creator_db);

        let rt = Runtime::new().unwrap();
        let younger_guard = match rt.block_on(manager.acquire(younger, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("younger tx should acquire immediately"),
        };

        let manager_for_task = manager.clone();
        let older_task = rt.spawn(async move {
            match manager_for_task.acquire(older, |_| async {}).await {
                AcquireDisposition::Acquired(_guard) => true,
                AcquireDisposition::Cancelled => false,
            }
        });

        std::thread::sleep(Duration::from_millis(5));
        drop(younger_guard);

        assert!(matches!(rt.block_on(older_task).expect("task should complete"), true));
        assert!(!manager.is_wounded(&younger));
    }

    #[test]
    fn younger_requester_waits_behind_older_owner() {
        let manager = Arc::new(GlobalTxManager::default());
        let older = tx_id(10, 1, 0);
        let younger = tx_id(20, 2, 0);
        manager.ensure_session(older, super::GlobalTxRole::Participant, older.creator_db);
        manager.ensure_session(younger, super::GlobalTxRole::Participant, younger.creator_db);
        let rt = Runtime::new().unwrap();

        let older_guard = match rt.block_on(manager.acquire(older, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("older tx should acquire immediately"),
        };
        let wait = rt.block_on(async {
            tokio::time::timeout(Duration::from_millis(25), manager.acquire(younger, |_| async {})).await
        });
        assert!(wait.is_err());
        drop(older_guard);
    }

    #[test]
    fn waiter_acquires_after_release() {
        let manager = Arc::new(GlobalTxManager::default());
        let owner = tx_id(10, 1, 0);
        let waiter = tx_id(20, 2, 0);
        manager.ensure_session(owner, super::GlobalTxRole::Participant, owner.creator_db);
        manager.ensure_session(waiter, super::GlobalTxRole::Participant, waiter.creator_db);
        let rt = Runtime::new().unwrap();

        let owner_guard = match rt.block_on(manager.acquire(owner, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("owner should acquire immediately"),
        };

        let manager_for_thread = manager.clone();
        let handle = std::thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            match rt.block_on(manager_for_thread.acquire(waiter, |_| async {})) {
                AcquireDisposition::Acquired(_guard) => {}
                AcquireDisposition::Cancelled => panic!("waiter should acquire after release"),
            }
        });

        std::thread::sleep(Duration::from_millis(25));
        drop(owner_guard);
        handle.join().unwrap();
    }

    #[test]
    fn wound_is_idempotent() {
        let manager = Arc::new(GlobalTxManager::default());
        let tx_id = tx_id(10, 1, 0);
        let session = manager.ensure_session(tx_id, super::GlobalTxRole::Coordinator, tx_id.creator_db);

        assert!(!session.is_wounded());
        assert!(manager.wound(&tx_id).is_some());
        assert!(session.is_wounded());
        assert!(manager.wound(&tx_id).is_some());
        assert!(session.is_wounded());
    }

    #[test]
    fn wound_subscription_notifies_waiter() {
        let manager = Arc::new(GlobalTxManager::default());
        let tx_id = tx_id(10, 1, 0);
        let _session = manager.ensure_session(tx_id, super::GlobalTxRole::Coordinator, tx_id.creator_db);
        let mut wounded_rx = manager.subscribe_wounded(&tx_id).expect("session should exist");

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let notifier = async {
                if !*wounded_rx.borrow() {
                    wounded_rx.changed().await.expect("sender should still exist");
                }
                *wounded_rx.borrow()
            };

            let trigger = async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                manager.wound(&tx_id).expect("session should still exist");
            };

            let (wounded, ()) = tokio::join!(notifier, trigger);
            assert!(wounded);
        });
    }

    #[test]
    fn prepared_participant_only_signals_coordinator() {
        let manager = Arc::new(GlobalTxManager::default());
        let owner = tx_id(20, 2, 0);
        let older = tx_id(10, 1, 0);
        let owner_session = manager.ensure_session(owner, super::GlobalTxRole::Participant, owner.creator_db);
        owner_session.set_state(super::GlobalTxState::Prepared);
        manager.ensure_session(older, super::GlobalTxRole::Participant, older.creator_db);

        let rt = Runtime::new().unwrap();
        let owner_guard = match rt.block_on(manager.acquire(owner, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("owner should acquire immediately"),
        };

        let coordinator_wounded = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (wound_tx, wound_rx) = std::sync::mpsc::channel();
        let flag = coordinator_wounded.clone();
        let manager_for_task = manager.clone();
        let older_task = rt.spawn(async move {
            match manager_for_task
                .acquire(older, move |_| {
                    let flag = flag.clone();
                    let wound_tx = wound_tx.clone();
                    async move {
                        flag.store(true, Ordering::SeqCst);
                        let _ = wound_tx.send(());
                    }
                })
                .await
            {
                AcquireDisposition::Acquired(_guard) => true,
                AcquireDisposition::Cancelled => false,
            }
        });

        wound_rx
            .recv_timeout(Duration::from_millis(50))
            .expect("coordinator should be notified");
        assert!(coordinator_wounded.load(Ordering::SeqCst));
        assert!(!manager.is_wounded(&owner));
        drop(owner_guard);
        assert!(rt.block_on(older_task).expect("task should complete"));
    }

    #[test]
    fn wounded_waiter_is_cancelled() {
        let manager = Arc::new(GlobalTxManager::default());
        let owner = tx_id(10, 1, 0);
        let waiter = tx_id(20, 2, 0);
        manager.ensure_session(owner, super::GlobalTxRole::Participant, owner.creator_db);
        manager.ensure_session(waiter, super::GlobalTxRole::Participant, waiter.creator_db);

        let rt = Runtime::new().unwrap();
        let owner_guard = match rt.block_on(manager.acquire(owner, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("owner should acquire immediately"),
        };

        let manager_for_task = manager.clone();
        let waiter_task = rt.spawn(async move {
            matches!(
                manager_for_task.acquire(waiter, |_| async {}).await,
                AcquireDisposition::Cancelled
            )
        });
        std::thread::sleep(Duration::from_millis(10));
        manager.wound(&waiter).expect("waiter session should exist");
        drop(owner_guard);

        assert!(matches!(rt.block_on(waiter_task).expect("task should complete"), true));
    }

    #[test]
    fn pruned_waiter_is_cancelled_instead_of_panicking() {
        let manager = Arc::new(GlobalTxManager::default());
        let owner = tx_id(20, 2, 0);
        let waiter = tx_id(10, 1, 0);
        manager.ensure_session(owner, super::GlobalTxRole::Participant, owner.creator_db);
        manager.ensure_session(waiter, super::GlobalTxRole::Participant, waiter.creator_db);

        let rt = Runtime::new().unwrap();
        let owner_guard = match rt.block_on(manager.acquire(owner, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("owner should acquire immediately"),
        };

        let manager_for_task = manager.clone();
        let waiter_task = rt.spawn(async move {
            matches!(
                manager_for_task.acquire(waiter, |_| async {}).await,
                AcquireDisposition::Cancelled
            )
        });

        let deadline = std::time::Instant::now() + Duration::from_millis(100);
        while std::time::Instant::now() < deadline {
            if manager
                .lock_state
                .lock()
                .unwrap()
                .waiter_ids_by_tx
                .contains_key(&waiter)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            manager
                .lock_state
                .lock()
                .unwrap()
                .waiter_ids_by_tx
                .contains_key(&waiter),
            "waiter should be registered before pruning it",
        );
        manager.mark_state(&waiter, super::GlobalTxState::Aborting);
        drop(owner_guard);

        assert!(rt.block_on(waiter_task).expect("task should complete"));
    }
}
