use spacetimedb::{reducer, table, ReducerContext, ScheduleAt, Table, Timestamp};
use std::time::Duration;

#[table(accessor = materialized_state, public)]
pub struct MaterializedState {
    #[primary_key]
    pub id: u64,
    pub status: String,
    pub version: u64,
    pub refreshed_at: Timestamp,
}

#[table(accessor = refresh_job, scheduled(refresh_materialized))]
pub struct RefreshJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub state_id: u64,
}

#[reducer]
pub fn start_refresh(ctx: &ReducerContext) {
    let pending = MaterializedState {
        id: 1,
        status: "pending".into(),
        version: 0,
        refreshed_at: Timestamp::UNIX_EPOCH,
    };
    if ctx.db.materialized_state().id().find(1).is_some() {
        ctx.db.materialized_state().id().update(pending);
    } else {
        ctx.db.materialized_state().insert(pending);
    }
    ctx.db.refresh_job().insert(RefreshJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ctx.timestamp + Duration::from_millis(1)),
        state_id: 1,
    });
}

#[reducer]
pub fn refresh_materialized(ctx: &ReducerContext, job: RefreshJob) {
    let mut state = ctx
        .db
        .materialized_state()
        .id()
        .find(job.state_id)
        .expect("materialized state missing");
    state.status = "ready".into();
    state.version = 1;
    state.refreshed_at = ctx.timestamp;
    ctx.db.materialized_state().id().update(state);
}
