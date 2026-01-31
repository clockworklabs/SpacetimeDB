use spacetimedb::{log, duration, ReducerContext, Table, Timestamp};

#[spacetimedb::table(name = scheduled_message, public, scheduled(my_repeating_reducer))]
pub struct ScheduledMessage {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: spacetimedb::ScheduleAt,
    prev: Timestamp,
}

#[spacetimedb::reducer(init)]
fn init(ctx: &ReducerContext) {
    ctx.db.scheduled_message().insert(ScheduledMessage {
        prev: ctx.timestamp,
        scheduled_id: 0,
        scheduled_at: duration!(100ms).into(),
    });
}

#[spacetimedb::reducer]
pub fn my_repeating_reducer(ctx: &ReducerContext, arg: ScheduledMessage) {
    log::info!("Invoked: ts={:?}, delta={:?}", ctx.timestamp, ctx.timestamp.duration_since(arg.prev));
}
