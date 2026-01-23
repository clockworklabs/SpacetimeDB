//! Tests translated from smoketests/tests/default_module_clippy.py

use spacetimedb_smoketests::Smoketest;
use std::process::Command;

/// Ensure that the default rust module has no clippy errors or warnings
#[test]
fn test_default_module_clippy_check() {
    // Build a smoketest with the default module code (no custom code)
    let test = Smoketest::builder().autopublish(false).build();

    let output = Command::new("cargo")
        .args(["clippy", "--", "-Dwarnings"])
        .current_dir(test.project_dir.path())
        .output()
        .expect("Failed to run cargo clippy");

    assert!(
        output.status.success(),
        "Default module should have no clippy warnings:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
