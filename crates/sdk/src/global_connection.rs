use crate::background_connection::BackgroundDbConnection;
use crate::callbacks::{CredentialStore, DbCallbacks, ReducerCallbacks};
use crate::client_cache::{ClientCache, ClientCacheView};
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

thread_local! {
    /// The `ClientCacheView` which should be shown to the current callback, if any.
    ///
    /// While inside a callback, this will be bound by a `CurrentStateGuard`,
    /// and accesses to the client cache state (e.g. by `TableType::iter`)
    /// will inspect the `CURRENT_STATE`, rather than the most-recent state
    /// in `global_connection::CONNECTION`.
    pub(crate) static CURRENT_STATE: RefCell<Option<ClientCacheView>> = RefCell::new(None);
}

/// If `CURRENT_STATE` is bound,
/// i.e. we're in a `CurrentStateGuard` frame,
/// i.e. we're in a callback,
/// extract and return the `CURRENT_STATE`.
pub(crate) fn try_current_state() -> Option<ClientCacheView> {
    CURRENT_STATE.with(|current_state| current_state.borrow().as_ref().map(Arc::clone))
}

/// If `CURRENT_STATE` is bound,
/// i.e. we're in a `CurrentStateGuard` frame,
/// i.e. we're in a callback,
/// extract and return the `CURRENT_STATE`.
/// If `CURRENT_STATE` is unbound, i.e. we're not in a callback,
/// attempt to extract the most recent client cache state from `CONNECTION`.
/// Return an error if both `CURRENT_STATE` and `CONNECTION` are unbound.
pub(crate) fn current_or_global_state() -> Result<ClientCacheView> {
    if let Some(curr) = try_current_state() {
        Ok(curr)
    } else {
        try_with_connection(|conn| conn.client_cache.lock().expect("ClientCache Mutex is poisoned").clone())
    }
}

/// An RAII-style guard for a binding of `CURRENT_STATE`.
///
/// Upon constructing a `CurrentStateGuard` via `with_current_state`,
/// the current thread's `CURRENT_STATE` will be bound
/// to `Some` of a particular `ClientCacheView`.
///
/// Upon destructing, the previous binding of `CURRENT_STATE` will be restored.
/// As of writing (2023-06-28), `CURRENT_STATE` bindings will never be nested,
/// so the `enclosing_state` will always be `None`.
/// Storing the `enclosing_state` and restoring it on destruction
/// will allow nested bindings of `CURRENT_STATE` in the future,
/// should that become useful.
///
/// `CURRENT_STATE` is implemented as a
/// [Common Lisp special variable](http://www.lispworks.com/documentation/HyperSpec/Body/03_abaab.htm),
/// i.e. a thread local with dynamically-scoped rebinding.
///
/// Attempts to get fancy with the lifetime of a `CurrentStateGuard`,
/// e.g. constructing two and then dropping the older before the younger,
/// will likely lead to unexpected behavior.
/// If this was C++, `CurrentStateGuard` would not have move or copy constructors,
/// and if this was a user-facing API,
/// we'd probably define a macro which bound a pinned `CurrentStateGuard`
/// to prevent moving.
pub(crate) struct CurrentStateGuard {
    enclosing_state: Option<ClientCacheView>,

    /// `Ref<'static, _>` prevents `CurrentStateGuard` from implementing `Send` or `Sync`.
    /// It should not be possible to send a `CurrentStateGuard` to another thread,
    /// as it represents a view into a thread-local static.
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
    /// Bind `CURRENT_STATE` to `state`
    /// for the duration of the returned `CurrentStateGuard`'s lifetime.
    pub(crate) fn with_current_state(state: ClientCacheView) -> CurrentStateGuard {
        let enclosing_state = CURRENT_STATE.with(|current_state| current_state.borrow_mut().replace(state));
        CurrentStateGuard {
            enclosing_state,
            _phantom: PhantomData,
        }
    }
}
