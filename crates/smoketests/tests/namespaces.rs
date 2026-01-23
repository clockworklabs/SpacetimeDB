//! Namespace tests translated from smoketests/tests/namespaces.py

use spacetimedb_smoketests::Smoketest;
use std::fs;
use std::path::Path;

/// Template module code matching the Python test's default
const TEMPLATE_MODULE_CODE: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::reducer(init)]
pub fn init(_ctx: &ReducerContext) {
    // Called when the module is initially published
}

#[spacetimedb::reducer(client_connected)]
pub fn identity_connected(_ctx: &ReducerContext) {
    // Called everytime a new client connects
}

#[spacetimedb::reducer(client_disconnected)]
pub fn identity_disconnected(_ctx: &ReducerContext) {
    // Called everytime a client disconnects
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
"#;

/// Count occurrences of a needle string in all .cs files under a directory
fn count_matches(dir: &Path, needle: &str) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_matches(&path, needle);
            } else if path.extension().map_or(false, |ext| ext == "cs") {
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
    let test = Smoketest::builder()
        .module_code(TEMPLATE_MODULE_CODE)
        .autopublish(false)
        .build();

    let tmpdir = tempfile::tempdir().expect("Failed to create temp dir");
    let project_path = test.project_dir.path().to_str().unwrap();

    // Use spacetime_local since generate doesn't need a server connection
    test.spacetime_local(&[
        "generate",
        "--out-dir",
        tmpdir.path().to_str().unwrap(),
        "--lang=csharp",
        "--project-path",
        project_path,
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
    let test = Smoketest::builder()
        .module_code(TEMPLATE_MODULE_CODE)
        .autopublish(false)
        .build();

    let tmpdir = tempfile::tempdir().expect("Failed to create temp dir");
    let project_path = test.project_dir.path().to_str().unwrap();

    // Use a unique namespace name
    let namespace = "CustomTestNamespace";

    // Use spacetime_local since generate doesn't need a server connection
    test.spacetime_local(&[
        "generate",
        "--out-dir",
        tmpdir.path().to_str().unwrap(),
        "--lang=csharp",
        "--namespace",
        namespace,
        "--project-path",
        project_path,
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
