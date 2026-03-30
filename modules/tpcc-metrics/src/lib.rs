use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = state, public)]
pub struct State {
    #[primary_key]
    pub id: i64,

    pub run_start_ms: u64,
    pub run_end_ms: u64,
    pub measure_start_ms: u64,
    pub measure_end_ms: u64,

    pub order_count: u64,
    pub measurement_time_ms: u64,
}

#[reducer]
pub fn reset(ctx: &ReducerContext, warmup_duration_ms: u64, measure_start_ms: u64, measure_end_ms: u64) {
    for row in ctx.db.state().iter() {
        ctx.db.state().delete(row);
    }

    ctx.db.state().insert(State {
        id: 0,
        order_count: 0,
        measurement_time_ms: 0,
        run_start_ms: measure_start_ms - warmup_duration_ms,
        run_end_ms: measure_end_ms + warmup_duration_ms,
        measure_start_ms,
        measure_end_ms,
    });
}

#[reducer]
pub fn clear_state(ctx: &ReducerContext) {
    for row in ctx.db.state().iter() {
        ctx.db.state().delete(row);
    }
}

#[reducer]
pub fn register_completed_order(ctx: &ReducerContext) {
    // We intentionally do not check if the current time is within the measurement window,
    // this is the driver's reponsibility

    let current_time_ms = ctx.timestamp.to_duration_since_unix_epoch().unwrap().as_millis() as u64;

    let mut state = ctx.db.state().id().find(0).unwrap();

    state.order_count += 1;
    state.measurement_time_ms = current_time_ms;

    ctx.db.state().id().update(state);
}
