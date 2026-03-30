//! Binding for calling reducers on remote SpacetimeDB databases.
//!
//! Call a reducer on another database using [`call_reducer_on_db`].
//!
//! The args must be BSATN-encoded. Returns `Ok(())` when the remote reducer
//! ran and succeeded, or one of the [`RemoteCallError`] variants on failure.
//!
//! # Example
//!
//! ```no_run
//! use spacetimedb::{remote_reducer, Identity};
//!
//! #[spacetimedb::reducer]
//! fn call_remote(ctx: &spacetimedb::ReducerContext, target: Identity) {
//!     // Empty BSATN args for a zero-argument reducer.
//!     let args = spacetimedb::bsatn::to_vec(&()).unwrap();
//!     match remote_reducer::call_reducer_on_db(target, "my_reducer", &args) {
//!         Ok(()) => log::info!("remote reducer succeeded"),
//!         Err(remote_reducer::RemoteCallError::Failed(msg)) => log::error!("reducer failed: {msg}"),
//!         Err(remote_reducer::RemoteCallError::NotFound(msg)) => log::error!("not found: {msg}"),
//!         Err(remote_reducer::RemoteCallError::Unreachable(msg)) => log::error!("unreachable: {msg}"),
//!         Err(remote_reducer::RemoteCallError::Wounded(msg)) => log::warn!("wounded: {msg}"),
//!     }
//! }
//! ```

use crate::{rt::read_bytes_source_into, Identity, IterBuf};

/// Error returned by [`call_reducer_on_db`].
#[derive(Debug)]
pub enum RemoteCallError {
    /// The remote reducer ran but returned an error. Contains the error message from the server.
    Failed(String),
    /// The target database or reducer does not exist (HTTP 404).
    NotFound(String),
    /// The call could not be delivered (connection refused, timeout, network error, etc.).
    Unreachable(String),
    /// The distributed transaction was wounded by an older transaction.
    Wounded(String),
}

impl core::fmt::Display for RemoteCallError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            RemoteCallError::Failed(msg) => write!(f, "remote reducer failed: {msg}"),
            RemoteCallError::NotFound(msg) => write!(f, "remote database or reducer not found: {msg}"),
            RemoteCallError::Unreachable(msg) => write!(f, "remote database unreachable: {msg}"),
            RemoteCallError::Wounded(msg) => write!(f, "{msg}"),
        }
    }
}

pub fn into_reducer_error_message(error: RemoteCallError) -> String {
    match error {
        RemoteCallError::Wounded(msg) => crate::rt::encode_wounded_error_message(msg),
        other => other.to_string(),
    }
}

/// Call a reducer on a remote database.
///
/// - `database_identity`: the target database.
/// - `reducer_name`: the name of the reducer to invoke (must be valid UTF-8).
/// - `args`: BSATN-encoded reducer arguments.
///
/// Returns `Ok(bytes)` when the remote reducer ran and succeeded, with `bytes` being the reducer's output.
/// Returns `Err(RemoteCallError::Failed(msg))` when the reducer ran but returned an error.
/// Returns `Err(RemoteCallError::NotFound(msg))` when the database or reducer does not exist.
/// Returns `Err(RemoteCallError::Unreachable(msg))` on transport failure (connection refused, timeout, …).
/// Returns `Err(RemoteCallError::Wounded(msg))` if the surrounding distributed transaction was wounded.
pub fn call_reducer_on_db(
    database_identity: Identity,
    reducer_name: &str,
    args: &[u8],
) -> Result<Vec<u8>, RemoteCallError> {
    let identity_bytes = database_identity.to_byte_array();
    match spacetimedb_bindings_sys::call_reducer_on_db(identity_bytes, reducer_name, args) {
        Ok((status, body_source)) => {
            if status < 300 {
                let mut out = Vec::new();
                read_bytes_source_into(body_source, &mut out);
                return Ok(out);
            }
            // Decode the response body as the error message.
            let msg = if body_source == spacetimedb_bindings_sys::raw::BytesSource::INVALID {
                String::new()
            } else {
                let mut buf = IterBuf::take();
                read_bytes_source_into(body_source, &mut buf);
                String::from_utf8_lossy(&buf).into_owned()
            };
            if status == 404 {
                Err(RemoteCallError::NotFound(msg))
            } else {
                Err(RemoteCallError::Failed(msg))
            }
        }
        Err((errno, err_source)) => {
            use crate::rt::read_bytes_source_as;
            let msg = read_bytes_source_as::<String>(err_source);
            Err(if errno == spacetimedb_bindings_sys::Errno::WOUNDED_TRANSACTION {
                RemoteCallError::Wounded(msg)
            } else {
                RemoteCallError::Unreachable(msg)
            })
        }
    }
}

/// Call a reducer on a remote database using the 2PC prepare protocol.
///
/// This is the 2PC variant of [`call_reducer_on_db`]. It calls the target database's
/// `/prepare/{reducer}` endpoint. On success, the runtime stores the prepare_id internally.
/// After the coordinator's reducer commits, all participants are committed automatically.
/// If the coordinator's reducer fails (panics or returns Err), all participants are aborted.
///
/// Returns and errors are identical to [`call_reducer_on_db`].
pub fn call_reducer_on_db_2pc(
    database_identity: Identity,
    reducer_name: &str,
    args: &[u8],
) -> Result<(), RemoteCallError> {
    let identity_bytes = database_identity.to_byte_array();
    match spacetimedb_bindings_sys::call_reducer_on_db_2pc(identity_bytes, reducer_name, args) {
        Ok((status, body_source)) => {
            if status < 300 {
                return Ok(());
            }
            let msg = if body_source == spacetimedb_bindings_sys::raw::BytesSource::INVALID {
                String::new()
            } else {
                let mut buf = IterBuf::take();
                read_bytes_source_into(body_source, &mut buf);
                String::from_utf8_lossy(&buf).into_owned()
            };
            if status == 404 {
                Err(RemoteCallError::NotFound(msg))
            } else {
                Err(RemoteCallError::Failed(msg))
            }
        }
        Err((errno, err_source)) => {
            use crate::rt::read_bytes_source_as;
            let msg = read_bytes_source_as::<String>(err_source);
            Err(if errno == spacetimedb_bindings_sys::Errno::WOUNDED_TRANSACTION {
                RemoteCallError::Wounded(msg)
            } else {
                RemoteCallError::Unreachable(msg)
            })
        }
    }
}
