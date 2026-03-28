//! Naive binding for calling reducers on remote SpacetimeDB databases.
//!
//! Call a reducer on another database using [`call_reducer_on_db`].
//!
//! The args must be BSATN-encoded. The response body is raw bytes returned by
//! the remote database's HTTP handler. An HTTP status >= 400 does not cause an
//! `Err` return; only a transport failure (connection refused, timeout, …) does.
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
//!         Ok((status, body)) => log::info!("status={status} body={body:?}"),
//!         Err(msg) => log::error!("transport error: {msg}"),
//!     }
//! }
//! ```

use crate::{
    rt::{read_bytes_source_as, read_bytes_source_into},
    Identity, IterBuf,
};

/// Call a reducer on a remote database.
///
/// - `database_identity`: the target database.
/// - `reducer_name`: the name of the reducer to invoke (must be valid UTF-8).
/// - `args`: BSATN-encoded reducer arguments.
///
/// Returns `Ok((status, body))` on any transport success (including HTTP errors like 400/500).
/// Returns `Err(message)` on transport failure (connection refused, timeout, …).
pub fn call_reducer_on_db(
    database_identity: Identity,
    reducer_name: &str,
    args: &[u8],
) -> Result<(u16, Vec<u8>), String> {
    let identity_bytes = database_identity.to_byte_array();
    match spacetimedb_bindings_sys::call_reducer_on_db(identity_bytes, reducer_name, args) {
        Ok((status, body_source)) => {
            // INVALID signals an empty body (host optimization to avoid allocation).
            let body = if body_source == spacetimedb_bindings_sys::raw::BytesSource::INVALID {
                Vec::new()
            } else {
                let mut buf = IterBuf::take();
                read_bytes_source_into(body_source, &mut buf);
                buf.to_vec()
            };
            Ok((status, body))
        }
        Err(err_source) => {
            let message = read_bytes_source_as::<String>(err_source);
            Err(message)
        }
    }
}
