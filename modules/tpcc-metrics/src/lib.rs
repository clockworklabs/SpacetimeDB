use spacetimedb::{reducer, table, ReducerContext, Table};

#[table(accessor = state, public)]
pub struct State {
    #[primary_key]
    pub id: i64,

    pub run_start_ms: u64,
    pub run_end_ms: u64,
    pub measure_start_ms: u64,
    pub measure_end_ms: u64,
    pub warehouse_count: u64,
}

#[table(accessor = txn, public)]
pub struct Txn {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub measurement_time_ms: u64,
    pub latency_ms: u16,
}

#[reducer]
pub fn reset(
    ctx: &ReducerContext,
    warehouse_count: u64,
    warmup_duration_ms: u64,
    measure_start_ms: u64,
    measure_end_ms: u64,
) {
    for row in ctx.db.state().iter() {
        ctx.db.state().id().delete(row.id);
    }

    for row in ctx.db.txn().iter() {
        ctx.db.txn().id().delete(row.id);
    }

    ctx.db.state().insert(State {
        id: 0,
        run_start_ms: measure_start_ms - warmup_duration_ms,
        run_end_ms: measure_end_ms + warmup_duration_ms,
        measure_start_ms,
        measure_end_ms,
        warehouse_count,
    });
}

#[reducer]
pub fn clear_state(ctx: &ReducerContext) {
    for row in ctx.db.state().iter() {
        ctx.db.state().id().delete(row.id);
    }

    for row in ctx.db.txn().iter() {
        ctx.db.txn().id().delete(row.id);
    }
}

#[reducer]
pub fn record_txn(ctx: &ReducerContext, latency_ms: u16) {
    let current_time_ms = ctx.timestamp.to_duration_since_unix_epoch().unwrap().as_millis() as u64;

    ctx.db.txn().insert(Txn {
        id: 0,
        measurement_time_ms: current_time_ms,
        latency_ms,
    });
}
