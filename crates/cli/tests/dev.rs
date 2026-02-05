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
            "basic-rust",
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

#[test]
fn config_with_snake_case_field_shows_error() {
    // Test that using snake_case field names (dev_run) instead of kebab-case (dev-run)
    // shows a helpful error message
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

    // Create a config with snake_case field name
    let config_content = r#"{
  "dev_run": "npm run dev",
  "publish": {
    "database": "test-db"
  }
}"#;
    std::fs::write(temp_dir.path().join("spacetime.json"), config_content).expect("failed to write config");

    // Create minimal spacetimedb module
    std::fs::create_dir(temp_dir.path().join("spacetimedb")).expect("failed to create spacetimedb dir");
    std::fs::create_dir(temp_dir.path().join("spacetimedb/src")).expect("failed to create src dir");
    std::fs::write(
        temp_dir.path().join("spacetimedb/Cargo.toml"),
        r#"[package]
name = "test"
version = "0.1.0"

[dependencies]
spacetimedb = "1.0"

[lib]
crate-type = ["cdylib"]
"#,
    )
    .expect("failed to write Cargo.toml");
    std::fs::write(temp_dir.path().join("spacetimedb/src/lib.rs"), "").expect("failed to write lib.rs");

    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.current_dir(temp_dir.path())
        .args(["dev", "test-db"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to load spacetime.json"))
        .stderr(predicate::str::contains("unknown field `dev_run`"));
}
