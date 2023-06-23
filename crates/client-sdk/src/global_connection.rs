use crate::background_connection::BackgroundDbConnection;
use crate::callbacks::{CredentialStore, DbCallbacks, ReducerCallbacks};
use crate::client_cache::ClientCache;
use anyhow::{anyhow, Result};
use std::{
    cell::{Ref, RefCell},
    marker::PhantomData,
    sync::{Arc, RwLock},
};

pub(crate) static CONNECTION: RwLock<Option<BackgroundDbConnection>> = RwLock::new(None);

/// Invoke `f` with `CONNECTION` locked.
///
/// Calls to this function are generated in the `connect` function in `mod.rs` generated
/// by the Spacetime CLI. Users should not call this function directly.
pub fn with_connection<Res>(f: impl FnOnce(&mut Option<BackgroundDbConnection>) -> Res) -> Res {
    let mut connection = CONNECTION.write().expect("CONNECTION RwLock is poisoned");
    f(&mut connection)
}

/// If currently connected, invoke `f` with `CONNECTION` locked. If not connected, return an error.
pub(crate) fn try_with_connection<Res>(f: impl FnOnce(&BackgroundDbConnection) -> Res) -> Result<Res> {
    let connection = CONNECTION.read().expect("CONNECTION RwLock is poisoned");
    if let Some(connection) = &*connection {
        Ok(f(connection))
    } else {
        Err(anyhow!("Not connected"))
    }
}

/// If `CURRENT_STATE` is bound in this thread, invoke `f` with the current state.
/// Otherwise, invoke `f` with `CONNECTION`'s client cache.
/// Return an error if not connected.
pub(crate) fn try_with_client_cache<Res>(f: impl FnOnce(&ClientCache) -> Res) -> Result<Res> {
    let state = current_or_global_state()?;
    Ok(f(&state))
}

/// If currently connected, invoke `f` with the current connection's `ReducerCallbacks`. If not
/// connected, return an error.
pub(crate) fn try_with_reducer_callbacks<Res>(f: impl FnOnce(&mut ReducerCallbacks) -> Res) -> Result<Res> {
    try_with_connection(|connection| {
        log::info!("Acquiring ReducerCallbacks Mutex");
        let mut callbacks = connection
            .reducer_callbacks
            .lock()
            .expect("ReducerCallbacks Mutex is poisoned");
        log::info!("Got ReducerCallbacks Mutex");
        f(&mut callbacks)
    })
}

/// If currently connected, invoke `f` with the current connection's `CredentialStore`. If not
/// connected, return an error.
pub(crate) fn try_with_credential_store<Res>(f: impl FnOnce(&mut CredentialStore) -> Res) -> Result<Res> {
    try_with_connection(|connection| {
        log::info!("Acquiring Credentials Mutex");
        let mut credentials = connection
            .credentials
            .lock()
            .expect("CredentialStore Mutex is poisoned");
        log::info!("Got Credentials Mutex");
        f(&mut credentials)
    })
}

pub(crate) fn try_with_db_callbacks<Res>(f: impl FnOnce(&mut DbCallbacks) -> Res) -> Result<Res> {
    try_with_connection(|connection| {
        log::info!("Acquiring DbCallbacks Mutex");
        let mut db_callbacks = connection.db_callbacks.lock().expect("DbCallbacks Mutex is poisoned");
        log::info!("Got DbCallbacks Mutex");
        f(&mut db_callbacks)
    })
}

thread_local! {
    pub(crate) static CURRENT_STATE: RefCell<Option<Arc<ClientCache>>> = RefCell::new(None);
}

pub(crate) fn try_current_state() -> Option<Arc<ClientCache>> {
    CURRENT_STATE.with(|current_state| current_state.borrow().as_ref().map(Arc::clone))
}

pub(crate) fn current_or_global_state() -> Result<Arc<ClientCache>> {
    if let Some(curr) = try_current_state() {
        Ok(curr)
    } else {
        try_with_connection(|conn| {
            log::info!("Acquiring ClientCache Mutex");
            let cache = conn.client_cache.lock().expect("ClientCache Mutex is poisoned").clone();
            log::info!("Got ClientCache Mutex");
            cache
        })
    }
}

pub(crate) struct CurrentStateGuard {
    enclosing_state: Option<Arc<ClientCache>>,
    _phantom: PhantomData<Ref<'static, ClientCache>>,
}

impl Drop for CurrentStateGuard {
    fn drop(&mut self) {
        CURRENT_STATE.with(|current_state| {
            *current_state.borrow_mut() = self.enclosing_state.take();
        });
    }
}

impl CurrentStateGuard {
    pub(crate) fn with_current_state(state: Arc<ClientCache>) -> CurrentStateGuard {
        let mut temp = Some(state);
        CURRENT_STATE.with(|current_state| {
            std::mem::swap(&mut *current_state.borrow_mut(), &mut temp);
        });
        CurrentStateGuard {
            enclosing_state: temp,
            _phantom: PhantomData,
        }
    }
}
