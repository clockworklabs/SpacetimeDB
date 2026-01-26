//! Tests translated from smoketests/tests/delete_database.py

use spacetimedb_smoketests::Smoketest;
use std::thread;
use std::time::Duration;

/// Test that deleting a database stops the module.
/// The module is considered stopped if its scheduled reducer stops
/// producing update events.
#[test]
fn test_delete_database() {
    let mut test = Smoketest::builder()
        .precompiled_module("delete-database")
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());
    test.publish_module_named(&name, false).unwrap();

    // Start subscription in background to collect updates
    // We request many updates but will stop early when we delete the db
    let sub = test.subscribe_background(&["SELECT * FROM counter"], 1000).unwrap();

    // Let the scheduled reducer run for a bit
    thread::sleep(Duration::from_secs(2));

    // Delete the database
    test.spacetime(&["delete", "--server", &test.server_url, &name])
        .unwrap();

    // Collect whatever updates we got
    let updates = sub.collect().unwrap();

    // At a rate of 100ms, we shouldn't have more than 20 updates in 2secs.
    // But let's say 50, in case the delete gets delayed for some reason.
    assert!(
        updates.len() <= 50,
        "Expected at most 50 updates, got {}. Database may not have stopped.",
        updates.len()
    );
}
