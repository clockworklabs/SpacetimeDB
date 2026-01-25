//! Auto-increment tests translated from smoketests/tests/auto_inc.py
//!
//! This is a simplified version that tests representative integer types
//! rather than all 10 types in the Python version.

use spacetimedb_smoketests::Smoketest;

#[test]
fn test_autoinc_u32() {
    let test = Smoketest::builder()
        .precompiled_module("autoinc-basic-u32")
        .build();

    test.call("add_u32", &[r#""Robert""#, "1"]).unwrap();
    test.call("add_u32", &[r#""Julie""#, "2"]).unwrap();
    test.call("add_u32", &[r#""Samantha""#, "3"]).unwrap();
    test.call("say_hello_u32", &[]).unwrap();

    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 3:Samantha!")),
        "Expected 'Hello, 3:Samantha!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 2:Julie!")),
        "Expected 'Hello, 2:Julie!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 1:Robert!")),
        "Expected 'Hello, 1:Robert!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, World!")),
        "Expected 'Hello, World!' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_autoinc_u64() {
    let test = Smoketest::builder()
        .precompiled_module("autoinc-basic-u64")
        .build();

    test.call("add_u64", &[r#""Robert""#, "1"]).unwrap();
    test.call("add_u64", &[r#""Julie""#, "2"]).unwrap();
    test.call("add_u64", &[r#""Samantha""#, "3"]).unwrap();
    test.call("say_hello_u64", &[]).unwrap();

    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 3:Samantha!")),
        "Expected 'Hello, 3:Samantha!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 2:Julie!")),
        "Expected 'Hello, 2:Julie!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 1:Robert!")),
        "Expected 'Hello, 1:Robert!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, World!")),
        "Expected 'Hello, World!' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_autoinc_i32() {
    let test = Smoketest::builder()
        .precompiled_module("autoinc-basic-i32")
        .build();

    test.call("add_i32", &[r#""Robert""#, "1"]).unwrap();
    test.call("add_i32", &[r#""Julie""#, "2"]).unwrap();
    test.call("add_i32", &[r#""Samantha""#, "3"]).unwrap();
    test.call("say_hello_i32", &[]).unwrap();

    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 3:Samantha!")),
        "Expected 'Hello, 3:Samantha!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 2:Julie!")),
        "Expected 'Hello, 2:Julie!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 1:Robert!")),
        "Expected 'Hello, 1:Robert!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, World!")),
        "Expected 'Hello, World!' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_autoinc_i64() {
    let test = Smoketest::builder()
        .precompiled_module("autoinc-basic-i64")
        .build();

    test.call("add_i64", &[r#""Robert""#, "1"]).unwrap();
    test.call("add_i64", &[r#""Julie""#, "2"]).unwrap();
    test.call("add_i64", &[r#""Samantha""#, "3"]).unwrap();
    test.call("say_hello_i64", &[]).unwrap();

    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 3:Samantha!")),
        "Expected 'Hello, 3:Samantha!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 2:Julie!")),
        "Expected 'Hello, 2:Julie!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 1:Robert!")),
        "Expected 'Hello, 1:Robert!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, World!")),
        "Expected 'Hello, World!' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_autoinc_unique_u64() {
    let test = Smoketest::builder()
        .precompiled_module("autoinc-unique-u64")
        .build();

    // Insert Robert with explicit id 2
    test.call("update_u64", &[r#""Robert""#, "2"]).unwrap();

    // Auto-inc should assign id 1 to Success
    test.call("add_new_u64", &[r#""Success""#]).unwrap();

    // Auto-inc tries to assign id 2, but Robert already has it - should fail
    let result = test.call("add_new_u64", &[r#""Failure""#]);
    assert!(
        result.is_err(),
        "Expected add_new to fail due to unique constraint violation"
    );

    test.call("say_hello_u64", &[]).unwrap();

    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 2:Robert!")),
        "Expected 'Hello, 2:Robert!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 1:Success!")),
        "Expected 'Hello, 1:Success!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, World!")),
        "Expected 'Hello, World!' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_autoinc_unique_i64() {
    let test = Smoketest::builder()
        .precompiled_module("autoinc-unique-i64")
        .build();

    // Insert Robert with explicit id 2
    test.call("update_i64", &[r#""Robert""#, "2"]).unwrap();

    // Auto-inc should assign id 1 to Success
    test.call("add_new_i64", &[r#""Success""#]).unwrap();

    // Auto-inc tries to assign id 2, but Robert already has it - should fail
    let result = test.call("add_new_i64", &[r#""Failure""#]);
    assert!(
        result.is_err(),
        "Expected add_new to fail due to unique constraint violation"
    );

    test.call("say_hello_i64", &[]).unwrap();

    let logs = test.logs(4).unwrap();
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 2:Robert!")),
        "Expected 'Hello, 2:Robert!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, 1:Success!")),
        "Expected 'Hello, 1:Success!' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|msg| msg.contains("Hello, World!")),
        "Expected 'Hello, World!' in logs, got: {:?}",
        logs
    );
}
