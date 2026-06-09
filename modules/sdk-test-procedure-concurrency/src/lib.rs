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

#[derive(Copy, Clone, Debug)]
struct PollOptions {
    timeout: Duration,
    poll_interval: Duration,
}

impl Default for PollOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            poll_interval: Duration::from_millis(100),
        }
    }
}

fn poll_until_tx_true(ctx: &mut ProcedureContext, pred: impl Fn(&TxContext) -> bool, options: PollOptions) {
    let deadline = ctx.timestamp + options.timeout;
    log::info!("poll_until_tx_true: will give up at {deadline}");
    while ctx.timestamp < deadline {
        let try_again = ctx.timestamp + options.poll_interval;
        log::info!("poll_until_tx_true: sleeping until {try_again}");
        ctx.sleep_until(try_again);
        if ctx.with_tx(&pred) {
            log::info!("poll_until_tx_true: succeeded, returning now");
            return;
        }
        log::info!("poll_until_tx_true: false");
    }
    panic!("poll_until_tx_true: exceeded timeout {:?}", options.timeout)
}

#[procedure]
fn procedure_sleep_between_inserts(ctx: &mut ProcedureContext) {
    ctx.with_tx(|ctx| insert_procedure_concurrency_row(ctx, "procedure_before"));
    poll_until_tx_true(
        ctx,
        |tx| {
            tx.db
                .procedure_concurrency_row()
                .iter()
                .any(|row| row.insertion_context != "procedure_before")
        },
        Default::default(),
    );
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
    poll_until_tx_true(
        ctx,
        |tx| {
            tx.db
                .procedure_concurrency_row()
                .iter()
                .any(|row| row.insertion_context != "procedure_before")
        },
        Default::default(),
    );
    ctx.with_tx(|ctx| insert_procedure_concurrency_row(ctx, "procedure_after"));
}

#[table(accessor = scheduled_procedure_row, scheduled(scheduled_procedure_sleep_between_inserts))]
struct ScheduledProcedureRow {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}

#[procedure]
fn scheduled_procedure_sleep_between_inserts(ctx: &mut ProcedureContext, _schedule: ScheduledProcedureRow) {
    ctx.with_tx(|ctx| insert_procedure_concurrency_row(ctx, "scheduled_procedure_before"));
    // Unfortunately, we can't poll and wake on event here,
    // as (until https://github.com/clockworklabs/SpacetimeDB/pull/5224 is fixed)
    // the scheduled reducer actually won't run until after this procedure fully completes.
    ctx.sleep_until(ctx.timestamp + Duration::from_secs(10));
    ctx.with_tx(|ctx| insert_procedure_concurrency_row(ctx, "scheduled_procedure_after"));
}

#[reducer]
fn schedule_procedure_then_reducer(ctx: &ReducerContext) {
    ctx.db().scheduled_procedure_row().insert(ScheduledProcedureRow {
        scheduled_id: 0,
        scheduled_at: ctx.timestamp.into(),
    });
    ctx.db().scheduled_reducer_row().insert(ScheduledReducerRow {
        scheduled_id: 0,
        scheduled_at: (ctx.timestamp + Duration::from_secs(2)).into(),
    });
}
