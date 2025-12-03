use spacetimedb::{
    duration, procedure, reducer, table, DbContext, ProcedureContext, ReducerContext, ScheduleAt, SpacetimeType, Table,
    Timestamp, TxContext,
};

#[derive(SpacetimeType)]
struct ReturnStruct {
    a: u32,
    b: String,
}

#[derive(SpacetimeType)]
enum ReturnEnum {
    A(u32),
    B(String),
}

#[procedure]
fn return_primitive(_ctx: &mut ProcedureContext, lhs: u32, rhs: u32) -> u32 {
    lhs + rhs
}

#[procedure]
fn return_struct(_ctx: &mut ProcedureContext, a: u32, b: String) -> ReturnStruct {
    ReturnStruct { a, b }
}

#[procedure]
fn return_enum_a(_ctx: &mut ProcedureContext, a: u32) -> ReturnEnum {
    ReturnEnum::A(a)
}

#[procedure]
fn return_enum_b(_ctx: &mut ProcedureContext, b: String) -> ReturnEnum {
    ReturnEnum::B(b)
}

#[procedure]
fn will_panic(_ctx: &mut ProcedureContext) {
    panic!("This procedure is expected to panic")
}

#[procedure]
fn read_my_schema(ctx: &mut ProcedureContext) -> String {
    let module_identity = ctx.identity();
    match ctx.http.get(format!(
        "http://localhost:3000/v1/database/{module_identity}/schema?version=9"
    )) {
        Ok(result) => result.into_body().into_string_lossy(),
        Err(e) => panic!("{e}"),
    }
}

#[procedure]
fn invalid_request(ctx: &mut ProcedureContext) -> String {
    match ctx.http.get("http://foo.invalid/") {
        Ok(result) => panic!(
            "Got result from requesting `http://foo.invalid`... huh?\n{}",
            result.into_body().into_string_lossy()
        ),
        Err(e) => e.to_string(),
    }
}

#[table(public, name = my_table)]
struct MyTable {
    field: ReturnStruct,
}

fn insert_my_table(ctx: &TxContext) {
    ctx.db.my_table().insert(MyTable {
        field: ReturnStruct {
            a: 42,
            b: "magic".into(),
        },
    });
}

fn assert_row_count(ctx: &mut ProcedureContext, count: u64) {
    ctx.with_tx(|ctx| {
        assert_eq!(count, ctx.db.my_table().count());
    });
}

#[procedure]
fn insert_with_tx_commit(ctx: &mut ProcedureContext) {
    // Insert a row and commit.
    ctx.with_tx(insert_my_table);

    // Assert that there's a row.
    assert_row_count(ctx, 1);
}

#[procedure]
fn insert_with_tx_rollback(ctx: &mut ProcedureContext) {
    let _: Result<(), u32> = ctx.try_with_tx(|ctx| {
        insert_my_table(ctx);
        Err(24)
    });

    // Assert that there's not a row.
    assert_row_count(ctx, 0);
}

/// A reducer that schedules [`scheduled_proc`] via `ScheduledProcTable`.
#[reducer]
fn schedule_proc(ctx: &ReducerContext) {
    // Schedule the procedure to run in 1s.
    ctx.db().scheduled_proc_table().insert(ScheduledProcTable {
        scheduled_id: 0,
        scheduled_at: duration!("1000ms").into(),
        // Store the timestamp at which this reducer was called.
        // In tests, we'll compare this with the timestamp the procedure was called.
        reducer_ts: ctx.timestamp,
        x: 42,
        y: 24,
    });
}

#[table(name = scheduled_proc_table, scheduled(scheduled_proc))]
struct ScheduledProcTable {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
    reducer_ts: Timestamp,
    x: u8,
    y: u8,
}

/// A procedure that should be called 1s after `schedule_proc`.
#[procedure]
fn scheduled_proc(ctx: &mut ProcedureContext, data: ScheduledProcTable) {
    let ScheduledProcTable { reducer_ts, x, y, .. } = data;
    let procedure_ts = ctx.timestamp;
    ctx.with_tx(|ctx| {
        ctx.db().proc_inserts_into().insert(ProcInsertsInto {
            reducer_ts,
            procedure_ts,
            x,
            y,
        })
    });
}

#[table(name = proc_inserts_into, public)]
struct ProcInsertsInto {
    reducer_ts: Timestamp,
    procedure_ts: Timestamp,
    x: u8,
    y: u8,
}
