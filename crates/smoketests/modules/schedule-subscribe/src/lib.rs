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
