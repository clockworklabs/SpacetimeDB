//! Tests translated from smoketests/tests/connect_disconnect_from_cli.py

use spacetimedb_smoketests::Smoketest;

/// Ensure that the connect and disconnect functions are called when invoking a reducer from the CLI
#[test]
fn test_conn_disconn() {
    let test = Smoketest::builder().precompiled_module("connect-disconnect").build();

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
