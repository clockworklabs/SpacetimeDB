use spacetimedb_smoketests::Smoketest;

/// Test that SQL requests are rejected when client_connected returns an error.
///
/// This verifies that the /sql HTTP endpoint now runs the module's
/// client_connected reducer and rejects the request if it errors.
/// Without PR #4563, this SQL query would succeed.
#[test]
fn test_sql_rejected_when_client_connected_errors() {
    let test = Smoketest::builder()
        .precompiled_module("client-connection-reject")
        .build();

    // SQL should fail because client_connected returns an error
    let result = test.sql("SELECT * FROM all_u8s");
    assert!(
        result.is_err(),
        "Expected SQL query to be rejected when client_connected errors, but it succeeded"
    );
}

/// Test that SQL requests trigger client_connected and client_disconnected hooks.
///
/// This verifies that the /sql HTTP endpoint calls the module's lifecycle
/// reducers. Without PR #4563, no connect/disconnect logs would appear.
#[test]
fn test_sql_triggers_connect_disconnect_hooks() {
    let test = Smoketest::builder().precompiled_module("sql-connect-hook").build();

    // Run a SQL query
    test.sql("SELECT * FROM person").unwrap();

    // Check that both connect and disconnect hooks were called
    let logs = test.logs(100).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("sql_connect_hook: client_connected")),
        "Expected client_connected log from SQL request, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("sql_connect_hook: client_disconnected")),
        "Expected client_disconnected log from SQL request, got: {:?}",
        logs
    );
}

/// Test that SQL queries still return data when client_connected accepts.
///
/// Ensures the connect hook doesn't break normal SQL functionality.
#[test]
fn test_sql_returns_data_with_connect_hook() {
    let test = Smoketest::builder().precompiled_module("sql-connect-hook").build();

    test.assert_sql(
        "SELECT * FROM person",
        r#" name
---------
 "Alice""#,
    );
}

/// Test that client_disconnected is still called even when the SQL query fails.
///
/// The `authorize_sql` and `exec_sql` errors are captured inside an async block,
/// so `call_identity_disconnected` runs regardless of query success or failure.
#[test]
fn test_sql_disconnect_called_on_query_error() {
    let test = Smoketest::builder().precompiled_module("sql-connect-hook").build();

    // Run an invalid SQL query — this will fail in exec_sql
    let result = test.sql("SELECT * FROM nonexistent_table");
    assert!(result.is_err(), "Expected invalid SQL to fail");

    // Despite the query error, both connect and disconnect should have been called
    let logs = test.logs(100).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("sql_connect_hook: client_connected")),
        "Expected client_connected even on failed SQL, got: {:?}",
        logs
    );
    assert!(
        logs.iter().any(|l| l.contains("sql_connect_hook: client_disconnected")),
        "Expected client_disconnected even on failed SQL, got: {:?}",
        logs
    );
}
