//! Panic and error handling tests translated from smoketests/tests/panic.py

use spacetimedb_smoketests::Smoketest;

const PANIC_MODULE_CODE: &str = r#"
use spacetimedb::{log, ReducerContext};
use std::cell::RefCell;

thread_local! {
    static X: RefCell<u32> = RefCell::new(0);
}
#[spacetimedb::reducer]
fn first(_ctx: &ReducerContext) {
    X.with(|x| {
        let _x = x.borrow_mut();
        panic!()
    })
}
#[spacetimedb::reducer]
fn second(_ctx: &ReducerContext) {
    X.with(|x| *x.borrow_mut());
    log::info!("Test Passed");
}
"#;

/// Tests to check if a SpacetimeDB module can handle a panic without corrupting
#[test]
fn test_panic() {
    let test = Smoketest::builder()
        .module_code(PANIC_MODULE_CODE)
        .build();

    // First reducer should panic/fail
    let result = test.call("first", &[]);
    assert!(result.is_err(), "Expected first reducer to fail due to panic");

    // Second reducer should succeed, proving state wasn't corrupted
    test.call("second", &[]).unwrap();

    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("Test Passed")),
        "Expected 'Test Passed' in logs, got: {:?}",
        logs
    );
}

const REDUCER_ERROR_MODULE_CODE: &str = r#"
use spacetimedb::ReducerContext;

#[spacetimedb::reducer]
fn fail(_ctx: &ReducerContext) -> Result<(), String> {
    Err("oopsie :(".into())
}
"#;

/// Tests to ensure an error message returned from a reducer gets printed to logs
#[test]
fn test_reducer_error_message() {
    let test = Smoketest::builder()
        .module_code(REDUCER_ERROR_MODULE_CODE)
        .build();

    // Reducer should fail with error
    let result = test.call("fail", &[]);
    assert!(result.is_err(), "Expected fail reducer to return error");

    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("oopsie :(")),
        "Expected 'oopsie :(' in logs, got: {:?}",
        logs
    );
}
