use assert_cmd::cargo::cargo_bin_cmd;
use spacetimedb_guard::SpacetimeDbGuard;

#[test]
fn cli_can_publish_spacetimedb_on_disk() {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();

    // Workspace root for `cargo run -p ...`
    let workspace_dir = cargo_metadata::MetadataCommand::new().exec().unwrap().workspace_root;
    // dir = <workspace_root>/modules/quickstart-chat
    let dir = workspace_dir
        .join("templates")
        .join("quickstart-chat-rust")
        .join("spacetimedb");

    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args(["publish", "--server", &spacetime.host_url.to_string(), "foobar"])
        .current_dir(dir.clone())
        .assert()
        .success();

    // Can republish without error to the same name
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args(["publish", "--server", &spacetime.host_url.to_string(), "foobar"])
        .current_dir(dir)
        .assert()
        .success();
}

// TODO: Somewhere we should test that data is actually deleted properly in all the expected cases,
// e.g. when providing --delete-data, or when there's a conflict and --delete-data=on-conflict is provided.

fn migration_test(module_name: &str, republish_args: &[&str], expect_success: bool) {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();

    let workspace_dir = cargo_metadata::MetadataCommand::new().exec().unwrap().workspace_root;
    let dir = workspace_dir.join("modules").join("module-test");

    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args(["publish", module_name, "--server", &spacetime.host_url.to_string()])
        .current_dir(dir.clone())
        .assert()
        .success();

    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args(["publish", module_name, "--server", &spacetime.host_url.to_string()])
        .args(republish_args)
        .current_dir(dir);

    if expect_success {
        cmd.assert().success();
    } else {
        cmd.assert().failure();
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
