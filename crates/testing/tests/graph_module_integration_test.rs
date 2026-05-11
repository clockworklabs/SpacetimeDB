use serial_test::serial;
use spacetimedb_lib::sats::product;
use spacetimedb_testing::modules::{CompilationMode, CompiledModule, DEFAULT_CONFIG};

fn init() {
    let _ = env_logger::builder()
        .parse_filters(
            "spacetimedb=trace,spacetimedb_client_api=trace,spacetimedb_lib=trace,spacetimedb_standalone=trace",
        )
        .is_test(true)
        .try_init();
}

// ========================================================================
// Vertex CRUD happy path
// ========================================================================

#[test]
#[serial]
fn graph_create_vertex() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{\"name\":\"Alice\"}"])
                .await
                .unwrap();
        },
    );
}

#[test]
#[serial]
fn graph_update_vertex() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{\"name\":\"Alice\"}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("update_vertex", &product![1u64, "Person", "{\"name\":\"Bob\"}"])
                .await
                .unwrap();
        },
    );
}

#[test]
#[serial]
fn graph_delete_vertex() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{\"name\":\"Alice\"}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("delete_vertex", &product![1u64])
                .await
                .unwrap();
        },
    );
}

// ========================================================================
// Edge CRUD happy path
// ========================================================================

#[test]
#[serial]
fn graph_create_edge() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 2u64, "knows", "{}"])
                .await
                .unwrap();
        },
    );
}

#[test]
#[serial]
fn graph_update_edge() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 2u64, "knows", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("update_edge", &product![1u64, 2u64, 1u64, "friend", "{}"])
                .await
                .unwrap();
        },
    );
}

#[test]
#[serial]
fn graph_delete_edge() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 2u64, "knows", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("delete_edge", &product![1u64])
                .await
                .unwrap();
        },
    );
}

// ========================================================================
// Edge endpoint validation
// ========================================================================

#[test]
#[serial]
fn graph_create_edge_rejects_missing_start_vertex() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            let result = module
                .call_reducer_binary("create_edge", &product![999u64, 1u64, "knows", "{}"])
                .await;
            assert!(result.is_err(), "Expected error for missing start vertex");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("Start vertex 999 not found"),
                "Error should mention missing start vertex, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_create_edge_rejects_missing_end_vertex() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            let result = module
                .call_reducer_binary("create_edge", &product![1u64, 999u64, "knows", "{}"])
                .await;
            assert!(result.is_err(), "Expected error for missing end vertex");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("End vertex 999 not found"),
                "Error should mention missing end vertex, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_update_edge_rejects_missing_start_vertex() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 2u64, "knows", "{}"])
                .await
                .unwrap();
            let result = module
                .call_reducer_binary("update_edge", &product![1u64, 999u64, 2u64, "knows", "{}"])
                .await;
            assert!(result.is_err(), "Expected error for missing start vertex on update");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("Start vertex 999 not found"),
                "Error should mention missing start vertex, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_update_edge_rejects_missing_end_vertex() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 2u64, "knows", "{}"])
                .await
                .unwrap();
            let result = module
                .call_reducer_binary("update_edge", &product![1u64, 1u64, 999u64, "knows", "{}"])
                .await;
            assert!(result.is_err(), "Expected error for missing end vertex on update");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("End vertex 999 not found"),
                "Error should mention missing end vertex, got: {}",
                err
            );
        },
    );
}

// ========================================================================
// Cascade deletion
// ========================================================================

#[test]
#[serial]
fn graph_delete_vertex_cascades_to_connected_edges() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 2u64, "knows", "{}"])
                .await
                .unwrap();

            // Delete vertex 1 — should cascade and remove edge 1
            module
                .call_reducer_binary("delete_vertex", &product![1u64])
                .await
                .unwrap();

            // Edge should no longer exist
            let result = module.call_reducer_binary("delete_edge", &product![1u64]).await;
            assert!(result.is_err(), "Expected edge to be cascade-deleted");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("Edge 1 not found"),
                "Error should mention missing edge, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_delete_vertex_cascades_multiple_edges() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            // Create a star: vertex 1 connected to 2, 3, 4
            for _ in 0..4 {
                module
                    .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                    .await
                    .unwrap();
            }
            module
                .call_reducer_binary("create_edge", &product![1u64, 2u64, "knows", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 3u64, "knows", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 4u64, "knows", "{}"])
                .await
                .unwrap();

            // Delete the center vertex — all three edges should be removed
            module
                .call_reducer_binary("delete_vertex", &product![1u64])
                .await
                .unwrap();

            for edge_id in 1..=3u64 {
                let result = module.call_reducer_binary("delete_edge", &product![edge_id]).await;
                assert!(result.is_err(), "Expected edge {} to be cascade-deleted", edge_id);
                let err = format!("{}", result.unwrap_err());
                assert!(
                    err.contains(&format!("Edge {edge_id} not found")),
                    "Error should mention missing edge {edge_id}, got: {}",
                    err
                );
            }
        },
    );
}

