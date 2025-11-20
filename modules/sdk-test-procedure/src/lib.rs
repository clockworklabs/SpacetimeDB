use spacetimedb::{procedure, table, ProcedureContext, SpacetimeType, Table, TxContext};

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
    match ctx.http.get(format!("http://foo.invalid/")) {
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
