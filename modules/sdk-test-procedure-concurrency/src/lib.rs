use spacetimedb::{
    procedure, reducer, table, DbContext, ProcedureContext, ReducerContext, ScheduleAt, Table, TxContext,
};
use std::time::Duration;

#[table(public, accessor = procedure_concurrency_row)]
struct ProcedureConcurrencyRow {
    #[auto_inc]
    insertion_order: u32,
    insertion_context: String,
}

fn insert_procedure_concurrency_row(ctx: &TxContext, insertion_context: &str) {
    ctx.db.procedure_concurrency_row().insert(ProcedureConcurrencyRow {
        insertion_order: 0,
        insertion_context: insertion_context.into(),
    });
}

#[reducer]
fn insert_reducer_row(ctx: &ReducerContext) {
    ctx.db().procedure_concurrency_row().insert(ProcedureConcurrencyRow {
        insertion_order: 0,
        insertion_context: "reducer".into(),
    });
}

#[procedure]
fn procedure_sleep_between_inserts(ctx: &mut ProcedureContext) {
    ctx.with_tx(|ctx| insert_procedure_concurrency_row(ctx, "procedure_before"));
    ctx.sleep_until(ctx.timestamp + Duration::from_secs(10));
    ctx.with_tx(|ctx| insert_procedure_concurrency_row(ctx, "procedure_after"));
}

#[table(accessor = scheduled_reducer_row, scheduled(insert_scheduled_reducer))]
struct ScheduledReducerRow {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[reducer]
fn insert_scheduled_reducer(ctx: &ReducerContext, _schedule: ScheduledReducerRow) {
    ctx.db().procedure_concurrency_row().insert(ProcedureConcurrencyRow {
        insertion_order: 0,
        insertion_context: "scheduled_reducer".into(),
    });
}

#[procedure]
fn procedure_schedule_reducer_between_inserts(ctx: &mut ProcedureContext) {
    ctx.with_tx(|ctx| {
        insert_procedure_concurrency_row(ctx, "procedure_before");
        ctx.db.scheduled_reducer_row().insert(ScheduledReducerRow {
            scheduled_id: 0,
            scheduled_at: ctx.timestamp.into(),
        });
    });
    ctx.sleep_until(ctx.timestamp + Duration::from_secs(10));
    ctx.with_tx(|ctx| insert_procedure_concurrency_row(ctx, "procedure_after"));
}
