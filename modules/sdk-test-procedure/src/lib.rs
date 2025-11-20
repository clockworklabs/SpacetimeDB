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

// TODO(procedure-http): Add a procedure here which does an HTTP request against a SpacetimeDB route (as `http://localhost:3000/v1/`)
// and returns some value derived from the response.
// Then write a test which invokes it in the Rust client SDK test suite.

// TODO(procedure-http): Add a procedure here which does an HTTP request against an invalid SpacetimeDB route
// and returns some value derived from the error.
// Then write a test which invokes it in the Rust client SDK test suite.

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
