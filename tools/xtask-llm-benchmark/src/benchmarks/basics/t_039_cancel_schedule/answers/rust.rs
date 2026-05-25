use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table};
use std::time::Duration;

#[table(accessor = cleanup_job, scheduled(run_cleanup))]
pub struct CleanupJob {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[reducer]
pub fn run_cleanup(_ctx: &ReducerContext, _row: CleanupJob) {}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.cleanup_job().insert(CleanupJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(Duration::from_secs(60).into()),
    });
}

#[reducer]
pub fn cancel_cleanup(ctx: &ReducerContext, scheduled_id: u64) {
    ctx.db.cleanup_job().scheduled_id().delete(&scheduled_id);
}
