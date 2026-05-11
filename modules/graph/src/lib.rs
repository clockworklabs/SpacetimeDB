use spacetimedb::{log_stopwatch::LogStopwatch, ProcedureContext, ReducerContext, Table};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Tables — data model
// ---------------------------------------------------------------------------

#[spacetimedb::table(accessor = vertex, public)]
pub struct Vertex {
    #[primary_key]
    #[auto_inc]
    id: u64,
    label: String,
    properties: String,
}

#[spacetimedb::table(
    accessor = edge,
    public,
    index(accessor = idx_start, btree(columns = [start_id])),
    index(accessor = idx_end, btree(columns = [end_id]))
)]
pub struct Edge {
    #[primary_key]
    #[auto_inc]
    id: u64,
    start_id: u64,
    end_id: u64,
    edge_type: String,
    properties: String,
}

// ---------------------------------------------------------------------------
// Vertex CRUD
// ---------------------------------------------------------------------------

#[spacetimedb::reducer]
pub fn create_vertex(ctx: &ReducerContext, label: String, properties: String) {
    ctx.db.vertex().insert(Vertex {
        id: 0,
        label,
        properties,
    });
}

#[spacetimedb::reducer]
pub fn update_vertex(
    ctx: &ReducerContext,
    id: u64,
    label: String,
    properties: String,
) -> Result<(), String> {
    if ctx.db.vertex().id().find(id).is_none() {
        return Err(format!("Vertex {id} not found"));
    }
    ctx.db.vertex().id().update(Vertex {
        id,
        label,
        properties,
    });
    Ok(())
}

#[spacetimedb::reducer]
pub fn delete_vertex(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    let connected_edges: Vec<Edge> = ctx
        .db
        .edge()
        .idx_start()
        .filter(&id)
        .chain(ctx.db.edge().idx_end().filter(&id))
        .collect();
    for e in connected_edges {
        ctx.db.edge().id().delete(e.id);
    }
    if ctx.db.vertex().id().delete(id) {
        Ok(())
    } else {
        Err(format!("Vertex {id} not found"))
    }
}

// ---------------------------------------------------------------------------
// Edge CRUD
// ---------------------------------------------------------------------------

#[spacetimedb::reducer]
pub fn create_edge(
    ctx: &ReducerContext,
    start_id: u64,
    end_id: u64,
    edge_type: String,
    properties: String,
) -> Result<(), String> {
    if ctx.db.vertex().id().find(start_id).is_none() {
        return Err(format!("Start vertex {start_id} not found"));
    }
    if ctx.db.vertex().id().find(end_id).is_none() {
        return Err(format!("End vertex {end_id} not found"));
    }
    ctx.db.edge().insert(Edge {
        id: 0,
        start_id,
        end_id,
        edge_type,
        properties,
    });
    Ok(())
}

#[spacetimedb::reducer]
pub fn update_edge(
    ctx: &ReducerContext,
    id: u64,
    start_id: u64,
    end_id: u64,
    edge_type: String,
    properties: String,
) -> Result<(), String> {
    if ctx.db.edge().id().find(id).is_none() {
        return Err(format!("Edge {id} not found"));
    }
    if ctx.db.vertex().id().find(start_id).is_none() {
        return Err(format!("Start vertex {start_id} not found"));
    }
    if ctx.db.vertex().id().find(end_id).is_none() {
        return Err(format!("End vertex {end_id} not found"));
    }
    ctx.db.edge().id().update(Edge {
        id,
        start_id,
        end_id,
        edge_type,
        properties,
    });
    Ok(())
}

#[spacetimedb::reducer]
pub fn delete_edge(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    if ctx.db.edge().id().delete(id) {
        Ok(())
    } else {
        Err(format!("Edge {id} not found"))
    }
}

// ---------------------------------------------------------------------------
// Tables — traversal results
// ---------------------------------------------------------------------------

#[spacetimedb::table(accessor = traversal_result, public)]
pub struct TraversalResult {
    #[primary_key]
    #[auto_inc]
    id: u64,
    run_tag: String,
    vertex_id: u64,
    depth: u32,
}

#[spacetimedb::table(accessor = path_result, public)]
pub struct PathResult {
    #[primary_key]
    #[auto_inc]
    id: u64,
    run_tag: String,
    step: u32,
    vertex_id: u64,
}

// ---------------------------------------------------------------------------
// Traversal reducers
// ---------------------------------------------------------------------------

fn outgoing_neighbors(ctx: &ReducerContext, vertex_id: u64) -> Vec<u64> {
    ctx.db
        .edge()
        .idx_start()
        .filter(&vertex_id)
        .map(|e| e.end_id)
        .collect()
}

