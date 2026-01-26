//! Tests translated from smoketests/tests/client_connected_error_rejects_connection.py

use spacetimedb_smoketests::Smoketest;

/// Test that client_connected returning an error rejects the connection
#[test]
fn test_client_connected_error_rejects_connection() {
    let test = Smoketest::builder()
        .precompiled_module("client-connection-reject")
        .build();

    // Subscribe should fail because client_connected returns an error
    let result = test.subscribe(&["SELECT * FROM all_u8s"], 0);
    assert!(
        result.is_err(),
        "Expected subscribe to fail when client_connected returns error"
    );

    let logs = test.logs(100).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("Rejecting connection from client")),
        "Expected rejection message in logs: {:?}",
        logs
    );
    assert!(
        !logs.iter().any(|l| l.contains("This should never be called")),
        "client_disconnected should not have been called: {:?}",
        logs
    );
}

/// Test that client_disconnected panicking still cleans up the st_client row
#[test]
fn test_client_disconnected_error_still_deletes_st_client() {
    let test = Smoketest::builder()
        .precompiled_module("client-connection-disconnect-panic")
        .build();

    // Subscribe should succeed (client_connected returns Ok)
    let result = test.subscribe(&["SELECT * FROM all_u8s"], 0);
    assert!(result.is_ok(), "Expected subscribe to succeed");

    let logs = test.logs(100).unwrap();
    assert!(
        logs.iter()
            .any(|l| { l.contains("This should be called, but the `st_client` row should still be deleted") }),
        "Expected disconnect panic message in logs: {:?}",
        logs
    );
}
