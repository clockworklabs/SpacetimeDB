//! Tests translated from smoketests/tests/rls.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
use spacetimedb::{Identity, ReducerContext, Table};

#[spacetimedb::table(name = users, public)]
pub struct Users {
    name: String,
    identity: Identity,
}

#[spacetimedb::client_visibility_filter]
const USER_FILTER: spacetimedb::Filter = spacetimedb::Filter::Sql(
    "SELECT * FROM users WHERE identity = :sender"
);

#[spacetimedb::reducer]
pub fn add_user(ctx: &ReducerContext, name: String) {
    ctx.db.users().insert(Users { name, identity: ctx.sender });
}
"#;

/// Tests for querying tables with RLS rules
#[test]
fn test_rls_rules() {
    let test = Smoketest::builder().module_code(MODULE_CODE).build();

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
