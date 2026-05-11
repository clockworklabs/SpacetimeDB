use graph_algo::{bfs, dfs, shortest_path};
use std::time::{Duration, Instant};

const EDGES_PER_VERTEX: u64 = 3;
const WARMUP_ITERS: u32 = 2;
const BENCH_ITERS: u32 = 5;

/// Build a ring-of-successors adjacency list identical to `bench_seed_graph`.
fn build_adjacency(vertex_count: u64, edges_per_vertex: u64) -> Vec<Vec<u64>> {
    let mut adj: Vec<Vec<u64>> = vec![Vec::new(); vertex_count as usize + 1];
    for src in 1..=vertex_count {
        for offset in 1..=edges_per_vertex {
            let dst = (src - 1 + offset) % vertex_count + 1;
            adj[src as usize].push(dst);
        }
    }
    adj
}

fn neighbors_fn(adj: &[Vec<u64>]) -> impl Fn(u64) -> Vec<u64> + '_ {
    move |v| adj.get(v as usize).cloned().unwrap_or_default()
}

struct BenchResult {
    name: String,
    vertex_count: u64,
    detail: String,
    median: Duration,
    min: Duration,
    max: Duration,
}

fn bench<F: Fn()>(f: F) -> (Duration, Duration, Duration) {
    for _ in 0..WARMUP_ITERS {
        f();
    }
    let mut times = Vec::with_capacity(BENCH_ITERS as usize);
    for _ in 0..BENCH_ITERS {
        let start = Instant::now();
        f();
        times.push(start.elapsed());
    }
    times.sort();
    let median = times[times.len() / 2];
    let min = *times.first().unwrap();
    let max = *times.last().unwrap();
    (median, min, max)
}

fn main() {
    let sizes: &[u64] = &[100, 1_000, 10_000];

    // Part 1: depth-bounded traversals (show constant-time for fixed fan-out)
    let max_depths: &[u32] = &[3, 5, 10];
    let mut results: Vec<BenchResult> = Vec::new();

    for &n in sizes {
        let adj = build_adjacency(n, EDGES_PER_VERTEX);
        let nf = neighbors_fn(&adj);

        for &depth in max_depths {
            let (median, min, max) = bench(|| {
                let _ = bfs(1, depth, &nf);
            });
            let visited = bfs(1, depth, &nf).len();
            results.push(BenchResult {
                name: "BFS".into(),
                vertex_count: n,
                detail: format!("max_depth={depth}, visited={visited}"),
                median, min, max,
            });

            let (median, min, max) = bench(|| {
                let _ = dfs(1, depth, &nf);
            });
            let visited = dfs(1, depth, &nf).len();
            results.push(BenchResult {
                name: "DFS".into(),
                vertex_count: n,
                detail: format!("max_depth={depth}, visited={visited}"),
                median, min, max,
            });
        }
    }

    println!("## Depth-bounded traversals");
    println!();
    println!("| Algorithm | Vertices | Detail | Median | Min | Max |");
    println!("|-----------|----------|--------|--------|-----|-----|");
    for r in &results {
        println!(
            "| {} | {} | {} | {:.3}ms | {:.3}ms | {:.3}ms |",
            r.name, r.vertex_count, r.detail,
            r.median.as_secs_f64() * 1000.0,
            r.min.as_secs_f64() * 1000.0,
            r.max.as_secs_f64() * 1000.0,
        );
    }

    // Part 2: full-graph traversals (visit all vertices)
    let mut full_results: Vec<BenchResult> = Vec::new();

    for &n in sizes {
        let adj = build_adjacency(n, EDGES_PER_VERTEX);
        let nf = neighbors_fn(&adj);

        let (median, min, max) = bench(|| {
            let _ = bfs(1, u32::MAX, &nf);
        });
        let visited = bfs(1, u32::MAX, &nf).len();
        full_results.push(BenchResult {
            name: "BFS".into(),
            vertex_count: n,
            detail: format!("full graph, visited={visited}"),
            median, min, max,
        });

        let (median, min, max) = bench(|| {
            let _ = dfs(1, u32::MAX, &nf);
        });
        let visited = dfs(1, u32::MAX, &nf).len();
        full_results.push(BenchResult {
            name: "DFS".into(),
            vertex_count: n,
            detail: format!("full graph, visited={visited}"),
            median, min, max,
        });
    }

    println!();
    println!("## Full-graph traversals (all vertices)");
    println!();
    println!("| Algorithm | Vertices | Detail | Median | Min | Max |");
    println!("|-----------|----------|--------|--------|-----|-----|");
    for r in &full_results {
        println!(
            "| {} | {} | {} | {:.3}ms | {:.3}ms | {:.3}ms |",
            r.name, r.vertex_count, r.detail,
            r.median.as_secs_f64() * 1000.0,
            r.min.as_secs_f64() * 1000.0,
            r.max.as_secs_f64() * 1000.0,
        );
    }

    // Part 3: shortest path at multiple scales
    let mut sp_results: Vec<BenchResult> = Vec::new();

    for &n in sizes {
        let adj = build_adjacency(n, EDGES_PER_VERTEX);
        let nf = neighbors_fn(&adj);

        let target = n / 2 + 1;
        let (median, min, max) = bench(|| {
            let _ = shortest_path(1, target, &nf);
        });
        let path = shortest_path(1, target, &nf);
        let hops = if path.is_empty() { 0 } else { path.len() - 1 };
        sp_results.push(BenchResult {
            name: "shortest_path".into(),
            vertex_count: n,
            detail: format!("1->{target}, hops={hops}"),
            median, min, max,
        });

        let near_target = (EDGES_PER_VERTEX + 1).min(n);
        let (median, min, max) = bench(|| {
            let _ = shortest_path(1, near_target, &nf);
        });
        let path = shortest_path(1, near_target, &nf);
        let hops = if path.is_empty() { 0 } else { path.len() - 1 };
        sp_results.push(BenchResult {
            name: "shortest_path".into(),
            vertex_count: n,
            detail: format!("1->{near_target} (near), hops={hops}"),
            median, min, max,
        });
    }

    println!();
    println!("## Shortest path");
    println!();
    println!("| Algorithm | Vertices | Detail | Median | Min | Max |");
    println!("|-----------|----------|--------|--------|-----|-----|");
    for r in &sp_results {
        println!(
            "| {} | {} | {} | {:.3}ms | {:.3}ms | {:.3}ms |",
            r.name, r.vertex_count, r.detail,
            r.median.as_secs_f64() * 1000.0,
            r.min.as_secs_f64() * 1000.0,
            r.max.as_secs_f64() * 1000.0,
        );
    }
}
