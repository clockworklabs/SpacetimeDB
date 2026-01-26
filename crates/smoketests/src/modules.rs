//! Registry for pre-compiled smoketest modules.
//!
//! This module provides access to WASM modules that are pre-compiled during the
//! smoketest warmup phase, eliminating per-test compilation overhead.
//!
//! Modules are built from the nested workspace at `crates/smoketests/modules/`
//! and their WASM outputs are stored in that workspace's target directory.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::workspace_root;

/// Registry mapping module names to their pre-compiled WASM paths.
static REGISTRY: OnceLock<HashMap<&'static str, PathBuf>> = OnceLock::new();

/// Returns the path to a pre-compiled module's WASM file.
///
/// # Panics
///
/// Panics if the module name is not found in the registry. This indicates
/// either a typo in the module name or that the module hasn't been added
/// to the nested workspace yet.
pub fn precompiled_module(name: &str) -> PathBuf {
    let registry = REGISTRY.get_or_init(build_registry);
    registry.get(name).cloned().unwrap_or_else(|| {
        panic!(
            "Unknown precompiled module: '{}'. Available modules: {:?}",
            name,
            registry.keys().collect::<Vec<_>>()
        )
    })
}

/// Returns true if pre-compiled modules are available.
///
/// This checks if the modules workspace target directory exists and contains
/// at least one WASM file. Tests can use this to fall back to runtime
/// compilation if precompiled modules aren't available.
pub fn precompiled_modules_available() -> bool {
    let target = modules_target_dir();
    target.exists() && target.join("smoketest_module_filtering.wasm").exists()
}

/// Returns the target directory where pre-compiled WASM modules are stored.
fn modules_target_dir() -> PathBuf {
    workspace_root().join("crates/smoketests/modules/target/wasm32-unknown-unknown/release")
}

/// Builds the registry mapping module names to WASM paths.
fn build_registry() -> HashMap<&'static str, PathBuf> {
    let target = modules_target_dir();

    let mut reg = HashMap::new();

    // Filtering and query tests
    reg.insert("filtering", target.join("smoketest_module_filtering.wasm"));
    reg.insert("dml", target.join("smoketest_module_dml.wasm"));

    // Views tests
    reg.insert("views-basic", target.join("smoketest_module_views_basic.wasm"));
    // views-broken-namespace and views-broken-return-type are intentionally broken, not precompiled
    reg.insert("views-sql", target.join("smoketest_module_views_sql.wasm"));

    // Security and permissions
    reg.insert("rls", target.join("smoketest_module_rls.wasm"));
    reg.insert(
        "permissions-private",
        target.join("smoketest_module_permissions_private.wasm"),
    );
    reg.insert(
        "permissions-lifecycle",
        target.join("smoketest_module_permissions_lifecycle.wasm"),
    );

    // Call/procedure tests
    reg.insert(
        "call-reducer-procedure",
        target.join("smoketest_module_call_reducer_procedure.wasm"),
    );
    reg.insert("call-empty", target.join("smoketest_module_call_empty.wasm"));

    // SQL format tests
    reg.insert("sql-format", target.join("smoketest_module_sql_format.wasm"));
    reg.insert("pg-wire", target.join("smoketest_module_pg_wire.wasm"));

    // Scheduled reducer tests
    reg.insert("schedule-cancel", target.join("smoketest_module_schedule_cancel.wasm"));
    reg.insert(
        "schedule-subscribe",
        target.join("smoketest_module_schedule_subscribe.wasm"),
    );
    reg.insert(
        "schedule-volatile",
        target.join("smoketest_module_schedule_volatile.wasm"),
    );

    // Module lifecycle tests
    reg.insert("describe", target.join("smoketest_module_describe.wasm"));
    reg.insert("modules-basic", target.join("smoketest_module_modules_basic.wasm"));
    // modules-breaking is intentionally broken, not precompiled
    reg.insert(
        "modules-add-table",
        target.join("smoketest_module_modules_add_table.wasm"),
    );

    // Index tests
    reg.insert(
        "add-remove-index",
        target.join("smoketest_module_add_remove_index.wasm"),
    );
    reg.insert(
        "add-remove-index-indexed",
        target.join("smoketest_module_add_remove_index_indexed.wasm"),
    );

    // Panic/error handling
    reg.insert("panic", target.join("smoketest_module_panic.wasm"));
    reg.insert("panic-error", target.join("smoketest_module_panic_error.wasm"));

    // Restart tests
    reg.insert("restart-person", target.join("smoketest_module_restart_person.wasm"));
    reg.insert(
        "restart-connected-client",
        target.join("smoketest_module_restart_connected_client.wasm"),
    );

    // Connection tests
    reg.insert(
        "connect-disconnect",
        target.join("smoketest_module_connect_disconnect.wasm"),
    );
    reg.insert("confirmed-reads", target.join("smoketest_module_confirmed_reads.wasm"));
    reg.insert("delete-database", target.join("smoketest_module_delete_database.wasm"));
    reg.insert(
        "client-connection-reject",
        target.join("smoketest_module_client_connection_reject.wasm"),
    );
    reg.insert(
        "client-connection-disconnect-panic",
        target.join("smoketest_module_client_connection_disconnect_panic.wasm"),
    );

    // Misc tests
    reg.insert("namespaces", target.join("smoketest_module_namespaces.wasm"));
    reg.insert("new-user-flow", target.join("smoketest_module_new_user_flow.wasm"));
    reg.insert(
        "module-nested-op",
        target.join("smoketest_module_module_nested_op.wasm"),
    );
    // fail-initial-publish-broken is intentionally broken, not precompiled
    reg.insert(
        "fail-initial-publish-fixed",
        target.join("smoketest_module_fail_initial_publish_fixed.wasm"),
    );

    // Auto-increment tests (parameterized variants)
    reg.insert(
        "autoinc-basic-u32",
        target.join("smoketest_module_autoinc_basic_u32.wasm"),
    );
    reg.insert(
        "autoinc-basic-u64",
        target.join("smoketest_module_autoinc_basic_u64.wasm"),
    );
    reg.insert(
        "autoinc-basic-i32",
        target.join("smoketest_module_autoinc_basic_i32.wasm"),
    );
    reg.insert(
        "autoinc-basic-i64",
        target.join("smoketest_module_autoinc_basic_i64.wasm"),
    );
    reg.insert(
        "autoinc-unique-u64",
        target.join("smoketest_module_autoinc_unique_u64.wasm"),
    );
    reg.insert(
        "autoinc-unique-i64",
        target.join("smoketest_module_autoinc_unique_i64.wasm"),
    );

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_entries() {
        let registry = REGISTRY.get_or_init(build_registry);
        assert!(!registry.is_empty(), "Registry should have entries");
    }

    #[test]
    fn test_module_paths_end_with_wasm() {
        let registry = REGISTRY.get_or_init(build_registry);
        for (name, path) in registry.iter() {
            assert!(
                path.extension().map_or(false, |ext| ext == "wasm"),
                "Module {} path should end with .wasm: {:?}",
                name,
                path
            );
        }
    }
}
