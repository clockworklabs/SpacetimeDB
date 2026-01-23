//! Tests translated from smoketests/tests/delete_database.py

use spacetimedb_smoketests::Smoketest;
use std::thread;
use std::time::Duration;

const MODULE_CODE: &str = r#"
use spacetimedb::{ReducerContext, Table, duration};

#[spacetimedb::table(name = counter, public)]
pub struct Counter {
    #[primary_key]
    id: u64,
    val: u64
}

#[spacetimedb::table(name = scheduled_counter, public, scheduled(inc, at = sched_at))]
pub struct ScheduledCounter {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    sched_at: spacetimedb::ScheduleAt,
}

#[spacetimedb::reducer]
pub fn inc(ctx: &ReducerContext, arg: ScheduledCounter) {
    if let Some(mut counter) = ctx.db.counter().id().find(arg.scheduled_id) {
        counter.val += 1;
        ctx.db.counter().id().update(counter);
    } else {
        ctx.db.counter().insert(Counter {
            id: arg.scheduled_id,
            val: 1,
        });
    }
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.scheduled_counter().insert(ScheduledCounter {
        scheduled_id: 0,
        sched_at: duration!(100ms).into(),
    });
}
"#;

/// Test that deleting a database stops the module.
/// The module is considered stopped if its scheduled reducer stops
/// producing update events.
#[test]
fn test_delete_database() {
    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    let name = format!("test-db-{}", std::process::id());
    test.publish_module_named(&name, false).unwrap();

    // Start subscription in background to collect updates
    // We request many updates but will stop early when we delete the db
    let sub = test.subscribe_background(&["SELECT * FROM counter"], 1000).unwrap();

    // Let the scheduled reducer run for a bit
    thread::sleep(Duration::from_secs(2));

    // Delete the database
    test.spacetime(&["delete", &name]).unwrap();

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
