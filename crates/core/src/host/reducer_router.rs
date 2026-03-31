/// Trait for resolving which node to contact when calling a reducer on another database.
///
/// Implementations differ between deployment modes:
///
/// - **Standalone / single-node** — [`LocalReducerRouter`] always returns the local node's
///   HTTP base URL.  Every database is on the same node, so there is nothing to resolve.
///
/// - **Cluster / multi-node** — `ClusterReducerRouter` (private crate) queries the control DB
///   to discover the leader replica's node and returns that node's advertise address.
///
/// The trait is object-safe (futures are boxed) so it can be stored as `Arc<dyn ReducerCallRouter>`
/// in [`crate::replica_context::ReplicaContext`] and swapped at startup.
use spacetimedb_lib::Identity;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait ReducerCallRouter: Send + Sync + 'static {
    /// Return the HTTP base URL (e.g. `"http://10.0.0.5:3000"`) of the node that
    /// is the leader for `database_identity`.
    ///
    /// The caller appends `/v1/database/{identity}/call/{reducer}` to produce the full URL.
    ///
    /// # Errors
    ///
    /// Returns an error string when the leader cannot be resolved
    /// (database not found, no leader elected yet, node has no network address, etc.).
    fn resolve_base_url<'a>(&'a self, database_identity: Identity) -> BoxFuture<'a, anyhow::Result<String>>;

    /// Blocking variant of [`Self::resolve_base_url`] for use on the WASM executor thread.
    ///
    /// Default implementation drives the async version via `futures::executor::block_on`.
    /// Implementations may override for efficiency (e.g., `LocalReducerRouter` avoids I/O).
    fn resolve_base_url_blocking(&self, database_identity: Identity) -> anyhow::Result<String> {
        futures::executor::block_on(self.resolve_base_url(database_identity))
    }
}

// Arc<dyn ReducerCallRouter> is itself a ReducerCallRouter.
impl ReducerCallRouter for Arc<dyn ReducerCallRouter> {
    fn resolve_base_url<'a>(&'a self, database_identity: Identity) -> BoxFuture<'a, anyhow::Result<String>> {
        (**self).resolve_base_url(database_identity)
    }
}

/// Single-node implementation of [`ReducerCallRouter`].
///
/// Always routes to the same fixed base URL regardless of which database is targeted.
/// Suitable for standalone (single-node) deployments where every database is local.
///
/// For cluster deployments, replace this with `ClusterReducerRouter` from the private crate.
pub struct LocalReducerRouter {
    pub base_url: String,
}

impl LocalReducerRouter {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }
}

impl ReducerCallRouter for LocalReducerRouter {
    fn resolve_base_url<'a>(&'a self, _database_identity: Identity) -> BoxFuture<'a, anyhow::Result<String>> {
        let url = self.base_url.clone();
        Box::pin(async move { Ok(url) })
    }

    fn resolve_base_url_blocking(&self, _database_identity: Identity) -> anyhow::Result<String> {
        Ok(self.base_url.clone())
    }
}
