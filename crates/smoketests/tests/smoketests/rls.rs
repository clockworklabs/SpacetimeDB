use spacetimedb_smoketests::Smoketest;

/// Tests for querying tables with RLS rules
#[test]
fn test_rls_rules() {
    let test = Smoketest::builder().precompiled_module("rls").build();

    // Insert a user for Alice (current identity)
    test.call("add_user", &["Alice"]).unwrap();

    // Create a new identity for Bob
    test.new_identity().unwrap();
    test.call("add_user", &["Bob"]).unwrap();

    // Query the users table using Bob's identity - should only see Bob
    test.assert_sql(
        "SELECT name FROM users",
        r#" name
-------
 "Bob""#,
    );

    // Create another new identity - should see no users
    test.new_identity().unwrap();
    test.assert_sql(
        "SELECT name FROM users",
        r#" name
------"#,
    );
}

/// Module code with RLS on a private table (intentionally broken)
const MODULE_CODE_BROKEN_RLS: &str = r#"
use spacetimedb::{client_visibility_filter, Filter, Identity};

#[spacetimedb::table(accessor = user)]
pub struct User {
    identity: Identity,
}

#[client_visibility_filter]
const PERSON_FILTER: Filter = Filter::Sql("SELECT * FROM \"user\" WHERE identity = :sender");
"#;

/// Tests that publishing an RLS rule on a private table fails
#[test]
fn test_publish_fails_for_rls_on_private_table() {
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE_BROKEN_RLS)
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());

    // Publishing should fail because RLS is on a private table
    let result = test.publish_module_named(&name, false);
    assert!(result.is_err(), "Expected publish to fail for RLS on private table");
}

/// Tests that changing the RLS rules disconnects existing clients
#[test]
fn test_rls_disconnect_if_change() {
    let mut test = Smoketest::builder()
        .precompiled_module("rls-no-filter")
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());

    // Initial publish without RLS
    test.publish_module_named(&name, false).unwrap();

    // Now re-publish with RLS added (requires --break-clients)
    test.use_precompiled_module("rls-with-filter");
    test.publish_module_with_options(&name, false, true).unwrap();

    // Check the row-level SQL filter is added correctly
    test.assert_sql(
        "SELECT sql FROM st_row_level_security",
        r#" sql
------------------------------------------------
 "SELECT * FROM users WHERE identity = :sender""#,
    );

    let logs = test.logs(100).unwrap();

    // Validate disconnect + schema migration logs
    assert!(
        logs.iter().any(|l| l.contains("Disconnecting all users")),
        "Expected 'Disconnecting all users' in logs: {:?}",
        logs
    );
}

/// Tests that not changing the RLS rules does not disconnect existing clients
#[test]
fn test_rls_no_disconnect() {
    let mut test = Smoketest::builder()
        .precompiled_module("rls-with-filter")
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());

    // Initial publish with RLS
    test.publish_module_named(&name, false).unwrap();

    // Re-publish the same module (no RLS change)
    test.publish_module_named(&name, false).unwrap();

    let logs = test.logs(100).unwrap();

    // Validate no disconnect logs
    assert!(
        !logs.iter().any(|l| l.contains("Disconnecting all users")),
        "Expected no 'Disconnecting all users' in logs: {:?}",
        logs
    );
}
