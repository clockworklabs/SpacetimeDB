//! Tests translated from smoketests/tests/permissions.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE_PRIVATE: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = secret, private)]
pub struct Secret {
    answer: u8,
}

#[spacetimedb::table(name = common_knowledge, public)]
pub struct CommonKnowledge {
    thing: String,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.secret().insert(Secret { answer: 42 });
}

#[spacetimedb::reducer]
pub fn do_thing(ctx: &ReducerContext, thing: String) {
    ctx.db.secret().insert(Secret { answer: 20 });
    ctx.db.common_knowledge().insert(CommonKnowledge { thing });
}
"#;

/// Ensure that a private table can only be queried by the database owner
#[test]
fn test_private_table() {
    let test = Smoketest::builder().module_code(MODULE_CODE_PRIVATE).build();

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
    assert_eq!(events.len(), 1, "Expected 1 update, got {:?}", events);

    let expected = serde_json::json!({
        "common_knowledge": {
            "deletes": [],
            "inserts": [{"thing": "godmorgon"}]
        }
    });
    assert_eq!(events[0], expected);
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

const MODULE_CODE_LIFECYCLE: &str = r#"
#[spacetimedb::reducer(init)]
fn lifecycle_init(_ctx: &spacetimedb::ReducerContext) {}

#[spacetimedb::reducer(client_connected)]
fn lifecycle_client_connected(_ctx: &spacetimedb::ReducerContext) {}

#[spacetimedb::reducer(client_disconnected)]
fn lifecycle_client_disconnected(_ctx: &spacetimedb::ReducerContext) {}
"#;

/// Ensure that lifecycle reducers (init, on_connect, etc) can't be called directly
#[test]
fn test_lifecycle_reducers_cant_be_called() {
    let test = Smoketest::builder().module_code(MODULE_CODE_LIFECYCLE).build();

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
