//! Namespace tests translated from smoketests/tests/namespaces.py

use spacetimedb_smoketests::Smoketest;
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Count occurrences of a needle string in all .cs files under a directory
fn count_matches(dir: &Path, needle: &str) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_matches(&path, needle);
            } else if path.extension().is_some_and(|ext| ext == "cs") {
                if let Ok(contents) = fs::read_to_string(&path) {
                    count += contents.matches(needle).count();
                }
            }
        }
    }
    count
}

/// Ensure that the default namespace is working properly
#[test]
fn test_spacetimedb_ns_csharp() {
    let _test = Smoketest::builder()
        .precompiled_module("namespaces")
        .autopublish(false)
        .build();

    let tmpdir = tempfile::tempdir().expect("Failed to create temp dir");
    let project_path = workspace_root().join("crates/smoketests/modules/namespaces");

    _test
        .spacetime(&[
            "generate",
            "--out-dir",
            tmpdir.path().to_str().unwrap(),
            "--lang=csharp",
            "--project-path",
            project_path.to_str().unwrap(),
        ])
        .unwrap();

    let namespace = "SpacetimeDB.Types";
    assert_eq!(
        count_matches(tmpdir.path(), &format!("namespace {}", namespace)),
        7,
        "Expected 7 occurrences of 'namespace {}'",
        namespace
    );
    assert_eq!(
        count_matches(tmpdir.path(), "using SpacetimeDB;"),
        0,
        "Expected 0 occurrences of 'using SpacetimeDB;'"
    );
}

/// Ensure that when a custom namespace is specified on the command line, it actually gets used in generation
#[test]
fn test_custom_ns_csharp() {
    let _test = Smoketest::builder()
        .precompiled_module("namespaces")
        .autopublish(false)
        .build();

    let tmpdir = tempfile::tempdir().expect("Failed to create temp dir");
    let project_path = workspace_root().join("crates/smoketests/modules/namespaces");

    // Use a unique namespace name
    let namespace = "CustomTestNamespace";

    _test
        .spacetime(&[
            "generate",
            "--out-dir",
            tmpdir.path().to_str().unwrap(),
            "--lang=csharp",
            "--namespace",
            namespace,
            "--project-path",
            project_path.to_str().unwrap(),
        ])
        .unwrap();

    assert_eq!(
        count_matches(tmpdir.path(), &format!("namespace {}", namespace)),
        7,
        "Expected 7 occurrences of 'namespace {}'",
        namespace
    );
    assert_eq!(
        count_matches(tmpdir.path(), "using SpacetimeDB;"),
        7,
        "Expected 7 occurrences of 'using SpacetimeDB;'"
    );
}
