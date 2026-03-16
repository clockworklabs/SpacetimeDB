//! Tests for server restart behavior.
//! Translated from smoketests/tests/zz_docker.py

use spacetimedb_smoketests::{require_local_server, Smoketest};

/// Test data persistence across server restart.
///
/// This tests to see if SpacetimeDB can be queried after a restart.
#[test]
fn test_restart_module() {
    require_local_server!();
    let mut test = Smoketest::builder().precompiled_module("restart-person").build();

    test.call("add", &["Robert"]).unwrap();

    // Wait for data to be durable before restarting.
    // The --confirmed flag ensures we only see durable data.
    let output = test
        .sql_confirmed("SELECT * FROM person WHERE name = 'Robert'")
        .unwrap();
    assert!(
        output.contains("Robert"),
        "Data not confirmed before restart: {}",
        output
    );

    test.restart_server();

    test.call("add", &["Julie"]).unwrap();
    test.call("add", &["Samantha"]).unwrap();
    test.call("say_hello", &[]).unwrap();

    let logs = test.logs(100).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("Hello, Robert!")),
        "Missing 'Hello, Robert!' in logs"
    );
    assert!(
        logs.iter().any(|l| l.contains("Hello, Julie!")),
        "Missing 'Hello, Julie!' in logs"
    );
    assert!(
        logs.iter().any(|l| l.contains("Hello, Samantha!")),
        "Missing 'Hello, Samantha!' in logs"
    );
    assert!(
        logs.iter().any(|l| l.contains("Hello, World!")),
        "Missing 'Hello, World!' in logs"
    );
}

/// Test SQL queries work after restart.
#[test]
fn test_restart_sql() {
    require_local_server!();
    let mut test = Smoketest::builder().precompiled_module("restart-person").build();

    test.call("add", &["Robert"]).unwrap();
    test.call("add", &["Julie"]).unwrap();
    test.call("add", &["Samantha"]).unwrap();

    // Wait for all data to be durable before restarting.
    // Query the last inserted row to ensure all data is confirmed.
    let output = test
        .sql_confirmed("SELECT * FROM person WHERE name = 'Samantha'")
        .unwrap();
    assert!(
        output.contains("Samantha"),
        "Data not confirmed before restart: {}",
        output
    );

    test.restart_server();

    let output = test.sql("SELECT name FROM person WHERE id = 3").unwrap();
    assert!(
        output.contains("Samantha"),
        "Expected 'Samantha' in SQL output: {}",
        output
    );
}

/// Test clients are auto-disconnected on restart.
#[test]
fn test_restart_auto_disconnect() {
    require_local_server!();
    let mut test = Smoketest::builder()
        .precompiled_module("restart-connected-client")
        .build();

    // Start two subscribers in the background
    let sub1 = test
        .subscribe_background(&["SELECT * FROM connected_client"], 2)
        .unwrap();
    let sub2 = test
        .subscribe_background(&["SELECT * FROM connected_client"], 2)
        .unwrap();

    // Call print_num_connected and check we have 3 clients (2 subscribers + the call)
    test.call("print_num_connected", &[]).unwrap();
    let logs = test.logs(10).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("CONNECTED CLIENTS: 3")),
        "Expected 3 connected clients before restart, logs: {:?}",
        logs
    );

    // Restart the server - this should disconnect all clients
    test.restart_server();

    // The subscriptions should fail/complete since the server restarted
    // We don't wait for them, just drop the handles
    drop(sub1);
    drop(sub2);

    // After restart, only the current call should be connected
    test.call("print_num_connected", &[]).unwrap();
    let logs = test.logs(10).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("CONNECTED CLIENTS: 1")),
        "Expected 1 connected client after restart, logs: {:?}",
        logs
    );
}

const JOIN_QUERY: &str = "select t1.* from t1 join t2 on t1.id = t2.id where t2.id = 1001";

/// Test autoinc sequences work correctly after restart.
///
/// This is the `AddRemoveIndex` test from add_remove_index.py,
/// but restarts the server between each publish.
///
/// This detects a bug we once had where the system autoinc sequences
/// were borked after restart, leading newly-created database objects
/// to re-use IDs.
#[test]
fn test_add_remove_index_after_restart() {
    require_local_server!();
    let mut test = Smoketest::builder()
        .precompiled_module("add-remove-index")
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());

    // Publish and attempt subscribing to a join query.
    // There are no indices, resulting in an unsupported unindexed join.
    test.publish_module_named(&name, false).unwrap();
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(result.is_err(), "Expected subscription to fail without indices");

    // Restart before adding indices
    test.restart_server();

    // Publish the indexed version.
    // Now we have indices, so the query should be accepted.
    test.use_precompiled_module("add-remove-index-indexed");
    test.publish_module_named(&name, false).unwrap();

    // Subscription should work now
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(
        result.is_ok(),
        "Expected subscription to succeed with indices, got: {:?}",
        result.err()
    );

    // Verify call works too
    let sub = test.subscribe_background(&[JOIN_QUERY], 1).unwrap();
    test.call_anon("add", &[]).unwrap();
    let results = sub.collect().unwrap();
    assert_eq!(results.len(), 1, "Expected 1 update from subscription");

    // Restart before removing indices
    test.restart_server();

    // Publish the unindexed version again, removing the index.
    // The initial subscription should be rejected again.
    test.use_precompiled_module("add-remove-index");
    test.publish_module_named(&name, false).unwrap();
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(result.is_err(), "Expected subscription to fail after removing indices");
}
