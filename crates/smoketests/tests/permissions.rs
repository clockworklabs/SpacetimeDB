use spacetimedb_smoketests::Smoketest;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Ensure that anyone has the permission to call any standard reducer
#[test]
fn test_call() {
    let test = Smoketest::builder().precompiled_module("modules-basic").build();

    test.call_anon("say_hello", &[]).unwrap();

    let logs = test.logs(10000).unwrap();
    let world_count = logs.iter().filter(|l| l.contains("World")).count();
    assert_eq!(world_count, 1, "Expected 1 'World' in logs, got {}", world_count);
}

/// Ensure that anyone can describe any database
#[test]
fn test_describe() {
    let test = Smoketest::builder().precompiled_module("modules-basic").build();

    // Should succeed with anonymous describe
    test.describe_anon().unwrap();
}

/// Ensure that we are not able to view the logs of a module that we don't have permission to view
#[test]
fn test_logs() {
    let test = Smoketest::builder().precompiled_module("modules-basic").build();

    // Call say_hello as owner
    test.call("say_hello", &[]).unwrap();

    // Switch to a new identity
    test.new_identity().unwrap();

    // Call say_hello as new identity (should work - reducers are public)
    test.call("say_hello", &[]).unwrap();

    // Switch to another new identity
    test.new_identity().unwrap();

    // Try to view logs - should fail as non-owner
    let identity = test.database_identity.as_ref().unwrap();
    let result = test.spacetime(&["logs", "--server", &test.server_url, identity, "-n", "10000"]);
    assert!(result.is_err(), "Expected logs to fail for non-owner");
}

/// Ensure that you cannot publish to an identity that you do not own
#[test]
fn test_publish() {
    let test = Smoketest::builder().precompiled_module("modules-basic").build();

    let identity = test.database_identity.as_ref().unwrap().clone();

    // Switch to a new identity
    test.new_identity().unwrap();

    // Try to publish with --delete-data - should fail
    let project_path = workspace_root().join("crates/smoketests/modules/modules-basic");
    let result = test.spacetime(&[
        "publish",
        &identity,
        "--server",
        &test.server_url,
        "--project-path",
        project_path.to_str().unwrap(),
        "--delete-data",
        "--yes",
    ]);
    assert!(
        result.is_err(),
        "Expected publish with --delete-data to fail for non-owner"
    );

    // Try to publish without --delete-data - should also fail
    let result = test.spacetime(&[
        "publish",
        &identity,
        "--server",
        &test.server_url,
        "--project-path",
        project_path.to_str().unwrap(),
        "--yes",
    ]);
    assert!(result.is_err(), "Expected publish to fail for non-owner");
}

/// Test that you can't replace names of a database you don't own
#[test]
fn test_replace_names() {
    let mut test = Smoketest::builder()
        .precompiled_module("modules-basic")
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());
    test.publish_module_named(&name, false).unwrap();

    // Switch to a new identity
    test.new_identity().unwrap();

    // Try to replace names - should fail
    let json_body = r#"["post", "gres"]"#;
    let response = test
        .api_call_json("PUT", &format!("/v1/database/{}/names", name), json_body)
        .unwrap();
    assert!(
        response.status_code != 200,
        "Expected replace names to fail for non-owner, got status {}",
        response.status_code
    );
}

/// Ensure that a private table can only be queried by the database owner
#[test]
fn test_private_table() {
    let test = Smoketest::builder().precompiled_module("permissions-private").build();

    // Owner can query private table
    test.assert_sql(
        "SELECT * FROM secret",
        r#" answer
--------
 42"#,
    );

    // Switch to a new identity
    test.new_identity().unwrap();

    // Non-owner cannot query private table
    let result = test.sql("SELECT * FROM secret");
    assert!(result.is_err(), "Expected query on private table to fail for non-owner");

    // Subscribing to the private table fails
    let result = test.subscribe(&["SELECT * FROM secret"], 0);
    assert!(
        result.is_err(),
        "Expected subscribe to private table to fail for non-owner"
    );

    // Subscribing to the public table works
    let sub = test
        .subscribe_background(&["SELECT * FROM common_knowledge"], 1)
        .unwrap();
    test.call("do_thing", &["godmorgon"]).unwrap();
    let events = sub.collect().unwrap();

    assert_eq!(
        serde_json::json!(events),
        serde_json::json!([{
            "common_knowledge": {
                "deletes": [],
                "inserts": [{"thing": "godmorgon"}]
            }
        }])
    );

    // Subscribing to both tables returns updates for the public one only
    let sub = test.subscribe_background(&["SELECT * FROM *"], 1).unwrap();
    test.call("do_thing", &["howdy"]).unwrap();
    let events = sub.collect().unwrap();

    assert_eq!(
        serde_json::json!(events),
        serde_json::json!([{
            "common_knowledge": {
                "deletes": [],
                "inserts": [{"thing": "howdy"}]
            }
        }])
    );
}

/// Ensure that you cannot delete a database that you do not own
#[test]
fn test_cannot_delete_others_database() {
    let test = Smoketest::builder().build();

    let identity = test.database_identity.as_ref().unwrap().clone();

    // Switch to a new identity
    test.new_identity().unwrap();

    // Try to delete the database - should fail
    let result = test.spacetime(&["delete", "--server", &test.server_url, &identity, "--yes"]);
    assert!(result.is_err(), "Expected delete to fail for non-owner");
}

/// Ensure that lifecycle reducers (init, on_connect, etc) can't be called directly
#[test]
fn test_lifecycle_reducers_cant_be_called() {
    let test = Smoketest::builder().precompiled_module("permissions-lifecycle").build();

    let lifecycle_kinds = ["init", "client_connected", "client_disconnected"];

    for kind in lifecycle_kinds {
        let reducer_name = format!("lifecycle_{}", kind);
        let result = test.call(&reducer_name, &[]);
        assert!(
            result.is_err(),
            "Expected call to lifecycle reducer '{}' to fail",
            reducer_name
        );
    }
}
