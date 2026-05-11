use std::collections::{HashMap, HashSet, VecDeque};

/// A visited vertex with its distance from the start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisitedVertex {
    pub vertex_id: u64,
    pub depth: u32,
}

/// BFS from `start` up to `max_depth` hops.
///
/// `neighbors` returns outgoing neighbor IDs for a given vertex.
/// Returns visited vertices in BFS order with their depth.
pub fn bfs(start: u64, max_depth: u32, neighbors: impl Fn(u64) -> Vec<u64>) -> Vec<VisitedVertex> {
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut result = Vec::new();

    visited.insert(start);
    queue.push_back((start, 0u32));
    result.push(VisitedVertex {
        vertex_id: start,
        depth: 0,
    });

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        for neighbor in neighbors(current) {
            if visited.insert(neighbor) {
                let next_depth = depth + 1;
                queue.push_back((neighbor, next_depth));
                result.push(VisitedVertex {
                    vertex_id: neighbor,
                    depth: next_depth,
                });
            }
        }
    }

    result
}

/// DFS from `start` up to `max_depth` hops.
///
/// `neighbors` returns outgoing neighbor IDs for a given vertex.
/// Returns visited vertices in DFS order with their depth.
pub fn dfs(start: u64, max_depth: u32, neighbors: impl Fn(u64) -> Vec<u64>) -> Vec<VisitedVertex> {
    let mut visited = HashSet::new();
    let mut stack = Vec::new();
    let mut result = Vec::new();

    visited.insert(start);
    stack.push((start, 0u32));
    result.push(VisitedVertex {
        vertex_id: start,
        depth: 0,
    });

    while let Some((current, depth)) = stack.pop() {
        if depth >= max_depth {
            continue;
        }
        for neighbor in neighbors(current) {
            if visited.insert(neighbor) {
                let next_depth = depth + 1;
                stack.push((neighbor, next_depth));
                result.push(VisitedVertex {
                    vertex_id: neighbor,
                    depth: next_depth,
                });
            }
        }
    }

    result
}

/// BFS-based shortest path from `start` to `end`.
///
/// Returns the path as an ordered list of vertex IDs (start to end),
/// or an empty vec if unreachable.
pub fn shortest_path(start: u64, end: u64, neighbors: impl Fn(u64) -> Vec<u64>) -> Vec<u64> {
    if start == end {
        return vec![start];
    }

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut parent: HashMap<u64, u64> = HashMap::new();

    visited.insert(start);
    queue.push_back(start);

    while let Some(current) = queue.pop_front() {
        for neighbor in neighbors(current) {
            if visited.insert(neighbor) {
                parent.insert(neighbor, current);
                if neighbor == end {
                    return reconstruct_path(&parent, start, end);
                }
                queue.push_back(neighbor);
            }
        }
    }

    Vec::new()
}