/// Build an in-memory adjacency map from the entire Edge table.
/// This avoids per-vertex B-tree index lookups during traversal.
fn build_adjacency_map(ctx: &ReducerContext) -> HashMap<u64, Vec<u64>> {
    let mut adj: HashMap<u64, Vec<u64>> = HashMap::new();
    for edge in ctx.db.edge().iter() {
        adj.entry(edge.start_id).or_default().push(edge.end_id);
    }
    adj
}
#[spacetimedb::reducer]
pub fn bfs(ctx: &ReducerContext, start_id: u64, max_depth: u32, run_tag: String) -> Result<(), String> {
    if ctx.db.vertex().id().find(start_id).is_none() {
        return Err(format!("Start vertex {start_id} not found"));
    }
let adj = build_adjacency_map(ctx);
    let visited = graph_algo::bfs(start_id, max_depth, |v| adj.get(&v).cloned().unwrap_or_default());
    for v in visited {
        ctx.db.traversal_result().insert(TraversalResult {
            id: 0,
            run_tag: run_tag.clone(),
            vertex_id: v.vertex_id,
            depth: v.depth,
        });
    }
    Ok(())
}

#[spacetimedb::reducer]
pub fn dfs(ctx: &ReducerContext, start_id: u64, max_depth: u32, run_tag: String) -> Result<(), String> {
    if ctx.db.vertex().id().find(start_id).is_none() {
        return Err(format!("Start vertex {start_id} not found"));
    }
let adj = build_adjacency_map(ctx);
    let visited = graph_algo::dfs(start_id, max_depth, |v| adj.get(&v).cloned().unwrap_or_default());
    for v in visited {
        ctx.db.traversal_result().insert(TraversalResult {
            id: 0,
            run_tag: run_tag.clone(),
            vertex_id: v.vertex_id,
            depth: v.depth,
        });
    }
    Ok(())
}

#[spacetimedb::reducer]
pub fn shortest_path(
    ctx: &ReducerContext,
    start_id: u64,
    end_id: u64,
    run_tag: String,
) -> Result<(), String> {
    if ctx.db.vertex().id().find(start_id).is_none() {
        return Err(format!("Start vertex {start_id} not found"));
    }
    if ctx.db.vertex().id().find(end_id).is_none() {
        return Err(format!("End vertex {end_id} not found"));
    }
let adj = build_adjacency_map(ctx);
    let path = graph_algo::shortest_path(start_id, end_id, |v| adj.get(&v).cloned().unwrap_or_default());
    for (step, &vid) in path.iter().enumerate() {
        ctx.db.path_result().insert(PathResult {
            id: 0,
            run_tag: run_tag.clone(),
            step: step as u32,
            vertex_id: vid,
        });
    }
    Ok(())
}

#[spacetimedb::reducer]
pub fn clear_traversal_results(ctx: &ReducerContext, run_tag: String) {
    let to_delete: Vec<u64> = ctx
        .db
        .traversal_result()
        .iter()
        .filter(|r| r.run_tag == run_tag)
        .map(|r| r.id)
        .collect();
    for id in to_delete {
        ctx.db.traversal_result().id().delete(id);
    }
    let to_delete: Vec<u64> = ctx
        .db
        .path_result()
        .iter()
        .filter(|r| r.run_tag == run_tag)
        .map(|r| r.id)
        .collect();
    for id in to_delete {
        ctx.db.path_result().id().delete(id);
    }
}

#[spacetimedb::reducer]
pub fn clear_graph(ctx: &ReducerContext) {
    let edge_ids: Vec<u64> = ctx.db.edge().iter().map(|e| e.id).collect();
    for id in edge_ids {
        ctx.db.edge().id().delete(id);
    }
    let vertex_ids: Vec<u64> = ctx.db.vertex().iter().map(|v| v.id).collect();
    for id in vertex_ids {
        ctx.db.vertex().id().delete(id);
    }
}

/// Bulk-insert `count` vertices using auto_inc (IDs will be 1..count).
/// Used by the cross-system benchmark harness.
#[spacetimedb::reducer]
pub fn bench_bulk_insert_vertices(ctx: &ReducerContext, count: u64) {
    for _ in 0..count {
        ctx.db.vertex().insert(Vertex {
            id: 0,
            label: String::new(),
            properties: String::new(),
        });
    }
}

