//! DML tests translated from smoketests/tests/dml.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = t, public)]
pub struct T {
    name: String,
}
"#;

/// Test that we receive subscription updates from DML
#[test]
fn test_subscribe() {
    use std::thread;
    use std::time::Duration;

    let test = Smoketest::builder().module_code(MODULE_CODE).build();

    // Start subscription FIRST (in background), matching Python semantics
    let sub = test.subscribe_background(&["SELECT * FROM t"], 2).unwrap();

    // Small delay to ensure subscription is connected before inserts
    thread::sleep(Duration::from_millis(500));

    // Then do the SQL inserts while subscription is running
    test.sql("INSERT INTO t (name) VALUES ('Alice')").unwrap();
    test.sql("INSERT INTO t (name) VALUES ('Bob')").unwrap();

    // Collect the subscription results
    let updates = sub.collect().unwrap();

    assert_eq!(
        updates,
        vec![
            serde_json::json!({"t": {"deletes": [], "inserts": [{"name": "Alice"}]}}),
            serde_json::json!({"t": {"deletes": [], "inserts": [{"name": "Bob"}]}}),
        ],
        "Expected subscription updates for Alice and Bob inserts"
    );
}
