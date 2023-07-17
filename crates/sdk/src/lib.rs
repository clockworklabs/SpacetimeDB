#[doc(hidden)]
pub mod callbacks;

pub mod identity;
pub mod reducer;
pub mod table;
use global_connection::with_connection;

// Any `#[doc(hidden)]` modules are public because code generated by the CLI's codegen
// references them, but users should not.

#[doc(hidden)]
pub use spacetimedb_client_api_messages::client_api as client_api_messages;

#[doc(hidden)]
pub mod client_cache;

#[doc(hidden)]
pub mod global_connection;

#[doc(hidden)]
pub mod websocket;

#[doc(hidden)]
pub mod background_connection;

// We re-export `spacetimedb_lib` so the cli codegen can reference it through us, rather
// than requiring downstream users to depend on it explicitly.
// TODO: determine if this should be `#[doc(hidden)]`
pub use spacetimedb_lib;

// Ditto re-exporing `log`.
// TODO: determine if this should be `#[doc(hidden)]`.
pub use log;

// Ditto re-exporting `anyhow`. This is not `#[doc(hidden)]`, because users may want to
// refer to results we return as `anyhow::Result`.
// TODO: determine if we should re-export anything.
pub use anyhow;

#[doc(hidden)]
pub use http;

#[doc(hidden)]
pub use spacetimedb_sats as sats;

/// Subscribe to a set of queries,
/// to be notified when rows which match those queries are altered.
///
/// The `queries` should be a slice of strings representing SQL queries.
///
/// A new call to `subscribe` (or [`subscribe_owned`]) will remove all previous subscriptions
/// and replace them with the new `queries`.
/// If any rows matched the previous subscribed queries but do not match the new queries,
/// those rows will be removed from the client cache,
/// and `TableType::on_delete` callbacks will be invoked for them.
///
/// `subscribe` will return an error if called before establishing a connection
/// with the autogenerated `connect` function.
/// In that case, the queries are not registered.
pub fn subscribe(queries: &[&str]) -> anyhow::Result<()> {
    with_connection(|conn| conn.subscribe(queries))
}

/// Subscribe to a set of queries,
/// to be notified when rows which match those queries are altered.
///
/// The `queries` should be a `Vec` of `String`s representing SQL queries.
///
/// A new call to `subscribe_owned` (or [`subscribe`]) will remove all previous subscriptions
/// and replace them with the new `queries`.
/// If any rows matched the previous subscribed queries but do not match the new queries,
/// those rows will be removed from the client cache,
/// and `TableType::on_delete` callbacks will be invoked for them.
///
/// `subscribe_owned` will return an error if called before establishing a connection
/// with the autogenerated `connect` function.
/// In that case, the queries are not registered.
pub fn subscribe_owned(queries: Vec<String>) -> anyhow::Result<()> {
    with_connection(|conn| conn.subscribe_owned(queries))
}
