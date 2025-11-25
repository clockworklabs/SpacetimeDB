mod util;

use crate::util::SpacetimeDbGuard;
use assert_cmd::cargo::cargo_bin_cmd;

#[test]
fn cli_can_publish_spacetimedb_on_disk() {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();

    // Workspace root for `cargo run -p ...`
    let workspace_dir = cargo_metadata::MetadataCommand::new().exec().unwrap().workspace_root;
    // dir = <workspace_root>/modules/quickstart-chat
    let dir = workspace_dir.join("modules").join("quickstart-chat");

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

// TODO: Somewhere we should test that --delete-data actually deletes the data in all cases
fn migration_test(module_name: &str, republish_args: &[&str], expect_success: bool) -> () {
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
fn cli_can_publish_with_automigration_change() {
    migration_test(
        "automigration-test",
        &["--build-options=--features test-add-column", "--yes-break-clients"],
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
fn cli_can_publish_no_conflict_does_not_delete_data() {
    migration_test(
        "no-conflict-test",
        &[
            "--delete-data=on-conflict",
            // NOTE: deleting data requires --yes,
            // so not providing it here ensures that no data deletion is attempted.
        ],
        true,
    );
}
