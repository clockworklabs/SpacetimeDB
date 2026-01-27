//! Tests translated from smoketests/tests/confirmed_reads.py
//!
//! TODO: We only test that we can pass a --confirmed flag and that things
//! appear to work as if we hadn't. Without controlling the server, we can't
//! test that there is any difference in behavior.

use spacetimedb_smoketests::Smoketest;

/// Tests that subscribing with confirmed=true receives updates
#[test]
fn test_confirmed_reads_receive_updates() {
    let test = Smoketest::builder().precompiled_module("confirmed-reads").build();

    // Start subscription in background with confirmed flag
    let sub = test
        .subscribe_background_confirmed(&["SELECT * FROM person"], 2)
        .unwrap();

    // Insert via reducer
    test.call("add", &["Horst"]).unwrap();

    // Insert via SQL (use sql_confirmed to ensure durability before continuing,
    // since the confirmed subscription won't send updates until durable)
    test.sql_confirmed("INSERT INTO person (name) VALUES ('Egon')").unwrap();

    // Collect updates
    let events = sub.collect().unwrap();

    assert_eq!(events.len(), 2, "Expected 2 updates, got {:?}", events);

    // Check that we got the expected inserts
    let horst_insert = serde_json::json!({
        "person": {
            "deletes": [],
            "inserts": [{"name": "Horst"}]
        }
    });
    let egon_insert = serde_json::json!({
        "person": {
            "deletes": [],
            "inserts": [{"name": "Egon"}]
        }
    });

    assert_eq!(events[0], horst_insert);
    assert_eq!(events[1], egon_insert);
}

/// Tests that an SQL operation with confirmed=true returns a result
#[test]
fn test_sql_with_confirmed_reads_receives_result() {
    let test = Smoketest::builder().precompiled_module("confirmed-reads").build();

    // Insert with confirmed
    test.sql_confirmed("INSERT INTO person (name) VALUES ('Horst')")
        .unwrap();

    // Query with confirmed
    let result = test.sql_confirmed("SELECT * FROM person").unwrap();

    assert!(result.contains("Horst"), "Expected 'Horst' in result: {}", result);
}
