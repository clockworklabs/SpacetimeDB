use spacetimedb::{duration, log, ReducerContext, Table};

#[spacetimedb::reducer(init)]
fn init(ctx: &ReducerContext) {
    let schedule = ctx.db.scheduled_reducer_args().insert(ScheduledReducerArgs {
        num: 1,
        scheduled_id: 0,
        scheduled_at: duration!(100ms).into(),
    });
    ctx.db
        .scheduled_reducer_args()
        .scheduled_id()
        .delete(&schedule.scheduled_id);

    let schedule = ctx.db.scheduled_reducer_args().insert(ScheduledReducerArgs {
        num: 2,
        scheduled_id: 0,
        scheduled_at: duration!(1000ms).into(),
    });
    do_cancel(ctx, schedule.scheduled_id);
}

#[spacetimedb::table(accessor = scheduled_reducer_args, public, scheduled(reducer))]
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
