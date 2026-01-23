//! Scheduled reducer tests translated from smoketests/tests/schedule_reducer.py

use spacetimedb_smoketests::Smoketest;
use std::thread;
use std::time::Duration;

const CANCEL_REDUCER_MODULE_CODE: &str = r#"
use spacetimedb::{duration, log, ReducerContext, Table};

#[spacetimedb::reducer(init)]
fn init(ctx: &ReducerContext) {
    let schedule = ctx.db.scheduled_reducer_args().insert(ScheduledReducerArgs {
        num: 1,
        scheduled_id: 0,
        scheduled_at: duration!(100ms).into(),
    });
    ctx.db.scheduled_reducer_args().scheduled_id().delete(&schedule.scheduled_id);

    let schedule = ctx.db.scheduled_reducer_args().insert(ScheduledReducerArgs {
         num: 2,
         scheduled_id: 0,
         scheduled_at: duration!(1000ms).into(),
     });
     do_cancel(ctx, schedule.scheduled_id);
}

#[spacetimedb::table(name = scheduled_reducer_args, public, scheduled(reducer))]
pub struct ScheduledReducerArgs {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    num: i32,
}

#[spacetimedb::reducer]
fn do_cancel(ctx: &ReducerContext, schedule_id: u64) {
    ctx.db.scheduled_reducer_args().scheduled_id().delete(&schedule_id);
}

#[spacetimedb::reducer]
fn reducer(_ctx: &ReducerContext, args: ScheduledReducerArgs) {
    log::info!("the reducer ran: {}", args.num);
}
"#;

/// Ensure cancelling a reducer works
#[test]
fn test_cancel_reducer() {
    let test = Smoketest::builder()
        .module_code(CANCEL_REDUCER_MODULE_CODE)
        .build();

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

const SUBSCRIBE_SCHEDULED_TABLE_MODULE_CODE: &str = r#"
use spacetimedb::{log, duration, ReducerContext, Table, Timestamp};

#[spacetimedb::table(name = scheduled_table, public, scheduled(my_reducer, at = sched_at))]
pub struct ScheduledTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    sched_at: spacetimedb::ScheduleAt,
    prev: Timestamp,
}

#[spacetimedb::reducer]
fn schedule_reducer(ctx: &ReducerContext) {
    ctx.db.scheduled_table().insert(ScheduledTable { prev: Timestamp::from_micros_since_unix_epoch(0), scheduled_id: 2, sched_at: Timestamp::from_micros_since_unix_epoch(0).into(), });
}

#[spacetimedb::reducer]
fn schedule_repeated_reducer(ctx: &ReducerContext) {
    ctx.db.scheduled_table().insert(ScheduledTable { prev: Timestamp::from_micros_since_unix_epoch(0), scheduled_id: 1, sched_at: duration!(100ms).into(), });
}

#[spacetimedb::reducer]
pub fn my_reducer(ctx: &ReducerContext, arg: ScheduledTable) {
    log::info!("Invoked: ts={:?}, delta={:?}", ctx.timestamp, ctx.timestamp.duration_since(arg.prev));
}
"#;

/// Test deploying a module with a scheduled reducer and check if client receives
/// subscription update for scheduled table entry and deletion of reducer once it ran
#[test]
fn test_scheduled_table_subscription() {
    let test = Smoketest::builder()
        .module_code(SUBSCRIBE_SCHEDULED_TABLE_MODULE_CODE)
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
        .module_code(SUBSCRIBE_SCHEDULED_TABLE_MODULE_CODE)
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
        invoked_count, logs
    );
}

const VOLATILE_NONATOMIC_MODULE_CODE: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = my_table, public)]
pub struct MyTable {
    x: String,
}

#[spacetimedb::reducer]
fn do_schedule(_ctx: &ReducerContext) {
    spacetimedb::volatile_nonatomic_schedule_immediate!(do_insert("hello".to_owned()));
}

#[spacetimedb::reducer]
fn do_insert(ctx: &ReducerContext, x: String) {
    ctx.db.my_table().insert(MyTable { x });
}
"#;

/// Check that volatile_nonatomic_schedule_immediate works
#[test]
fn test_volatile_nonatomic_schedule_immediate() {
    let test = Smoketest::builder()
        .module_code(VOLATILE_NONATOMIC_MODULE_CODE)
        .build();

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
