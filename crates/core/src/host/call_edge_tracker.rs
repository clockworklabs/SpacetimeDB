// TODO: Consolidate with `ControlStateWriteAccess` once that trait is moved
// to a crate that `core` can depend on (currently in `client-api`, which
// depends on `core` -- circular dependency). The edge tracking methods should
// live on `ControlStateWriteAccess` since that is the standard interface for
// interacting with the control database.

/// Trait for tracking cross-database call edges for distributed deadlock detection.
///
/// Before making a cross-database reducer call, the caller registers an edge
/// A -> B (caller -> callee). If this would create a cycle in the call graph,
/// the registration fails with an error, indicating a potential distributed deadlock.
///
/// Methods are synchronous (blocking) because they are called from the WASM
/// executor thread, which must not use async I/O.
///
/// Implementations differ between deployment modes:
///
/// - **Standalone** -- [`NoopCallEdgeTracker`] always returns `Ok(())`.
///   Single-node deployments cannot have distributed deadlocks.
///
/// - **Cluster** -- Calls a reducer on the control database that inserts the edge
///   and runs cycle detection. Returns `Err` if a cycle is found.
use spacetimedb_lib::Identity;

pub trait CallEdgeTracker: Send + Sync + 'static {
    /// Register a call edge: `caller` is about to call `callee`.
    ///
    /// Returns `Ok(())` if the edge was registered (no cycle).
    /// Returns `Err` if registering this edge would create a cycle.
    fn register_edge(&self, call_id: &str, caller: Identity, callee: Identity) -> anyhow::Result<()>;

    /// Unregister a call edge after the call completes (success or failure).
    fn unregister_edge(&self, call_id: &str) -> anyhow::Result<()>;

    /// Unregister all edges for this node (crash cleanup on startup).
    fn unregister_all_edges(&self) -> anyhow::Result<()>;
}

/// No-op implementation for standalone (single-node) deployments.
pub struct NoopCallEdgeTracker;

impl CallEdgeTracker for NoopCallEdgeTracker {
    fn register_edge(&self, _call_id: &str, _caller: Identity, _callee: Identity) -> anyhow::Result<()> {
        Ok(())
    }

    fn unregister_edge(&self, _call_id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn unregister_all_edges(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
