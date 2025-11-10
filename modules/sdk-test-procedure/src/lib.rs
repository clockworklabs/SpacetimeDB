use spacetimedb::{procedure, ProcedureContext, SpacetimeType};

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

// TODO(procedure-tx): Add a procedure here which acquires a transaction, inserts a row, commits, then returns.
// Then write a test which invokes it and asserts observing the row in the Rust client SDK test suite.

// TODO(procedure-tx): Add a procedure here which acquires a transaction, inserts a row, rolls back, then returns.
// Then write a test which invokes it and asserts not observing the row in the Rust client SDK test suite.
