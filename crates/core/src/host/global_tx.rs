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
    // Waiters ordered by tx_id with the oldest first, with remote waiters
    // always preferred over local waiters.
    local_waiting: BTreeSet<WaitKey>,
    remote_waiting: BTreeSet<WaitKey>,
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
            local_waiting: BTreeSet::new(),
            remote_waiting: BTreeSet::new(),
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
            let span = tracing::info_span!(
                "global_tx_lock_guard_drop",
                database_identity = %self.manager.local_database_identity,
                tx_id = %tx_id
            );
            let _enter = span.enter();
            self.manager.release(&tx_id);
        }
    }
}

pub struct GlobalTxManager {
    local_database_identity: Identity,
    sessions: Mutex<HashMap<GlobalTxId, Arc<GlobalTxSession>>>,
    prepare_to_tx: Mutex<HashMap<String, GlobalTxId>>,
    lock_state: Mutex<LockState>,
    wound_grace_period: Duration,
}

impl Default for GlobalTxManager {
    fn default() -> Self {
        Self::new(Identity::ZERO, DEFAULT_WOUND_GRACE_PERIOD)
    }
}

impl GlobalTxManager {
    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, tx_id = %tx_id))]
    fn session_metric_labels(&self, tx_id: &GlobalTxId) -> Option<(Identity, &'static str)> {
        let session = self.get_session(tx_id)?;
        let role = match session.role {
            GlobalTxRole::Coordinator => "coordinator",
            GlobalTxRole::Participant => "participant",
        };
        Some((session.coordinator_identity, role))
    }

    pub fn new(local_database_identity: Identity, wound_grace_period: Duration) -> Self {
        Self {
            local_database_identity,
            sessions: Mutex::default(),
            prepare_to_tx: Mutex::default(),
            lock_state: Mutex::default(),
            wound_grace_period,
        }
    }

    pub fn wound_grace_period(&self) -> Duration {
        self.wound_grace_period
    }

    fn is_local_tx(&self, tx_id: &GlobalTxId) -> bool {
        tx_id.creator_db == self.local_database_identity
    }

    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, tx_id = %tx_id))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, tx_id = %tx_id))]
    pub fn get_session(&self, tx_id: &GlobalTxId) -> Option<Arc<GlobalTxSession>> {
        self.sessions.lock().unwrap().get(tx_id).cloned()
    }

    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, tx_id = %tx_id))]
    pub fn remove_session(&self, tx_id: &GlobalTxId) {
        self.sessions.lock().unwrap().remove(tx_id);
    }

    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, prepare_id = prepare_id))]
    pub fn tx_for_prepare(&self, prepare_id: &str) -> Option<GlobalTxId> {
        self.prepare_to_tx.lock().unwrap().get(prepare_id).copied()
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, prepare_id),
        fields(database_identity = %self.local_database_identity, tx_id = %tx_id, prepare_id = prepare_id.as_str())
    )]
    pub fn set_prepare_mapping(&self, tx_id: GlobalTxId, prepare_id: String) {
        self.prepare_to_tx.lock().unwrap().insert(prepare_id.clone(), tx_id);
        if let Some(session) = self.get_session(&tx_id) {
            session.set_prepare_id(Some(prepare_id));
        }
    }

    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, prepare_id = prepare_id))]
    pub fn remove_prepare_mapping(&self, prepare_id: &str) -> Option<GlobalTxId> {
        let tx_id = self.prepare_to_tx.lock().unwrap().remove(prepare_id);
        if let Some(tx_id) = tx_id
            && let Some(session) = self.get_session(&tx_id)
        {
            session.set_prepare_id(None);
        }
        tx_id
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, prepare_id),
        fields(database_identity = %self.local_database_identity, tx_id = %tx_id, participant = %db_identity, prepare_id = prepare_id.as_str())
    )]
    pub fn add_participant(&self, tx_id: GlobalTxId, db_identity: Identity, prepare_id: String) {
        if let Some(session) = self.get_session(&tx_id) {
            session.add_participant(db_identity, prepare_id);
        }
    }

    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, tx_id = %tx_id, state = ?state))]
    pub fn mark_state(&self, tx_id: &GlobalTxId, state: GlobalTxState) {
        if let Some(session) = self.get_session(tx_id) {
            session.set_state(state);
        }
    }

    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, tx_id = %tx_id))]
    pub fn is_wounded(&self, tx_id: &GlobalTxId) -> bool {
        self.get_session(tx_id).map(|s| s.is_wounded()).unwrap_or(false)
    }

    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, tx_id = %tx_id))]
    pub fn subscribe_wounded(&self, tx_id: &GlobalTxId) -> Option<watch::Receiver<bool>> {
        self.get_session(tx_id).map(|s| s.subscribe_wounded())
    }

    // This should only be called by the coordinator.
    // Arguably we should have a separate state for wounded and aborted, in case we wound a remote tx before we send write the prepare.
    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, tx_id = %tx_id))]
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

    #[tracing::instrument(level = "trace", skip(self, on_wound), fields(database_identity = %self.local_database_identity, tx_id = %tx_id))]
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
                        let head_waiter = self.next_waiter_key_locked(&state).map(|wait_key| wait_key.tx_id);
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
                        let owner_to_wound =
                            (!self.is_local_tx(&tx_id) && tx_id < owner && state.wounded_owners.insert(owner))
                                .then_some(owner);
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
                let wound_grace_period = if self.is_local_tx(&owner) {
                    Duration::ZERO
                } else {
                    self.wound_grace_period
                };
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

    #[tracing::instrument(level = "trace", skip(self), fields(database_identity = %self.local_database_identity, tx_id = %tx_id))]
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
        self.insert_waiter_key_locked(state, WaitKey { tx_id, wait_id });
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
        match self.next_waiter_key_locked(state) {
            None => true,
            Some(wait_key) => wait_key.tx_id == tx_id,
        }
    }

    fn next_waiter_key_locked(&self, state: &LockState) -> Option<WaitKey> {
        state
            .remote_waiting
            .first()
            .copied()
            .or_else(|| state.local_waiting.first().copied())
    }

    fn notify_next_waiter_locked(&self, state: &LockState) {
        if let Some(wait_key) = self.next_waiter_key_locked(state)
            && let Some(wait_entry) = state.wait_entries.get(&wait_key.wait_id)
        {
            log::info!("Notifying next waiter for tx_id {}", wait_entry.tx_id);
            wait_entry.notify.notify_one();
        }
    }

    fn remove_waiter_locked(&self, state: &mut LockState, tx_id: &GlobalTxId) {
        if let Some(wait_id) = state.waiter_ids_by_tx.remove(tx_id) {
            state.wait_entries.remove(&wait_id);
            self.remove_waiter_key_locked(state, &WaitKey { tx_id: *tx_id, wait_id });
        }
    }

    fn remove_waiter_by_id(&self, state: &mut LockState, wait_id: u64) {
        log::info!("Removing waiter with wait_id {}", wait_id);
        let was_head = self.next_waiter_key_locked(state).map(|w| w.wait_id) == Some(wait_id);
        if let Some(wait_entry) = state.wait_entries.remove(&wait_id) {
            log::info!("Removing waiter with wait_id {}, tx_id {}", wait_id, wait_entry.tx_id);
            state.waiter_ids_by_tx.remove(&wait_entry.tx_id);
            self.remove_waiter_key_locked(
                state,
                &WaitKey {
                    tx_id: wait_entry.tx_id,
                    wait_id,
                },
            );
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

    fn insert_waiter_key_locked(&self, state: &mut LockState, wait_key: WaitKey) {
        if self.is_local_tx(&wait_key.tx_id) {
            state.local_waiting.insert(wait_key);
        } else {
            state.remote_waiting.insert(wait_key);
        }
    }

    fn remove_waiter_key_locked(&self, state: &mut LockState, wait_key: &WaitKey) {
        if self.is_local_tx(&wait_key.tx_id) {
            state.local_waiting.remove(wait_key);
        } else {
            state.remote_waiting.remove(wait_key);
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
        while let Some(wait_key) = self.next_waiter_key_locked(state) {
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
    use super::{AcquireDisposition, GlobalTxManager, DEFAULT_WOUND_GRACE_PERIOD};
    use crate::identity::Identity;
    use spacetimedb_lib::{GlobalTxId, Timestamp};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::runtime::Runtime;

    fn manager(local_database_identity: Identity) -> Arc<GlobalTxManager> {
        Arc::new(GlobalTxManager::new(
            local_database_identity,
            DEFAULT_WOUND_GRACE_PERIOD,
        ))
    }

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
        let manager = GlobalTxManager::new(Identity::ZERO, Duration::from_millis(42));
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
        std::thread::sleep(Duration::from_millis(50));
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
    fn local_owner_is_wounded_without_grace_period() {
        let local_db = Identity::from_byte_array([1; 32]);
        let manager = manager(local_db);
        let local_owner = tx_id(20, 1, 0);
        let remote_older = tx_id(10, 2, 0);
        manager.ensure_session(local_owner, super::GlobalTxRole::Coordinator, local_owner.creator_db);
        manager.ensure_session(remote_older, super::GlobalTxRole::Participant, remote_older.creator_db);

        let rt = Runtime::new().unwrap();
        let owner_guard = match rt.block_on(manager.acquire(local_owner, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("local owner should acquire immediately"),
        };

        let manager_for_task = manager.clone();
        let older_task = rt.spawn(async move {
            match manager_for_task.acquire(remote_older, |_| async {}).await {
                AcquireDisposition::Acquired(_guard) => true,
                AcquireDisposition::Cancelled => false,
            }
        });

        std::thread::sleep(Duration::from_millis(5));
        assert!(manager.is_wounded(&local_owner));
        drop(owner_guard);
        assert!(matches!(rt.block_on(older_task).expect("task should complete"), true));
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
    fn remote_waiter_is_prioritized_over_older_local_waiter() {
        let local_db = Identity::from_byte_array([1; 32]);
        let manager = manager(local_db);
        let owner = tx_id(5, 9, 0);
        let local_waiter = tx_id(10, 1, 0);
        let remote_waiter = tx_id(20, 2, 0);
        manager.ensure_session(owner, super::GlobalTxRole::Participant, owner.creator_db);
        manager.ensure_session(local_waiter, super::GlobalTxRole::Coordinator, local_waiter.creator_db);
        manager.ensure_session(
            remote_waiter,
            super::GlobalTxRole::Participant,
            remote_waiter.creator_db,
        );

        let rt = Runtime::new().unwrap();
        let owner_guard = match rt.block_on(manager.acquire(owner, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("owner should acquire immediately"),
        };

        let (order_tx, order_rx) = std::sync::mpsc::channel();

        let local_manager = manager.clone();
        let local_order_tx = order_tx.clone();
        let local_task = rt.spawn(async move {
            match local_manager.acquire(local_waiter, |_| async {}).await {
                AcquireDisposition::Acquired(_guard) => {
                    let _ = local_order_tx.send("local");
                    true
                }
                AcquireDisposition::Cancelled => false,
            }
        });

        let remote_manager = manager.clone();
        let remote_order_tx = order_tx.clone();
        let remote_task = rt.spawn(async move {
            match remote_manager.acquire(remote_waiter, |_| async {}).await {
                AcquireDisposition::Acquired(_guard) => {
                    let _ = remote_order_tx.send("remote");
                    true
                }
                AcquireDisposition::Cancelled => false,
            }
        });

        std::thread::sleep(Duration::from_millis(10));
        drop(owner_guard);

        assert_eq!(
            order_rx
                .recv_timeout(Duration::from_millis(100))
                .expect("first waiter should acquire"),
            "remote"
        );
        assert_eq!(
            order_rx
                .recv_timeout(Duration::from_millis(100))
                .expect("second waiter should acquire"),
            "local"
        );
        assert!(rt.block_on(local_task).expect("local task should complete"));
        assert!(rt.block_on(remote_task).expect("remote task should complete"));
    }

    #[test]
    fn remote_waiters_preserve_age_order() {
        let manager = manager(Identity::from_byte_array([1; 32]));
        let owner = tx_id(5, 9, 0);
        let older_remote = tx_id(10, 2, 0);
        let younger_remote = tx_id(20, 3, 0);
        manager.ensure_session(owner, super::GlobalTxRole::Participant, owner.creator_db);
        manager.ensure_session(older_remote, super::GlobalTxRole::Participant, older_remote.creator_db);
        manager.ensure_session(
            younger_remote,
            super::GlobalTxRole::Participant,
            younger_remote.creator_db,
        );

        let rt = Runtime::new().unwrap();
        let owner_guard = match rt.block_on(manager.acquire(owner, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("owner should acquire immediately"),
        };

        let (order_tx, order_rx) = std::sync::mpsc::channel();

        let older_manager = manager.clone();
        let older_order_tx = order_tx.clone();
        let older_task = rt.spawn(async move {
            match older_manager.acquire(older_remote, |_| async {}).await {
                AcquireDisposition::Acquired(_guard) => {
                    let _ = older_order_tx.send("older");
                    true
                }
                AcquireDisposition::Cancelled => false,
            }
        });

        let younger_manager = manager.clone();
        let younger_order_tx = order_tx.clone();
        let younger_task = rt.spawn(async move {
            match younger_manager.acquire(younger_remote, |_| async {}).await {
                AcquireDisposition::Acquired(_guard) => {
                    let _ = younger_order_tx.send("younger");
                    true
                }
                AcquireDisposition::Cancelled => false,
            }
        });

        std::thread::sleep(Duration::from_millis(10));
        drop(owner_guard);

        assert_eq!(
            order_rx
                .recv_timeout(Duration::from_millis(100))
                .expect("first remote waiter should acquire"),
            "older"
        );
        assert_eq!(
            order_rx
                .recv_timeout(Duration::from_millis(100))
                .expect("second remote waiter should acquire"),
            "younger"
        );
        assert!(rt.block_on(older_task).expect("older task should complete"));
        assert!(rt.block_on(younger_task).expect("younger task should complete"));
    }

    #[test]
    fn local_waiters_preserve_age_order_when_no_remote_waiters_exist() {
        let local_db = Identity::from_byte_array([1; 32]);
        let manager = manager(local_db);
        let owner = tx_id(5, 9, 0);
        let older_local = tx_id(10, 1, 0);
        let younger_local = tx_id(20, 1, 1);
        manager.ensure_session(owner, super::GlobalTxRole::Participant, owner.creator_db);
        manager.ensure_session(older_local, super::GlobalTxRole::Coordinator, older_local.creator_db);
        manager.ensure_session(
            younger_local,
            super::GlobalTxRole::Coordinator,
            younger_local.creator_db,
        );

        let rt = Runtime::new().unwrap();
        let owner_guard = match rt.block_on(manager.acquire(owner, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("owner should acquire immediately"),
        };

        let (order_tx, order_rx) = std::sync::mpsc::channel();

        let older_manager = manager.clone();
        let older_order_tx = order_tx.clone();
        let older_task = rt.spawn(async move {
            match older_manager.acquire(older_local, |_| async {}).await {
                AcquireDisposition::Acquired(_guard) => {
                    let _ = older_order_tx.send("older");
                    true
                }
                AcquireDisposition::Cancelled => false,
            }
        });

        let younger_manager = manager.clone();
        let younger_order_tx = order_tx.clone();
        let younger_task = rt.spawn(async move {
            match younger_manager.acquire(younger_local, |_| async {}).await {
                AcquireDisposition::Acquired(_guard) => {
                    let _ = younger_order_tx.send("younger");
                    true
                }
                AcquireDisposition::Cancelled => false,
            }
        });

        std::thread::sleep(Duration::from_millis(10));
        drop(owner_guard);

        assert_eq!(
            order_rx
                .recv_timeout(Duration::from_millis(100))
                .expect("first local waiter should acquire"),
            "older"
        );
        assert_eq!(
            order_rx
                .recv_timeout(Duration::from_millis(100))
                .expect("second local waiter should acquire"),
            "younger"
        );
        assert!(rt.block_on(older_task).expect("older task should complete"));
        assert!(rt.block_on(younger_task).expect("younger task should complete"));
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
    fn local_waiter_does_not_trigger_wound_or_callback() {
        let local_db = Identity::from_byte_array([1; 32]);
        let manager = manager(local_db);
        let owner = tx_id(20, 2, 0);
        let local_waiter = tx_id(10, 1, 0);
        manager.ensure_session(owner, super::GlobalTxRole::Participant, owner.creator_db);
        manager.ensure_session(local_waiter, super::GlobalTxRole::Coordinator, local_waiter.creator_db);

        let rt = Runtime::new().unwrap();
        let owner_guard = match rt.block_on(manager.acquire(owner, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("owner should acquire immediately"),
        };

        let callback_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let callback_flag = callback_called.clone();
        let manager_for_task = manager.clone();
        let waiter_task = rt.spawn(async move {
            match manager_for_task
                .acquire(local_waiter, move |_| {
                    let callback_flag = callback_flag.clone();
                    async move {
                        callback_flag.store(true, Ordering::SeqCst);
                    }
                })
                .await
            {
                AcquireDisposition::Acquired(_guard) => true,
                AcquireDisposition::Cancelled => false,
            }
        });

        std::thread::sleep(Duration::from_millis(50));
        assert!(!manager.is_wounded(&owner));
        assert!(!callback_called.load(Ordering::SeqCst));

        drop(owner_guard);
        assert!(rt.block_on(waiter_task).expect("waiter task should complete"));
    }

    #[test]
    fn suppressed_local_waiter_does_not_block_remote_wound() {
        let local_db = Identity::from_byte_array([1; 32]);
        let manager = manager(local_db);
        let owner = tx_id(30, 2, 0);
        let local_waiter = tx_id(10, 1, 0);
        let remote_waiter = tx_id(15, 3, 0);
        manager.ensure_session(owner, super::GlobalTxRole::Participant, owner.creator_db);
        manager.ensure_session(local_waiter, super::GlobalTxRole::Coordinator, local_waiter.creator_db);
        manager.ensure_session(
            remote_waiter,
            super::GlobalTxRole::Participant,
            remote_waiter.creator_db,
        );

        let rt = Runtime::new().unwrap();
        let owner_guard = match rt.block_on(manager.acquire(owner, |_| async {})) {
            AcquireDisposition::Acquired(guard) => guard,
            AcquireDisposition::Cancelled => panic!("owner should acquire immediately"),
        };

        let local_callback_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let local_callback_flag = local_callback_called.clone();
        let local_manager = manager.clone();
        let local_task = rt.spawn(async move {
            match local_manager
                .acquire(local_waiter, move |_| {
                    let local_callback_flag = local_callback_flag.clone();
                    async move {
                        local_callback_flag.store(true, Ordering::SeqCst);
                    }
                })
                .await
            {
                AcquireDisposition::Acquired(_guard) => true,
                AcquireDisposition::Cancelled => false,
            }
        });

        let remote_manager = manager.clone();
        let remote_task = rt.spawn(async move {
            match remote_manager.acquire(remote_waiter, |_| async {}).await {
                AcquireDisposition::Acquired(_guard) => true,
                AcquireDisposition::Cancelled => false,
            }
        });

        std::thread::sleep(Duration::from_millis(50));
        assert!(!local_callback_called.load(Ordering::SeqCst));
        assert!(manager.is_wounded(&owner));

        drop(owner_guard);
        assert!(rt.block_on(remote_task).expect("remote task should complete"));
        assert!(rt.block_on(local_task).expect("local task should complete"));
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