fn reconstruct_path(parent: &HashMap<u64, u64>, start: u64, end: u64) -> Vec<u64> {
    let mut path = Vec::new();
    let mut current = end;
    while current != start {
        path.push(current);
        current = parent[&current];
    }
    path.push(start);
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a neighbor-lookup closure from an adjacency list.
    fn adj(edges: &[(u64, u64)]) -> impl Fn(u64) -> Vec<u64> + '_ {
        move |v| {
            edges
                .iter()
                .filter(|(s, _)| *s == v)
                .map(|(_, e)| *e)
                .collect()
        }
    }

    // -----------------------------------------------------------------------
    // Graph fixture:
    //   0 -> 1 -> 3
    //   0 -> 2 -> 3 -> 4
    //         \-> 5
    // -----------------------------------------------------------------------
    fn diamond_edges() -> Vec<(u64, u64)> {
        vec![
            (0, 1),
            (0, 2),
            (1, 3),
            (2, 3),
            (2, 5),
            (3, 4),
        ]
    }

    // =======================================================================
    // BFS
    // =======================================================================

    #[test]
    fn bfs_visits_all_reachable() {
        let edges = diamond_edges();
        let result = bfs(0, u32::MAX, adj(&edges));
        let ids: HashSet<u64> = result.iter().map(|v| v.vertex_id).collect();
        assert_eq!(ids, [0, 1, 2, 3, 4, 5].iter().copied().collect());
    }

    #[test]
    fn bfs_respects_max_depth() {
        let edges = diamond_edges();
        let result = bfs(0, 1, adj(&edges));
        let ids: HashSet<u64> = result.iter().map(|v| v.vertex_id).collect();
        assert_eq!(ids, [0, 1, 2].iter().copied().collect());
    }

    #[test]
    fn bfs_depth_zero_returns_start_only() {
        let edges = diamond_edges();
        let result = bfs(0, 0, adj(&edges));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].vertex_id, 0);
        assert_eq!(result[0].depth, 0);
    }

    #[test]
    fn bfs_records_correct_depths() {
        let edges = diamond_edges();
        let result = bfs(0, u32::MAX, adj(&edges));
        let depths: HashMap<u64, u32> =
            result.iter().map(|v| (v.vertex_id, v.depth)).collect();
        assert_eq!(depths[&0], 0);
        assert_eq!(depths[&1], 1);
        assert_eq!(depths[&2], 1);
        assert_eq!(depths[&3], 2);
        assert_eq!(depths[&4], 3);
        assert_eq!(depths[&5], 2);
    }

    #[test]
    fn bfs_isolated_vertex() {
        let edges: Vec<(u64, u64)> = vec![];
        let result = bfs(42, u32::MAX, adj(&edges));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].vertex_id, 42);
    }

    // =======================================================================
    // DFS
    // =======================================================================

    #[test]
    fn dfs_visits_all_reachable() {
        let edges = diamond_edges();
        let result = dfs(0, u32::MAX, adj(&edges));
        let ids: HashSet<u64> = result.iter().map(|v| v.vertex_id).collect();
        assert_eq!(ids, [0, 1, 2, 3, 4, 5].iter().copied().collect());
    }

    #[test]
    fn dfs_respects_max_depth() {
        let edges = diamond_edges();
        let result = dfs(0, 1, adj(&edges));
        let ids: HashSet<u64> = result.iter().map(|v| v.vertex_id).collect();
        assert_eq!(ids, [0, 1, 2].iter().copied().collect());
    }

    #[test]
    fn dfs_depth_zero_returns_start_only() {
        let edges = diamond_edges();
        let result = dfs(0, 0, adj(&edges));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].vertex_id, 0);
    }

    #[test]
    fn dfs_isolated_vertex() {
        let edges: Vec<(u64, u64)> = vec![];
        let result = dfs(99, u32::MAX, adj(&edges));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].vertex_id, 99);
    }

    // =======================================================================
    // Shortest path
    // =======================================================================

    #[test]
    fn shortest_path_direct() {
        let edges = diamond_edges();
        let path = shortest_path(0, 1, adj(&edges));
        assert_eq!(path, vec![0, 1]);
    }

    #[test]
    fn shortest_path_multi_hop() {
        let edges = diamond_edges();
        let path = shortest_path(0, 4, adj(&edges));
        // 0 -> 1 -> 3 -> 4 or 0 -> 2 -> 3 -> 4 (both length 3)
        assert_eq!(path.len(), 4);
        assert_eq!(*path.first().unwrap(), 0);
        assert_eq!(*path.last().unwrap(), 4);
    }

    #[test]
    fn shortest_path_same_vertex() {
        let edges = diamond_edges();
        let path = shortest_path(0, 0, adj(&edges));
        assert_eq!(path, vec![0]);
    }

    #[test]
    fn shortest_path_unreachable() {
        let edges = diamond_edges();
        let path = shortest_path(4, 0, adj(&edges)); // no back-edges
        assert!(path.is_empty());
    }

    #[test]
    fn shortest_path_prefers_shorter() {
        // 0 -> 1 -> 2 (length 2)
        // 0 -> 2      (length 1)  <-- should win
        let edges = vec![(0, 1), (1, 2), (0, 2)];
        let path = shortest_path(0, 2, adj(&edges));
        assert_eq!(path, vec![0, 2]);
    }

    // =======================================================================
    // Cycle handling
    // =======================================================================

    #[test]
    fn bfs_handles_cycle() {
        let edges = vec![(0, 1), (1, 2), (2, 0)];
        let result = bfs(0, 10, adj(&edges));
        assert_eq!(result.len(), 3); // visits each once
    }

    #[test]
    fn dfs_handles_cycle() {
        let edges = vec![(0, 1), (1, 2), (2, 0)];
        let result = dfs(0, 10, adj(&edges));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn shortest_path_with_cycle() {
        let edges = vec![(0, 1), (1, 2), (2, 0), (2, 3)];
        let path = shortest_path(0, 3, adj(&edges));
        assert_eq!(path, vec![0, 1, 2, 3]);
    }
}
