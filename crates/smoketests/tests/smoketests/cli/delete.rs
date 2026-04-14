use spacetimedb_smoketests::Smoketest;
use std::thread;
use std::time::Duration;

fn assert_delete_prompt(output: &str, database: &str) {
    assert!(
        output.contains("Are you sure you want to delete database"),
        "expected confirmation prompt in output:\n{output}"
    );
    assert!(
        output.contains(database),
        "expected database name in confirmation prompt:\n{output}"
    );
}

#[test]
fn test_delete_database_aborts_without_confirmation() {
    let mut test = Smoketest::builder()
        .precompiled_module("delete-database")
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());
    test.publish_module_named(&name, false).unwrap();

    let output = test
        .spacetime(&["delete", "--server", &test.server_url, &name]);
        .unwrap();
    assert_delete_prompt(&output, &name);
    assert!(output.contains("Aborting"), "expected abort message:\n{output}");

    test.spacetime(&["logs", "--server", &test.server_url, &name]).unwrap();
}

/// Test that deleting a database stops the module.
/// The module is considered stopped if its scheduled reducer stops
/// producing update events.
#[test]
fn test_delete_database_with_confirmation() {
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
    let output = test
        .spacetime_with_stdin(&["delete", "--server", &test.server_url, &name], "y\n")
        .unwrap();
    assert_delete_prompt(&output, &name);

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

#[test]
fn test_delete_database_yes_skips_confirmation() {
    let mut test = Smoketest::builder()
        .precompiled_module("delete-database")
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());
    test.publish_module_named(&name, false).unwrap();

    let output = test
        .spacetime(&["delete", "--server", &test.server_url, "--yes", &name])
        .unwrap();
    assert!(
        output.contains("Skipping confirmation due to --yes"),
        "expected --yes skip message:\n{output}"
    );

    let result = test.spacetime(&["logs", "--server", &test.server_url, &name]);
    assert!(result.is_err(), "expected database to be deleted");
}
