use spacetimedb::{procedure, reducer, table, ProcedureContext, ReducerContext, ScheduleAt, Table};
use std::time::Duration;

#[table(accessor = procedure_result, public)]
pub struct ProcedureResult {
    #[primary_key]
    pub id: u64,
    pub value: u32,
}

#[table(accessor = procedure_job, scheduled(run_scheduled_procedure))]
pub struct ProcedureJob {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
    pub id: u64,
    pub lhs: u32,
    pub rhs: u32,
}

#[reducer]
pub fn schedule_procedure(ctx: &ReducerContext, id: u64, lhs: u32, rhs: u32) {
    ctx.db.procedure_job().insert(ProcedureJob {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(ctx.timestamp + Duration::from_millis(1)),
        id,
        lhs,
        rhs,
    });
}

#[procedure]
pub fn run_scheduled_procedure(ctx: &mut ProcedureContext, job: ProcedureJob) {
    ctx.with_tx(|tx| {
        tx.db.procedure_result().insert(ProcedureResult {
            id: job.id,
            value: job.lhs + job.rhs,
        });
    });
}
