use spacetimedb::*;

struct Test;

#[spacetimedb(reducer)]
fn bad_type(_a: Test) {}

#[spacetimedb(reducer)]
fn bad_return_type() -> Test {
    Test
}

#[spacetimedb(reducer)]
fn bad_type_ctx(_ctx: ReducerContext, _a: Test) {}

#[spacetimedb(reducer)]
fn bad_return_type_ctx(_ctx: ReducerContext) -> Test {
    Test
}

#[spacetimedb(reducer)]
fn lifetime<'a>(_a: &'a str) {}

#[spacetimedb(reducer)]
fn type_param<T>() {}

#[spacetimedb(reducer)]
fn const_param<const X: u8>() {}

fn main() {}
