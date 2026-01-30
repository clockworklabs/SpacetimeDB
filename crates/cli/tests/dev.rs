use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

#[test]
fn cli_dev_help_shows_template_option() {
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args(["dev", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--template"))
        .stdout(predicate::str::contains("-t"));
}

#[test]
fn cli_dev_accepts_template_flag() {
    // This test verifies that the CLI correctly parses the --template flag.
    // We use --help after the flag to avoid actually running dev mode,
    // but this still validates that the flag is recognized.
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    // Running with an invalid server should fail, but not because of the template flag
    cmd.args(["dev", "--template", "react", "--server", "nonexistent-server-12345"])
        .assert()
        .failure()
        // The error should be about the server, not about an unrecognized --template flag
        .stderr(
            predicate::str::contains("template")
                .not()
                .or(predicate::str::contains("unrecognized").not()),
        );
}

#[test]
fn cli_dev_accepts_short_template_flag() {
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args(["dev", "-t", "typescript", "--server", "nonexistent-server-12345"])
        .assert()
        .failure()
        // The error should be about the server, not about an unrecognized -t flag
        .stderr(
            predicate::str::contains("-t")
                .not()
                .or(predicate::str::contains("unrecognized").not()),
        );
}

#[test]
fn cli_init_with_template_creates_project() {
    // Test that `spacetime init --template` successfully creates a project
    // We use init directly since dev forwards to it for template handling
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.current_dir(temp_dir.path())
        .args([
            "init",
            "--template",
            "basic-rs",
            "--local",
            "--non-interactive",
            "test-project",
        ])
        .assert()
        .success();

    // Verify expected files were created
    let project_dir = temp_dir.path().join("test-project");
    assert!(
        project_dir.join("spacetimedb").exists(),
        "spacetimedb directory should exist"
    );
    assert!(project_dir.join("src").exists(), "src directory should exist");
}
