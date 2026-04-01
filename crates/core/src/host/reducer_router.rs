/// Trait for resolving which node to contact when calling a reducer on another database.
///
/// Implementations differ between deployment modes:
///
/// - **Standalone / single-node** -- [`LocalReducerRouter`] always returns the local node's
///   HTTP base URL.  Every database is on the same node, so there is nothing to resolve.
///
/// - **Cluster / multi-node** -- `CachingResolver` (private crate) queries a local cache
///   populated from the control DB to discover the leader replica's node address.
use spacetimedb_lib::Identity;
use std::sync::Arc;

pub trait ReducerCallRouter: Send + Sync + 'static {
    /// Return the HTTP base URL (e.g. `"http://10.0.0.5:3000"`) of the node that
    /// is the leader for `database_identity`.
    ///
    /// The caller appends `/v1/database/{identity}/call/{reducer}` to produce the full URL.
    ///
    /// # Errors
    ///
    /// Returns an error when the leader cannot be resolved
    /// (database not found, no leader elected yet, node has no network address, etc.).
    fn resolve_base_url(&self, database_identity: Identity) -> anyhow::Result<String>;
}

// Arc<dyn ReducerCallRouter> is itself a ReducerCallRouter.
impl ReducerCallRouter for Arc<dyn ReducerCallRouter> {
    fn resolve_base_url(&self, database_identity: Identity) -> anyhow::Result<String> {
        (**self).resolve_base_url(database_identity)
    }
}

/// Single-node implementation of [`ReducerCallRouter`].
///
/// Always routes to the same fixed base URL regardless of which database is targeted.
/// Suitable for standalone (single-node) deployments where every database is local.
///
/// For cluster deployments, replace this with `CachingResolver` from the private crate.
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
    fn resolve_base_url(&self, _database_identity: Identity) -> anyhow::Result<String> {
        Ok(self.base_url.clone())
    }
}
