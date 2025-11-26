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

#[test]
fn cli_can_publish_with_automigration_change() {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();

    // Workspace root for `cargo run -p ...`
    let workspace_dir = cargo_metadata::MetadataCommand::new().exec().unwrap().workspace_root;
    let dir = workspace_dir.join("modules").join("module-test");

    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "publish",
        "--server",
        &spacetime.host_url.to_string(),
        "automigration-test",
    ])
    .current_dir(dir.clone())
    .assert()
    .success();

    // Can republish with automigration change
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "publish",
        "--build-options=--features test-add-column",
        "--server",
        &spacetime.host_url.to_string(),
        "--yes-break-clients",
        "automigration-test",
    ])
    .current_dir(dir)
    .assert()
    .success();
}

#[test]
fn cli_cannot_publish_breaking_change_without_flag() {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();

    // Workspace root for `cargo run -p ...`
    let workspace_dir = cargo_metadata::MetadataCommand::new().exec().unwrap().workspace_root;
    let dir = workspace_dir.join("modules").join("module-test");

    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "publish",
        "--server",
        &spacetime.host_url.to_string(),
        "breaking-change-test",
    ])
    .current_dir(dir.clone())
    .assert()
    .success();

    // Cannot republish with breaking change without flag
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "publish",
        "--build-options=--features test-remove-table",
        "--server",
        &spacetime.host_url.to_string(),
        "breaking-change-test",
    ])
    .current_dir(dir)
    .assert()
    .failure();
}

#[test]
fn cli_can_publish_breaking_change_with_delete_data_flag() {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();

    // Workspace root for `cargo run -p ...`
    let workspace_dir = cargo_metadata::MetadataCommand::new().exec().unwrap().workspace_root;
    let dir = workspace_dir.join("modules").join("module-test");

    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "publish",
        "--server",
        &spacetime.host_url.to_string(),
        "breaking-change-delete-data-test",
    ])
    .current_dir(dir.clone())
    .assert()
    .success();

    // Can republish with breaking change with --delete-data flag
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "publish",
        "--build-options=--features test-remove-table",
        "--server",
        &spacetime.host_url.to_string(),
        "--delete-data",
        "--yes",
        "breaking-change-delete-data-test",
    ])
    .current_dir(dir)
    .assert()
    .success();
}

#[test]
fn cli_can_publish_breaking_change_with_on_conflict_flag() {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();

    // Workspace root for `cargo run -p ...`
    let workspace_dir = cargo_metadata::MetadataCommand::new().exec().unwrap().workspace_root;
    let dir = workspace_dir.join("modules").join("module-test");

    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "publish",
        "--server",
        &spacetime.host_url.to_string(),
        "breaking-change-on-conflict-test",
    ])
    .current_dir(dir.clone())
    .assert()
    .success();

    // Can republish with breaking change with --on-conflict=delete-data flag
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "publish",
        "--build-options=--features test-remove-table",
        "--server",
        &spacetime.host_url.to_string(),
        "--delete-data=on-conflict",
        "--yes",
        "breaking-change-on-conflict-test",
    ])
    .current_dir(dir)
    .assert()
    .success();
}

#[test]
fn cli_can_publish_no_conflict_does_not_delete_data() {
    let spacetime = SpacetimeDbGuard::spawn_in_temp_data_dir();

    // Workspace root for `cargo run -p ...`
    let workspace_dir = cargo_metadata::MetadataCommand::new().exec().unwrap().workspace_root;
    let dir = workspace_dir.join("modules").join("module-test");

    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "publish",
        "--server",
        &spacetime.host_url.to_string(),
        "no-conflict-test",
    ])
    .current_dir(dir.clone())
    .assert()
    .success();

    // Can republish without conflict even with --on-conflict=delete-data flag
    let mut cmd = cargo_bin_cmd!("spacetimedb-cli");
    cmd.args([
        "publish",
        "--server",
        &spacetime.host_url.to_string(),
        "--delete-data=on-conflict",
        // NOTE: deleting data requires --yes,
        // so not providing it here ensures that no data deletion is attempted.
        "no-conflict-test",
    ])
    .current_dir(dir)
    .assert()
    .success();
}
