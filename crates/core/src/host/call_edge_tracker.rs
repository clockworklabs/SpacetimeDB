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
/// Implementations:
///
/// - **Standalone** -- [`InMemoryCallEdgeTracker`] maintains an in-memory graph
///   and runs cycle detection locally. No network I/O.
///
/// - **Cluster** -- Calls a reducer on the control database that inserts the edge
///   and runs cycle detection. Returns `Err` if a cycle is found.
use spacetimedb_lib::Identity;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

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

    /// Set the base URL for reaching the control DB (cloud only).
    /// Default: no-op. Overridden by `CloudCallEdgeTracker`.
    fn set_base_url(&self, _url: &str) {}
}

/// In-memory call edge tracker with cycle detection.
///
/// Suitable for standalone (single-node) deployments where all databases
/// share the same process. Maintains an adjacency list of active call edges
/// and checks for cycles via DFS on each registration.
pub struct InMemoryCallEdgeTracker {
    state: Mutex<EdgeState>,
}

struct EdgeState {
    /// call_id -> (caller, callee)
    edges: HashMap<String, (Identity, Identity)>,
    /// caller -> set of callees (adjacency list for DFS)
    graph: HashMap<Identity, HashSet<Identity>>,
}

impl InMemoryCallEdgeTracker {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(EdgeState {
                edges: HashMap::new(),
                graph: HashMap::new(),
            }),
        }
    }
}

impl Default for InMemoryCallEdgeTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// DFS: is there a path from `from` to `to` in the graph?
fn has_path(graph: &HashMap<Identity, HashSet<Identity>>, from: Identity, to: Identity) -> bool {
    let mut visited = HashSet::new();
    let mut stack = vec![from];
    while let Some(current) = stack.pop() {
        if current == to {
            return true;
        }
        if !visited.insert(current) {
            continue;
        }
        if let Some(neighbors) = graph.get(&current) {
            stack.extend(neighbors);
        }
    }
    false
}

impl CallEdgeTracker for InMemoryCallEdgeTracker {
    fn register_edge(&self, call_id: &str, caller: Identity, callee: Identity) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap();

        // Check for cycle: is there a path from callee back to caller?
        if has_path(&state.graph, callee, caller) {
            anyhow::bail!(
                "cycle detected: adding edge {} -> {} would create a distributed deadlock",
                caller.to_abbreviated_hex(),
                callee.to_abbreviated_hex()
            );
        }

        // No cycle -- insert the edge.
        state.edges.insert(call_id.to_owned(), (caller, callee));
        state.graph.entry(caller).or_default().insert(callee);
        Ok(())
    }

    fn unregister_edge(&self, call_id: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap();
        if let Some((caller, callee)) = state.edges.remove(call_id) {
            if let Some(neighbors) = state.graph.get_mut(&caller) {
                neighbors.remove(&callee);
                if neighbors.is_empty() {
                    state.graph.remove(&caller);
                }
            }
        }
        Ok(())
    }

    fn unregister_all_edges(&self) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap();
        state.edges.clear();
        state.graph.clear();
        Ok(())
    }
}
