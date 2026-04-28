use serde_json::json;
use serde_json::Value;
use spacetimedb_smoketests::Smoketest;
use std::thread;
use std::time::Duration;

fn time_row_entry(scheduled_id: u64) -> Value {
    json!({
        "prev": {"__timestamp_micros_since_unix_epoch__": 0},
        "scheduled_id": scheduled_id,
        "sched_at": {"Time": {"__timestamp_micros_since_unix_epoch__": 0}},
    })
}

fn interval_row_entry(scheduled_id: u64, duration_micros: u64) -> Value {
    json!({
        "prev": {"__timestamp_micros_since_unix_epoch__": 0},
        "scheduled_id": scheduled_id,
        "sched_at": {"Interval": {"__time_duration_micros__": duration_micros}},
    })
}

fn collect_updates_after_call(test: &Smoketest, queries: &[&str], reducer_or_procedure: &str) -> Vec<Value> {
    let sub = test.subscribe_background(queries, 2).unwrap();
    test.call(reducer_or_procedure, &[]).unwrap();
    sub.collect().unwrap()
}

fn assert_table_and_view_insert_delete_updates(
    updates: Vec<Value>,
    table_name: &str,
    view_name: &str,
    row_entry: Value,
) {
    assert_eq!(
        serde_json::json!(updates),
        serde_json::json!([
            {
                table_name: {"deletes": [], "inserts": [row_entry.clone()]},
                view_name: {"deletes": [], "inserts": [row_entry.clone()]},
            },
            {
                table_name: {"deletes": [row_entry.clone()], "inserts": []},
                view_name: {"deletes": [row_entry], "inserts": []},
            },
        ])
    );
}

fn assert_table_insert_only_updates(updates: Vec<Value>, table_name: &str, first_row: Value, second_row: Value) {
    assert_eq!(
        serde_json::json!(updates),
        serde_json::json!([
            {table_name: {"deletes": [], "inserts": [first_row]}},
            {table_name: {"deletes": [], "inserts": [second_row]}},
        ])
    );
}

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

/// Test deploying a module with a scheduled reducer and check that the automatic cleanup
/// transaction updates both the scheduled table and its dependent view together.
#[test]
fn test_scheduled_table_and_view_subscription() {
    let test = Smoketest::builder().precompiled_module("schedule-subscribe").build();

    let updates = collect_updates_after_call(
        &test,
        &["SELECT * FROM scheduled_table", "SELECT * FROM scheduled_view"],
        "schedule_reducer",
    );

    // The insert and delete should update the scheduled table and its view in the same
    // subscription transactions.
    assert_table_and_view_insert_delete_updates(updates, "scheduled_table", "scheduled_view", time_row_entry(2));
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

    // subscription should have 2 updates and should not have any deletes
    assert_table_insert_only_updates(
        updates,
        "scheduled_table",
        interval_row_entry(1, 100_000),
        time_row_entry(2),
    );
}

/// Scheduled *procedure* subscription: expect insert + delete for both table and view.
#[test]
fn test_scheduled_procedure_table_and_view_subscription() {
    let test = Smoketest::builder().precompiled_module("schedule-procedure").build();

    let updates = collect_updates_after_call(
        &test,
        &["SELECT * FROM scheduled_table", "SELECT * FROM scheduled_view"],
        "schedule_procedure",
    );

    assert_table_and_view_insert_delete_updates(updates, "scheduled_table", "scheduled_view", time_row_entry(2));
}

/// Test that scheduled reducers refresh views for scheduled tables.
#[test]
fn test_view_refresh_for_scheduled_reducer() {
    let test = Smoketest::builder().precompiled_module("schedule-subscribe").build();

    test.call("seed_player_entity", &["2"]).unwrap();

    let updates = collect_updates_after_call(
        &test,
        &["SELECT * FROM scheduled_table", "SELECT * FROM scheduled_sender_view"],
        "schedule_reducer",
    );

    assert_table_and_view_insert_delete_updates(updates, "scheduled_table", "scheduled_sender_view", time_row_entry(2));
}

/// Test that cleanup still refreshes a view when the scheduled reducer fails.
#[test]
fn test_view_refresh_on_failed_scheduled_reducer() {
    let test = Smoketest::builder().precompiled_module("schedule-subscribe").build();

    test.call("seed_player_entity", &["3"]).unwrap();

    let updates = collect_updates_after_call(
        &test,
        &[
            "SELECT * FROM failing_scheduled_table",
            "SELECT * FROM failing_scheduled_sender_view",
        ],
        "schedule_failing_reducer",
    );

    assert_table_and_view_insert_delete_updates(
        updates,
        "failing_scheduled_table",
        "failing_scheduled_sender_view",
        time_row_entry(3),
    );

    let logs = test.logs(100).unwrap();
    let logs_str = logs.join("\n");
    assert!(
        logs_str.contains("scheduled reducer failed"),
        "Expected scheduled reducer failure to be logged, got: {:?}",
        logs
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

    assert_table_insert_only_updates(
        updates,
        "scheduled_table",
        interval_row_entry(1, 100_000),
        time_row_entry(2),
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
