//! Tests translated from smoketests/tests/domains.py

use spacetimedb_smoketests::Smoketest;

/// Tests the functionality of the rename command
#[test]
fn test_set_name() {
    let mut test = Smoketest::builder().autopublish(false).build();

    let orig_name = format!("test-db-{}", std::process::id());
    test.publish_module_named(&orig_name, false).unwrap();

    let rand_name = format!("test-db-{}-renamed", std::process::id());

    // This should fail before there's a db with this name
    let result = test.spacetime(&["logs", "--server", &test.server_url, &rand_name]);
    assert!(result.is_err(), "Expected logs to fail for non-existent name");

    // Rename the database
    let identity = test.database_identity.as_ref().unwrap();
    test.spacetime(&["rename", "--server", &test.server_url, "--to", &rand_name, identity])
        .unwrap();

    // Now logs should work with the new name
    test.spacetime(&["logs", "--server", &test.server_url, &rand_name])
        .unwrap();

    // Original name should no longer work
    let result = test.spacetime(&["logs", "--server", &test.server_url, &orig_name]);
    assert!(result.is_err(), "Expected logs to fail for original name after rename");
}

/// Test how we treat the / character in published names
#[test]
fn test_subdomain_behavior() {
    let mut test = Smoketest::builder().autopublish(false).build();

    let root_name = format!("test-db-{}", std::process::id());
    test.publish_module_named(&root_name, false).unwrap();

    // Double slash should fail
    let double_slash_name = format!("{}//test", root_name);
    let result = test.publish_module_named(&double_slash_name, false);
    assert!(result.is_err(), "Expected publish to fail with double slash in name");

    // Trailing slash should fail
    let trailing_slash_name = format!("{}/test/", root_name);
    let result = test.publish_module_named(&trailing_slash_name, false);
    assert!(result.is_err(), "Expected publish to fail with trailing slash in name");
}

/// Test that we can't rename to a name already in use
#[test]
fn test_set_to_existing_name() {
    let mut test = Smoketest::builder().autopublish(false).build();

    // Publish first database (no name)
    test.publish_module().unwrap();
    let id_to_rename = test.database_identity.clone().unwrap();

    // Publish second database with a name
    let rename_to = format!("test-db-{}-target", std::process::id());
    test.publish_module_named(&rename_to, false).unwrap();

    // Try to rename first db to the name of the second - should fail
    let result = test.spacetime(&[
        "rename",
        "--server",
        &test.server_url,
        "--to",
        &rename_to,
        &id_to_rename,
    ]);
    assert!(
        result.is_err(),
        "Expected rename to fail when target name is already in use"
    );
}
