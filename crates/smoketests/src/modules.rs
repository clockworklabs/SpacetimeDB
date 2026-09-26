//! Registry for pre-compiled smoketest modules.
//!
//! This module provides access to WASM and JavaScript modules built during the
//! smoketest warmup phase, eliminating per-test compilation overhead.
//!
//! Rust outputs live in the nested workspace's target directory; other language
//! outputs live in `target/smoketest-precompiled`. Both respect `CARGO_TARGET_DIR`.
//!
//! Module names are derived from artifact filenames:
//! - `smoketest_module_foo_bar.wasm` → module name `foo-bar`

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::workspace_root;

/// Registry mapping names to prepared artifacts.
static REGISTRY: OnceLock<HashMap<String, PrecompiledModule>> = OnceLock::new();

/// A prepared module and the format expected by `spacetime publish`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrecompiledModule {
    Wasm(PathBuf),
    JavaScript(PathBuf),
}

impl PrecompiledModule {
    pub fn path(&self) -> &Path {
        match self {
            Self::Wasm(path) | Self::JavaScript(path) => path,
        }
    }

    pub fn publish_flag(&self) -> &'static str {
        match self {
            Self::Wasm(_) => "--bin-path",
            Self::JavaScript(_) => "--js-path",
        }
    }
}

/// Returns a named precompiled module.
///
/// # Panics
///
/// Panics if the module name is not found in the registry. This indicates
/// either a typo in the module name or that the module hasn't been added
/// to preparation yet, or its language toolchain was unavailable.
pub fn precompiled_module(name: &str) -> PrecompiledModule {
    let registry = REGISTRY.get_or_init(build_registry);
    registry.get(name).cloned().unwrap_or_else(|| {
        panic!(
            "Unknown precompiled module: '{}'. Run `cargo smoketest prepare` with its language toolchain installed. Available modules: {:?}",
            name,
            registry.keys().collect::<Vec<_>>()
        )
    })
}

/// Returns the target directory where pre-compiled WASM modules are stored.
fn modules_target_dir() -> PathBuf {
    // Respect CARGO_TARGET_DIR if set (e.g., in CI), otherwise use the modules workspace's target dir
    let base = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root().join("crates/smoketests/modules/target"));
    base.join("wasm32-unknown-unknown/release")
}

/// Directory transferred with Rust WASM files in CI support archives.
pub fn prepared_modules_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
        .join("smoketest-precompiled")
}

fn build_registry() -> HashMap<String, PrecompiledModule> {
    scan_modules(&[modules_target_dir(), prepared_modules_dir()]).expect("Failed to load precompiled modules")
}

fn scan_modules(directories: &[PathBuf]) -> anyhow::Result<HashMap<String, PrecompiledModule>> {
    let mut reg = HashMap::new();
    for directory in directories {
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err.into()),
        };
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            let Some(module_name) = artifact_to_module_name(&path) else {
                continue;
            };
            let module = match path.extension().and_then(|s| s.to_str()) {
                Some("wasm") => PrecompiledModule::Wasm(path),
                Some("js") => PrecompiledModule::JavaScript(path),
                _ => continue,
            };
            anyhow::ensure!(
                !reg.contains_key(&module_name),
                "Duplicate precompiled module: {module_name}"
            );
            reg.insert(module_name, module);
        }
    }
    Ok(reg)
}

/// Extract module name: smoketest_module_foo_bar.wasm -> foo-bar
fn artifact_to_module_name(path: &Path) -> Option<String> {
    match path.extension()?.to_str()? {
        "wasm" | "js" => Some(
            path.file_stem()?
                .to_str()?
                .strip_prefix("smoketest_module_")?
                .replace('_', "-"),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_name_derivation() {
        // Test the naming convention
        let filename = "smoketest_module_foo_bar.wasm";
        let expected = "foo-bar";
        let actual = artifact_to_module_name(Path::new(filename));
        assert_eq!(actual, Some(expected.to_string()));
    }

    #[test]
    fn discovers_both_formats_and_rejects_ambiguous_names() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "smoketest_module_foo_bar.wasm",
            "smoketest_module_script.js",
            "unrelated.wasm",
        ] {
            std::fs::write(dir.path().join(name), []).unwrap();
        }
        let directories = [dir.path().to_owned()];
        let registry = scan_modules(&directories).unwrap();
        assert_eq!(registry.len(), 2);
        assert_eq!(registry["foo-bar"].publish_flag(), "--bin-path");
        assert_eq!(registry["script"].publish_flag(), "--js-path");
        std::fs::write(dir.path().join("smoketest_module_foo-bar.js"), []).unwrap();
        assert!(scan_modules(&directories)
            .unwrap_err()
            .to_string()
            .contains("Duplicate"));
    }
}
