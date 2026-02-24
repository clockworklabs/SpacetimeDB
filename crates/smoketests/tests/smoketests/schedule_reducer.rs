use serde_json::json;
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
    let test = Smoketest::builder().precompiled_module("schedule-subscribe").build();

    // Subscribe to empty scheduled_table.
    let sub = test
        .subscribe_background(&["SELECT * FROM scheduled_table"], 2)
        .unwrap();

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

    let updates = sub.collect().unwrap();

    let row_entry = json!({
        "prev": {"__timestamp_micros_since_unix_epoch__": 0},
        "scheduled_id": 2,
        "sched_at": {"Time": {"__timestamp_micros_since_unix_epoch__": 0}},
    });

    // subscription should have 2 updates, first for row insert in scheduled table and second for row deletion.
    assert_eq!(
        serde_json::json!(updates),
        serde_json::json!([
            {"scheduled_table": {"deletes": [], "inserts": [row_entry.clone()]}},
            {"scheduled_table": {"deletes": [row_entry], "inserts": []}},
        ])
    );
}

/// Test that repeated reducers run multiple times
#[test]
fn test_scheduled_table_subscription_repeated_reducer() {
    let test = Smoketest::builder().precompiled_module("schedule-subscribe").build();

    // Subscribe to empty scheduled_table.
    let sub = test
        .subscribe_background(&["SELECT * FROM scheduled_table"], 2)
        .unwrap();

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

    // Scheduling a one-shot reducer again just to get the 2nd subscription update.
    test.call("schedule_reducer", &[]).unwrap();

    let updates = sub.collect().unwrap();

    let repeated_row_entry = json!({
        "prev": {"__timestamp_micros_since_unix_epoch__": 0},
        "scheduled_id": 1,
        "sched_at": {"Interval": {"__time_duration_micros__": 100000}},
    });

    let row_entry = json!({
        "prev": {"__timestamp_micros_since_unix_epoch__": 0},
        "scheduled_id": 2,
        "sched_at": {"Time": {"__timestamp_micros_since_unix_epoch__": 0}},
    });

    // subscription should have 2 updates and should not have any deletes
    assert_eq!(
        serde_json::json!(updates),
        serde_json::json!([
            {"scheduled_table": {"deletes": [], "inserts": [repeated_row_entry]}},
            {"scheduled_table": {"deletes": [], "inserts": [row_entry]}},
        ])
    );
}

/// Scheduled *procedure* subscription: expect insert + delete.
#[test]
fn test_scheduled_procedure_table_subscription() {
    let test = Smoketest::builder().precompiled_module("schedule-procedure").build();

    // Subscribe to empty table.
    let sub = test
        .subscribe_background(&["SELECT * FROM scheduled_table"], 2)
        .unwrap();

    test.call("schedule_procedure", &[]).unwrap();

    thread::sleep(Duration::from_secs(2));

    let logs = test.logs(100).unwrap();
    let invoked_count = logs.iter().filter(|line| line.contains("Invoked:")).count();
    assert_eq!(
        invoked_count, 1,
        "Expected scheduled procedure to run exactly once, but it ran {} times. Logs: {:?}",
        invoked_count, logs
    );

    let updates = sub.collect().unwrap();

    let row_entry = json!({
        "prev": {"__timestamp_micros_since_unix_epoch__": 0},
        "scheduled_id": 2,
        "sched_at": {"Time": {"__timestamp_micros_since_unix_epoch__": 0}},
    });

    assert_eq!(
        serde_json::json!(updates),
        serde_json::json!([
            {"scheduled_table": {"deletes": [], "inserts": [row_entry.clone()]}},
            {"scheduled_table": {"deletes": [row_entry], "inserts": []}},
        ])
    );
}

/// Repeated scheduled *procedure* subscription: expect inserts only (no deletes).
#[test]
fn test_scheduled_procedure_table_subscription_repeated_procedure() {
    let test = Smoketest::builder().precompiled_module("schedule-procedure").build();

    let sub = test
        .subscribe_background(&["SELECT * FROM scheduled_table"], 2)
        .unwrap();

    test.call("schedule_repeated_procedure", &[]).unwrap();

    thread::sleep(Duration::from_secs(2));

    let logs = test.logs(100).unwrap();
    let invoked_count = logs.iter().filter(|line| line.contains("Invoked:")).count();
    assert!(
        invoked_count > 2,
        "Expected repeated procedure to run more than twice, but it ran {} times. Logs: {:?}",
        invoked_count,
        logs
    );

    // Trigger another update so we get the expected 2 subscription updates.
    test.call("schedule_procedure", &[]).unwrap();

    let updates = sub.collect().unwrap();

    let repeated_row_entry = json!({
        "prev": {"__timestamp_micros_since_unix_epoch__": 0},
        "scheduled_id": 1,
        "sched_at": {"Interval": {"__time_duration_micros__": 100000}},
    });

    let row_entry = json!({
        "prev": {"__timestamp_micros_since_unix_epoch__": 0},
        "scheduled_id": 2,
        "sched_at": {"Time": {"__timestamp_micros_since_unix_epoch__": 0}},
    });

    assert_eq!(
        serde_json::json!(updates),
        serde_json::json!([
            {"scheduled_table": {"deletes": [], "inserts": [repeated_row_entry]}},
            {"scheduled_table": {"deletes": [], "inserts": [row_entry]}},
        ])
    );
}

/// Check that volatile_nonatomic_schedule_immediate works
#[test]
fn test_volatile_nonatomic_schedule_immediate() {
    let test = Smoketest::builder().precompiled_module("schedule-volatile").build();

    let sub = test.subscribe_background(&["SELECT * FROM my_table"], 2).unwrap();

    // Insert directly first
    test.call("do_insert", &[r#""yay!""#]).unwrap();

    // Schedule another insert
    test.call("do_schedule", &[]).unwrap();

    let updates = sub.collect().unwrap();
    assert_eq!(
        serde_json::json!(updates),
        serde_json::json!([
            {"my_table": {"deletes": [], "inserts": [{"x": "yay!"}] }},
            {"my_table": {"deletes": [], "inserts": [{"x": "hello"}] }},
        ])
    );
}
