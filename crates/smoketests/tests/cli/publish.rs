//! CLI publish command tests

use spacetimedb_smoketests::{require_local_server, Smoketest};

#[test]
fn cli_can_publish_spacetimedb_on_disk() {
    let test = Smoketest::builder().autopublish(false).build();

    // Workspace root for `cargo run -p ...`
    let workspace_dir = cargo_metadata::MetadataCommand::new().exec().unwrap().workspace_root;
    // dir = <workspace_root>/templates/chat-console-rs/spacetimedb
    let dir = workspace_dir
        .join("templates")
        .join("chat-console-rs")
        .join("spacetimedb");

    let dir = dir.to_string();
    let _ = test
        .spacetime(&[
            "publish",
            "--project-path",
            &dir,
            "--server",
            &test.server_url,
            "foobar",
        ])
        .unwrap();

    // Can republish without error to the same name
    let _ = test
        .spacetime(&[
            "publish",
            "--project-path",
            &dir,
            "--server",
            &test.server_url,
            "foobar",
        ])
        .unwrap();
}

// TODO: Somewhere we should test that data is actually deleted properly in all the expected cases,
// e.g. when providing --delete-data, or when there's a conflict and --delete-data=on-conflict is provided.

fn migration_test(module_name: &str, republish_args: &[&str], expect_success: bool) {
    // This only requires a local server because the module names are static
    require_local_server!();

    let test = Smoketest::builder().autopublish(false).build();

    let workspace_dir = cargo_metadata::MetadataCommand::new().exec().unwrap().workspace_root;
    let dir = workspace_dir.join("modules").join("module-test");

    let dir = dir.to_string();
    let _ = test
        .spacetime(&[
            "publish",
            "--project-path",
            &dir,
            "--server",
            &test.server_url,
            module_name,
        ])
        .unwrap();

    let dir = dir.to_string();
    let mut args = vec![
        "publish",
        "--project-path",
        &dir,
        "--server",
        &test.server_url,
        module_name,
    ];
    args.extend(republish_args);
    let output = test.spacetime_cmd(&args);

    if expect_success {
        assert!(
            output.status.success(),
            "republish should have succeeded: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else {
        assert!(!output.status.success(), "republish should have failed but succeeded");
    }
}

#[test]
fn cli_can_publish_no_conflict_does_not_delete_data() {
    migration_test(
        "no-conflict-test",
        &[
            // NOTE: deleting data requires --yes,
            // so not providing it here ensures that no data deletion is attempted.
            "--delete-data=on-conflict",
        ],
        true,
    );
}

#[test]
fn cli_can_publish_no_conflict_with_delete_data_flag() {
    migration_test("no-conflict-delete-data-test", &["--delete-data", "--yes"], true);
}

#[test]
fn cli_can_publish_no_conflict_without_delete_data_flag() {
    migration_test("no-conflict-test", &[], true);
}

#[test]
fn cli_can_publish_with_automigration_change() {
    migration_test(
        "automigration-test",
        &["--build-options=--features test-add-column", "--break-clients"],
        true,
    );
}

#[test]
fn cli_cannot_publish_automigration_change_without_yes_break_clients() {
    migration_test(
        "automigration-test-no-break-flag",
        &["--build-options=--features test-add-column"],
        false,
    );
}

#[test]
fn cli_can_publish_automigration_change_with_on_conflict_and_yes_break_clients() {
    migration_test(
        "automigration-on-conflict-test",
        &[
            "--build-options=--features test-add-column",
            // NOTE: deleting data requires --yes,
            // so not providing it here ensures that no data deletion is attempted.
            "--delete-data=on-conflict",
            "--break-clients",
        ],
        true,
    );
}

#[test]
fn cli_cannot_publish_automigration_change_with_on_conflict_without_yes_break_clients() {
    migration_test(
        "automigration-on-conflict-no-break-flag-test",
        &[
            "--build-options=--features test-add-column",
            // NOTE: deleting data requires --yes,
            // so not providing it here ensures that no data deletion is attempted.
            "--delete-data=on-conflict",
        ],
        false,
    );
}

#[test]
fn cli_can_publish_automigration_change_with_delete_data_always_without_yes_break_clients() {
    migration_test(
        "automigration-delete-data-test",
        &["--build-options=--features test-add-column", "--delete-data", "--yes"],
        true,
    );
}

#[test]
fn cli_can_publish_automigration_change_with_delete_data_always_and_yes_break_clients() {
    migration_test(
        "automigration-delete-data-break-test",
        &[
            "--build-options=--features test-add-column",
            "--delete-data",
            "--yes",
            "--break-clients",
        ],
        true,
    );
}

#[test]
fn cli_cannot_publish_breaking_change_without_flag() {
    migration_test(
        "breaking-change-test",
        &["--build-options=--features test-remove-table"],
        false,
    );
}

#[test]
fn cli_can_publish_breaking_change_with_delete_data_flag() {
    migration_test(
        "breaking-change-delete-data-test",
        &["--build-options=--features test-remove-table", "--delete-data", "--yes"],
        true,
    );
}

#[test]
fn cli_can_publish_breaking_change_with_on_conflict_flag() {
    migration_test(
        "breaking-change-on-conflict-test",
        &[
            "--build-options=--features test-remove-table",
            "--delete-data=on-conflict",
            "--yes",
        ],
        true,
    );
}

#[test]
fn cli_publish_with_config_but_no_match_uses_cli_args() {
    // Test that when config exists but doesn't match CLI args, we use CLI args
    let test = Smoketest::builder().autopublish(false).build();
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");

    // Initialize a new project (creates test-project/spacetimedb/)
    test.spacetime(&[
        "init",
        "--non-interactive",
        "--lang",
        "rust",
        temp_dir.path().join("test-project").to_str().unwrap(),
    ])
    .unwrap();

    let module_dir = temp_dir.path().join("test-project").join("spacetimedb");

    // Build the module first
    test.spacetime(&["build", "--project-path", module_dir.to_str().unwrap()])
        .unwrap();

    // Create a config with a different database name
    let config_content = r#"{
  "publish": {
    "database": "config-db-name"
  }
}"#;
    std::fs::write(module_dir.join("spacetime.json"), config_content).expect("failed to write config");

    // Publish with a different database name from CLI - should use CLI args, not config
    test.spacetime(&[
        "publish",
        "--server",
        &test.server_url,
        "cli-db-name",
        "--project-path",
        module_dir.to_str().unwrap(),
    ])
    .unwrap();
}
