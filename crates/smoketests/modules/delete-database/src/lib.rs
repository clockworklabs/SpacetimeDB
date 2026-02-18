use spacetimedb::{ReducerContext, Table, duration};

#[spacetimedb::table(accessor = counter, public)]
pub struct Counter {
    #[primary_key]
    id: u64,
    val: u64
}

#[spacetimedb::table(accessor = scheduled_counter, public, scheduled(inc, at = sched_at))]
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
