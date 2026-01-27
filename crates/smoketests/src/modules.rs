//! Registry for pre-compiled smoketest modules.
//!
//! This module provides access to WASM modules that are pre-compiled during the
//! smoketest warmup phase, eliminating per-test compilation overhead.
//!
//! Modules are built from the nested workspace at `crates/smoketests/modules/`
//! and their WASM outputs are stored in that workspace's target directory.
//!
//! Module names are automatically derived from WASM filenames:
//! - `smoketest_module_foo_bar.wasm` → module name `foo-bar`

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::workspace_root;

/// Registry mapping module names to their pre-compiled WASM paths.
static REGISTRY: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();

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
/// at least one WASM file.
pub fn precompiled_modules_available() -> bool {
    let target = modules_target_dir();
    if !target.exists() {
        return false;
    }
    // Check if there's at least one smoketest_module_*.wasm file
    std::fs::read_dir(&target)
        .map(|entries| {
            entries.filter_map(Result::ok).any(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("smoketest_module_") && n.ends_with(".wasm"))
            })
        })
        .unwrap_or(false)
}

/// Returns the target directory where pre-compiled WASM modules are stored.
fn modules_target_dir() -> PathBuf {
    workspace_root().join("crates/smoketests/modules/target/wasm32-unknown-unknown/release")
}

/// Builds the registry by scanning the target directory for WASM files.
///
/// Module names are derived from filenames:
/// - `smoketest_module_foo_bar.wasm` → `foo-bar`
fn build_registry() -> HashMap<String, PathBuf> {
    let target = modules_target_dir();
    let mut reg = HashMap::new();

    let Ok(entries) = std::fs::read_dir(&target) else {
        return reg;
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // Only process smoketest_module_*.wasm files
        if !filename.starts_with("smoketest_module_") || !filename.ends_with(".wasm") {
            continue;
        }

        // Extract module name: smoketest_module_foo_bar.wasm -> foo-bar
        let module_name = filename
            .strip_prefix("smoketest_module_")
            .unwrap()
            .strip_suffix(".wasm")
            .unwrap()
            .replace('_', "-");

        reg.insert(module_name, path);
    }

    reg
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_name_derivation() {
        // Test the naming convention
        let filename = "smoketest_module_foo_bar.wasm";
        let expected = "foo-bar";
        let actual = filename
            .strip_prefix("smoketest_module_")
            .unwrap()
            .strip_suffix(".wasm")
            .unwrap()
            .replace('_', "-");
        assert_eq!(actual, expected);
    }
}
