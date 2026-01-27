//! CLI dev command tests

use predicates::prelude::*;
use spacetimedb_guard::ensure_binaries_built;
use std::process::Command;

fn cli_cmd() -> Command {
    Command::new(ensure_binaries_built())
}

#[test]
fn cli_dev_help_shows_template_option() {
    let output = cli_cmd().args(["dev", "--help"]).output().expect("failed to execute");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        predicate::str::contains("--template").eval(&stdout),
        "stdout should contain --template"
    );
    assert!(predicate::str::contains("-t").eval(&stdout), "stdout should contain -t");
}

#[test]
fn cli_dev_accepts_template_flag() {
    // Running with an invalid server should fail, but not because of the template flag
    let output = cli_cmd()
        .args(["dev", "--template", "react", "--server", "nonexistent-server-12345"])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The error should be about the server, not about an unrecognized --template flag
    assert!(
        !stderr.contains("unrecognized") || !stderr.contains("template"),
        "stderr should not complain about unrecognized template flag"
    );
}

#[test]
fn cli_dev_accepts_short_template_flag() {
    let output = cli_cmd()
        .args(["dev", "-t", "typescript", "--server", "nonexistent-server-12345"])
        .output()
        .expect("failed to execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The error should be about the server, not about an unrecognized -t flag
    assert!(
        !stderr.contains("unrecognized") || !stderr.contains("-t"),
        "stderr should not complain about unrecognized -t flag"
    );
}

#[test]
fn cli_init_with_template_creates_project() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

    let output = cli_cmd()
        .current_dir(temp_dir.path())
        .args([
            "init",
            "--template",
            "basic-rs",
            "--local",
            "--non-interactive",
            "test-project",
        ])
        .output()
        .expect("failed to execute");

    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify expected files were created
    let project_dir = temp_dir.path().join("test-project");
    assert!(
        project_dir.join("spacetimedb").exists(),
        "spacetimedb directory should exist"
    );
    assert!(project_dir.join("src").exists(), "src directory should exist");
}
