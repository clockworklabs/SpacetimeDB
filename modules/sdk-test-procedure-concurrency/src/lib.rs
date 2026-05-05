use spacetimedb::{procedure, reducer, table, DbContext, ProcedureContext, ReducerContext, Table, TxContext};
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