/// Bulk-insert edges from a flat array: [src0, dst0, src1, dst1, ...].
/// Vertex IDs are expected to be 1-based (matching auto_inc output).
/// Used by the cross-system benchmark harness.
#[spacetimedb::reducer]
pub fn bench_bulk_insert_edges(ctx: &ReducerContext, flat_edges: Vec<u64>) {
    for pair in flat_edges.chunks(2) {
        if pair.len() == 2 {
            ctx.db.edge().insert(Edge {
                id: 0,
                start_id: pair[0],
                end_id: pair[1],
                edge_type: String::new(),
                properties: String::new(),
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Benchmark reducers
// ---------------------------------------------------------------------------

#[spacetimedb::reducer]
pub fn bench_seed_graph(ctx: &ReducerContext, vertex_count: u64, edges_per_vertex: u64) {
    let _sw = LogStopwatch::new("bench_seed_graph");
    for _ in 0..vertex_count {
        ctx.db.vertex().insert(Vertex {
            id: 0,
            label: String::new(),
            properties: String::new(),
        });
    }
    // Wire each vertex to `edges_per_vertex` successors (wrapping around).
    // Vertex auto_inc IDs start at 1.
    for src in 1..=vertex_count {
        for offset in 1..=edges_per_vertex {
            let dst = (src - 1 + offset) % vertex_count + 1;
            ctx.db.edge().insert(Edge {
                id: 0,
                start_id: src,
                end_id: dst,
                edge_type: String::new(),
                properties: String::new(),
            });
        }
    }
    log::info!(
        "bench_seed_graph: seeded {vertex_count} vertices, {} edges",
        vertex_count * edges_per_vertex
    );
}

#[spacetimedb::reducer]
pub fn bench_bfs(ctx: &ReducerContext, start_id: u64, max_depth: u32) {
    let _sw = LogStopwatch::new("bench_bfs");
let adj = build_adjacency_map(ctx);
    let visited = graph_algo::bfs(start_id, max_depth, |v| adj.get(&v).cloned().unwrap_or_default());
    log::info!(
        "bench_bfs: start={start_id} max_depth={max_depth} visited={}",
        visited.len()
    );
}

#[spacetimedb::reducer]
pub fn bench_dfs(ctx: &ReducerContext, start_id: u64, max_depth: u32) {
    let _sw = LogStopwatch::new("bench_dfs");
let adj = build_adjacency_map(ctx);
    let visited = graph_algo::dfs(start_id, max_depth, |v| adj.get(&v).cloned().unwrap_or_default());
    log::info!(
        "bench_dfs: start={start_id} max_depth={max_depth} visited={}",
        visited.len()
    );
}

#[spacetimedb::reducer]
pub fn bench_shortest_path(ctx: &ReducerContext, start_id: u64, end_id: u64) {
    let _sw = LogStopwatch::new("bench_shortest_path");
let adj = build_adjacency_map(ctx);
    let path = graph_algo::shortest_path(start_id, end_id, |v| adj.get(&v).cloned().unwrap_or_default());
    log::info!(
        "bench_shortest_path: {start_id}->{end_id} hops={}",
        if path.is_empty() { 0 } else { path.len() - 1 }
    );
}
// ---------------------------------------------------------------------------
// Optimized queries — procedures (run outside transaction lock)
// ---------------------------------------------------------------------------

/// Count outgoing neighbors using the B-tree index directly.
/// Runs as a procedure: short tx for the index scan, no lock held otherwise.
#[spacetimedb::procedure]
pub fn neighbor_count(ctx: &mut ProcedureContext, vertex_id: u64) -> u64 {
    ctx.with_tx(|tx_ctx| {
        tx_ctx.db.edge().idx_start().filter(&vertex_id).count() as u64
    })
}

/// BFS from start_id, return visited count.
/// Runs as a procedure: short tx to snapshot edges, BFS runs outside the lock.
#[spacetimedb::procedure]
pub fn bfs_count(ctx: &mut ProcedureContext, start_id: u64) -> u64 {
    let adj = ctx.with_tx(|tx_ctx| {
        let mut adj: HashMap<u64, Vec<u64>> = HashMap::new();
        for edge in tx_ctx.db.edge().iter() {
            adj.entry(edge.start_id).or_default().push(edge.end_id);
        }
        adj
    });
    let visited = graph_algo::bfs(start_id, u32::MAX, |v| {
        adj.get(&v).cloned().unwrap_or_default()
    });
    visited.len() as u64
}

/// Shortest path from start_id to end_id, return hop count.
/// Runs as a procedure: short tx to snapshot edges, BFS runs outside the lock.
#[spacetimedb::procedure]
pub fn shortest_path_hops(
    ctx: &mut ProcedureContext,
    start_id: u64,
    end_id: u64,
) -> u64 {
    let adj = ctx.with_tx(|tx_ctx| {
        let mut adj: HashMap<u64, Vec<u64>> = HashMap::new();
        for edge in tx_ctx.db.edge().iter() {
            adj.entry(edge.start_id).or_default().push(edge.end_id);
        }
        adj
    });
    let path = graph_algo::shortest_path(start_id, end_id, |v| {
        adj.get(&v).cloned().unwrap_or_default()
    });
    if path.is_empty() { 0 } else { (path.len() - 1) as u64 }
}
