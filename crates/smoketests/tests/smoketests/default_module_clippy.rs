//! These tests verify that the Rust module templates have no clippy warnings.

use std::path::PathBuf;
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Run clippy on a template's spacetimedb module directory.
/// Both templates use workspace dependencies, so they can be checked in place.
fn check_template_clippy(template_name: &str) {
    let template_module_dir = workspace_root().join(format!("templates/{}/spacetimedb", template_name));

    assert!(
        template_module_dir.exists(),
        "Template module directory does not exist: {}",
        template_module_dir.display()
    );

    let output = Command::new("cargo")
        .args(["clippy", "--", "-Dwarnings"])
        .current_dir(&template_module_dir)
        .output()
        .expect("Failed to run cargo clippy");

    assert!(
        output.status.success(),
        "Template '{}' should have no clippy warnings:\nstdout: {}\nstderr: {}",
        template_name,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Ensure that the basic-rs template module has no clippy errors or warnings
#[test]
fn test_basic_rs_template_clippy() {
    check_template_clippy("basic-rs");
}

/// Ensure that the chat-console-rs template module has no clippy errors or warnings
#[test]
fn test_chat_console_rs_template_clippy() {
    check_template_clippy("chat-console-rs");
}
