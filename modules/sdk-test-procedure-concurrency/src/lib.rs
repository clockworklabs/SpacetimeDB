use spacetimedb::{procedure, reducer, table, ProcedureContext, ReducerContext, ScheduleAt, Table, TxContext};
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
    ctx.db.procedure_concurrency_row().insert(ProcedureConcurrencyRow {
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
    insertion_context: String,
}

#[reducer]
fn insert_scheduled_reducer(ctx: &ReducerContext, schedule: ScheduledReducerRow) {
    ctx.db.procedure_concurrency_row().insert(ProcedureConcurrencyRow {
        insertion_order: 0,
        insertion_context: schedule.insertion_context.clone(),
    });
    if schedule.insertion_context == "scheduled_reducer_update_first" {
        ctx.db
            .scheduled_reducer_row()
            .scheduled_id()
            .update(ScheduledReducerRow {
                scheduled_at: (ctx.timestamp + Duration::from_secs(1)).into(),
                insertion_context: "scheduled_reducer_update_second".into(),
                ..schedule
            });
    } else if schedule.insertion_context == "scheduled_interval_reducer_update_first" {
        ctx.db
            .scheduled_reducer_row()
            .scheduled_id()
            .update(ScheduledReducerRow {
                scheduled_at: Duration::from_secs(1).into(),
                insertion_context: "scheduled_interval_reducer_update_second".into(),
                ..schedule
            });
    } else if schedule.insertion_context == "scheduled_interval_reducer_update_second" {
        ctx.db
            .scheduled_reducer_row()
            .scheduled_id()
            .delete(schedule.scheduled_id);
    }
}

#[procedure]
fn procedure_schedule_reducer_between_inserts(ctx: &mut ProcedureContext) {
    ctx.with_tx(|ctx| {
        insert_procedure_concurrency_row(ctx, "procedure_before");
        ctx.db.scheduled_reducer_row().insert(ScheduledReducerRow {
            scheduled_id: 0,
            scheduled_at: ctx.timestamp.into(),
            insertion_context: "scheduled_reducer".into(),
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
    run: u8,
}

#[procedure]
fn scheduled_procedure_sleep_between_inserts(ctx: &mut ProcedureContext, schedule: ScheduledProcedureRow) {
    let (before, after, sleep) = match schedule.run {
        1 => (
            "scheduled_procedure_update_first_before",
            "scheduled_procedure_update_first_after",
            2,
        ),
        2 => (
            "scheduled_procedure_update_second_before",
            "scheduled_procedure_update_second_after",
            0,
        ),
        _ => ("scheduled_procedure_before", "scheduled_procedure_after", 10),
    };

    ctx.with_tx(|ctx| insert_procedure_concurrency_row(ctx, before));
    if schedule.run == 1 {
        ctx.with_tx(|ctx| {
            ctx.db
                .scheduled_procedure_row()
                .scheduled_id()
                .update(ScheduledProcedureRow {
                    scheduled_at: (ctx.timestamp + Duration::from_secs(1)).into(),
                    run: 2,
                    ..schedule
                });
        });
    }
    // Sleep long enough for the later scheduled reducer to run while this
    // procedure is still suspended.
    ctx.sleep_until(ctx.timestamp + Duration::from_secs(sleep));
    ctx.with_tx(|ctx| insert_procedure_concurrency_row(ctx, after));
}

#[reducer]
fn schedule_procedure_then_reducer(ctx: &ReducerContext) {
    ctx.db.scheduled_procedure_row().insert(ScheduledProcedureRow {
        scheduled_id: 0,
        scheduled_at: ctx.timestamp.into(),
        run: 0,
    });
    ctx.db.scheduled_reducer_row().insert(ScheduledReducerRow {
        scheduled_id: 0,
        scheduled_at: (ctx.timestamp + Duration::from_secs(2)).into(),
        insertion_context: "scheduled_reducer_1".into(),
    });
    ctx.db.scheduled_reducer_row().insert(ScheduledReducerRow {
        scheduled_id: 0,
        scheduled_at: (ctx.timestamp + Duration::from_secs(3)).into(),
        insertion_context: "scheduled_reducer_2".into(),
    });
}

#[reducer]
fn schedule_procedure_update_while_inflight(ctx: &ReducerContext) {
    ctx.db.scheduled_procedure_row().insert(ScheduledProcedureRow {
        scheduled_id: 0,
        scheduled_at: Duration::from_secs(1).into(),
        run: 1,
    });
    ctx.db.scheduled_reducer_row().insert(ScheduledReducerRow {
        scheduled_id: 0,
        scheduled_at: (ctx.timestamp + Duration::from_secs(4)).into(),
        insertion_context: "scheduled_procedure_update_verifier".into(),
    });
}

#[reducer]
fn schedule_oneshot_reducer_update_while_inflight(ctx: &ReducerContext) {
    ctx.db.scheduled_reducer_row().insert(ScheduledReducerRow {
        scheduled_id: 0,
        scheduled_at: ctx.timestamp.into(),
        insertion_context: "scheduled_reducer_update_first".into(),
    });
    ctx.db.scheduled_reducer_row().insert(ScheduledReducerRow {
        scheduled_id: 0,
        scheduled_at: Duration::from_secs(1).into(),
        insertion_context: "scheduled_interval_reducer_update_first".into(),
    });
    ctx.db.scheduled_reducer_row().insert(ScheduledReducerRow {
        scheduled_id: 0,
        scheduled_at: (ctx.timestamp + Duration::from_secs(4)).into(),
        insertion_context: "scheduled_reducer_update_verifier".into(),
    });
}
