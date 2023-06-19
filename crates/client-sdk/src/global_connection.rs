use crate::background_connection::BackgroundDbConnection;
use crate::callbacks::{CredentialStore, DbCallbacks, ReducerCallbacks};
use crate::client_cache::ClientCache;
use anyhow::{anyhow, Result};
use std::sync::RwLock;

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

/// If currently connected, invoke `f` with the current connection's `ClientCache`. If not
/// connected, return an error.
pub(crate) fn try_with_client_cache<Res>(f: impl FnOnce(&mut ClientCache) -> Res) -> Result<Res> {
    try_with_connection(|connection| {
        let mut cache = connection.client_cache.lock().expect("ClientCache Mutex is poisoned");
        f(&mut cache)
    })
}

/// If currently connected, invoke `f` with the current connection's `ReducerCallbacks`. If not
/// connected, return an error.
pub(crate) fn try_with_reducer_callbacks<Res>(f: impl FnOnce(&mut ReducerCallbacks) -> Res) -> Result<Res> {
    try_with_connection(|connection| {
        let mut callbacks = connection
            .reducer_callbacks
            .lock()
            .expect("ReducerCallbacks Mutex is poisoned");
        f(&mut callbacks)
    })
}

/// If currently connected, invoke `f` with the current connection's `CredentialStore`. If not
/// connected, return an error.
pub(crate) fn try_with_credential_store<Res>(f: impl FnOnce(&mut CredentialStore) -> Res) -> Result<Res> {
    try_with_connection(|connection| {
        let mut credentials = connection
            .credentials
            .lock()
            .expect("CredentialStore Mutex is poisoned");
        f(&mut credentials)
    })
}

pub(crate) fn try_with_db_callbacks<Res>(f: impl FnOnce(&mut DbCallbacks) -> Res) -> Result<Res> {
    try_with_connection(|connection| {
        let mut db_callbacks = connection.db_callbacks.lock().expect("DbCallbacks Mutex is poisoned");
        f(&mut db_callbacks)
    })
}
