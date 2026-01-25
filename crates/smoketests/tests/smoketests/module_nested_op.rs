//! Nested table operation tests translated from smoketests/tests/module_nested_op.py

use spacetimedb_smoketests::Smoketest;

/// This tests uploading a basic module and calling some functions and checking logs afterwards.
#[test]
fn test_module_nested_op() {
    let test = Smoketest::builder().precompiled_module("module-nested-op").build();

    test.call("create_account", &["1", r#""House""#]).unwrap();
    test.call("create_account", &["2", r#""Wilson""#]).unwrap();
    test.call("add_friend", &["1", "2"]).unwrap();
    test.call("say_friends", &[]).unwrap();

    let logs = test.logs(2).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("House is friends with Wilson")),
        "Expected 'House is friends with Wilson' in logs, got: {:?}",
        logs
    );
}
