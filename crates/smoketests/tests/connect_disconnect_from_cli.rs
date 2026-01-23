//! Tests translated from smoketests/tests/connect_disconnect_from_cli.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE: &str = r#"
use spacetimedb::{log, ReducerContext};

#[spacetimedb::reducer(client_connected)]
pub fn connected(_ctx: &ReducerContext) {
    log::info!("_connect called");
}

#[spacetimedb::reducer(client_disconnected)]
pub fn disconnected(_ctx: &ReducerContext) {
    log::info!("disconnect called");
}

#[spacetimedb::reducer]
pub fn say_hello(_ctx: &ReducerContext) {
    log::info!("Hello, World!");
}
"#;

/// Ensure that the connect and disconnect functions are called when invoking a reducer from the CLI
#[test]
fn test_conn_disconn() {
    let test = Smoketest::builder().module_code(MODULE_CODE).build();

    test.call("say_hello", &[]).unwrap();

    let logs = test.logs(10).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("_connect called")),
        "Expected '_connect called' in logs: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("disconnect called")),
        "Expected 'disconnect called' in logs: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("Hello, World!")),
        "Expected 'Hello, World!' in logs: {:?}",
        logs
    );
}
