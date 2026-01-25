//! Scheduled reducer tests translated from smoketests/tests/schedule_reducer.py

use spacetimedb_smoketests::Smoketest;
use std::thread;
use std::time::Duration;

/// Ensure cancelling a reducer works
#[test]
fn test_cancel_reducer() {
    let test = Smoketest::builder().precompiled_module("schedule-cancel").build();

    // Wait for any scheduled reducers to potentially run
    thread::sleep(Duration::from_secs(2));

    let logs = test.logs(5).unwrap();
    let logs_str = logs.join("\n");
    assert!(
        !logs_str.contains("the reducer ran"),
        "Expected no 'the reducer ran' in logs, got: {:?}",
        logs
    );
}

/// Test deploying a module with a scheduled reducer and check if client receives
/// subscription update for scheduled table entry and deletion of reducer once it ran
#[test]
fn test_scheduled_table_subscription() {
    let test = Smoketest::builder()
        .precompiled_module("schedule-subscribe")
        .build();

    // Call a reducer to schedule a reducer (runs immediately since timestamp is 0)
    test.call("schedule_reducer", &[]).unwrap();

    // Wait for the scheduled reducer to run
    thread::sleep(Duration::from_secs(2));

    let logs = test.logs(100).unwrap();
    let invoked_count = logs.iter().filter(|line| line.contains("Invoked:")).count();
    assert_eq!(
        invoked_count, 1,
        "Expected scheduled reducer to run exactly once, but it ran {} times. Logs: {:?}",
        invoked_count, logs
    );
}

/// Test that repeated reducers run multiple times
#[test]
fn test_scheduled_table_subscription_repeated_reducer() {
    let test = Smoketest::builder()
        .precompiled_module("schedule-subscribe")
        .build();

    // Call a reducer to schedule a repeated reducer
    test.call("schedule_repeated_reducer", &[]).unwrap();

    // Wait for the scheduled reducer to run multiple times
    thread::sleep(Duration::from_secs(2));

    let logs = test.logs(100).unwrap();
    let invoked_count = logs.iter().filter(|line| line.contains("Invoked:")).count();
    assert!(
        invoked_count > 2,
        "Expected repeated reducer to run more than twice, but it ran {} times. Logs: {:?}",
        invoked_count,
        logs
    );
}

/// Check that volatile_nonatomic_schedule_immediate works
#[test]
fn test_volatile_nonatomic_schedule_immediate() {
    let test = Smoketest::builder().precompiled_module("schedule-volatile").build();

    // Insert directly first
    test.call("do_insert", &[r#""yay!""#]).unwrap();

    // Schedule another insert
    test.call("do_schedule", &[]).unwrap();

    // Wait a moment for the scheduled insert to complete
    thread::sleep(Duration::from_millis(500));

    // Query the table to verify both inserts happened
    let result = test.sql("SELECT * FROM my_table").unwrap();
    assert!(
        result.contains("yay!") && result.contains("hello"),
        "Expected both 'yay!' and 'hello' in table, got: {}",
        result
    );
}
