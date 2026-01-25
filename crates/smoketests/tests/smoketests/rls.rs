//! Tests translated from smoketests/tests/rls.py

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
