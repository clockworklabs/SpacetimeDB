use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table};
use std::time::Duration;

#[table(accessor = reminder, scheduled(send_reminder))]
pub struct Reminder {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
    message: String,
}

#[reducer]
pub fn send_reminder(_ctx: &ReducerContext, _row: Reminder) {}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    let fire_at = ctx.timestamp + Duration::from_secs(60);
    ctx.db.reminder().insert(Reminder {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(fire_at),
        message: "Hello!".to_string(),
    });
}
