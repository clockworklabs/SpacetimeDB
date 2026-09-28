use spacetimedb_smoketests::Smoketest;

/// Tests to check if a SpacetimeDB module can handle a panic without corrupting
#[test]
fn test_panic() {
    let test = Smoketest::builder().precompiled_module("panic").build();

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

/// Tests to ensure an error message returned from a reducer gets printed to logs
#[test]
fn test_reducer_error_message() {
    let test = Smoketest::builder().precompiled_module("panic-error").build();

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