// ========================================================================
// Missing entity errors
// ========================================================================

#[test]
#[serial]
fn graph_update_vertex_rejects_missing() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let result = module
                .call_reducer_binary("update_vertex", &product![999u64, "Person", "{}"])
                .await;
            assert!(result.is_err(), "Expected error for missing vertex");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("Vertex 999 not found"),
                "Error should mention missing vertex, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_delete_vertex_rejects_missing() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let result = module.call_reducer_binary("delete_vertex", &product![999u64]).await;
            assert!(result.is_err(), "Expected error for missing vertex");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("Vertex 999 not found"),
                "Error should mention missing vertex, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_delete_edge_rejects_missing() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let result = module.call_reducer_binary("delete_edge", &product![999u64]).await;
            assert!(result.is_err(), "Expected error for missing edge");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("Edge 999 not found"),
                "Error should mention missing edge, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_update_edge_rejects_missing() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            let result = module
                .call_reducer_binary("update_edge", &product![999u64, 1u64, 2u64, "knows", "{}"])
                .await;
            assert!(result.is_err(), "Expected error for missing edge");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("Edge 999 not found"),
                "Error should mention missing edge, got: {}",
                err
            );
        },
    );
}

// ========================================================================
// Traversal reducers
// ========================================================================

#[test]
#[serial]
fn graph_bfs_happy_path() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            // 1 -> 2 -> 3
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 2u64, "knows", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![2u64, 3u64, "knows", "{}"])
                .await
                .unwrap();

            module
                .call_reducer_binary("bfs", &product![1u64, 10u32, "test-bfs"])
                .await
                .unwrap();
        },
    );
}

#[test]
#[serial]
fn graph_bfs_rejects_missing_start() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let result = module
                .call_reducer_binary("bfs", &product![999u64, 10u32, "test-bfs"])
                .await;
            assert!(result.is_err(), "Expected error for missing start vertex in BFS");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("Start vertex 999 not found"),
                "Error should mention missing start vertex, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_dfs_happy_path() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 2u64, "knows", "{}"])
                .await
                .unwrap();

            module
                .call_reducer_binary("dfs", &product![1u64, 10u32, "test-dfs"])
                .await
                .unwrap();
        },
    );
}

#[test]
#[serial]
fn graph_dfs_rejects_missing_start() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            let result = module
                .call_reducer_binary("dfs", &product![999u64, 10u32, "test-dfs"])
                .await;
            assert!(result.is_err(), "Expected error for missing start vertex in DFS");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("Start vertex 999 not found"),
                "Error should mention missing start vertex, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_shortest_path_happy_path() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("create_edge", &product![1u64, 2u64, "knows", "{}"])
                .await
                .unwrap();

            module
                .call_reducer_binary("shortest_path", &product![1u64, 2u64, "test-sp"])
                .await
                .unwrap();
        },
    );
}

#[test]
#[serial]
fn graph_shortest_path_rejects_missing_start() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            let result = module
                .call_reducer_binary("shortest_path", &product![999u64, 1u64, "test-sp"])
                .await;
            assert!(
                result.is_err(),
                "Expected error for missing start vertex in shortest_path"
            );
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("Start vertex 999 not found"),
                "Error should mention missing start vertex, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_shortest_path_rejects_missing_end() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            let result = module
                .call_reducer_binary("shortest_path", &product![1u64, 999u64, "test-sp"])
                .await;
            assert!(result.is_err(), "Expected error for missing end vertex in shortest_path");
            let err = format!("{}", result.unwrap_err());
            assert!(
                err.contains("End vertex 999 not found"),
                "Error should mention missing end vertex, got: {}",
                err
            );
        },
    );
}

#[test]
#[serial]
fn graph_clear_traversal_results() {
    init();
    CompiledModule::compile("graph", CompilationMode::Debug).with_module_async(
        DEFAULT_CONFIG,
        |module| async move {
            module
                .call_reducer_binary("create_vertex", &product!["Person", "{}"])
                .await
                .unwrap();
            module
                .call_reducer_binary("bfs", &product![1u64, 1u32, "clear-test"])
                .await
                .unwrap();
            module
                .call_reducer_binary("clear_traversal_results", &product!["clear-test"])
                .await
                .unwrap();
        },
    );
}
