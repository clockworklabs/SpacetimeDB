#![allow(clippy::disallowed_macros)]
use spacetimedb_smoketests::{require_local_server, require_psql, Smoketest};

#[test]
fn test_sql_format() {
    require_psql!();
    // This requires a local server because we don't have a clean way of providing
    // the remote server's PG port.
    require_local_server!();

    let mut test = Smoketest::builder()
        .precompiled_module("pg-wire")
        .pg_port(5433) // Use non-standard port to avoid conflicts
        .autopublish(false)
        .build();

    test.publish_module_named("quickstart", true).unwrap();
    test.call("test", &[]).unwrap();

    test.assert_psql(
        "quickstart",
        "SELECT * FROM t_ints",
        r#"i_8 | i_16  | i_32   |  i_64    |    i_128      |    i_256
-----+-------+--------+----------+---------------+---------------
 -25 | -3224 | -23443 | -2344353 | -234434897853 | -234434897853
(1 row)"#,
    );

    test.assert_psql(
        "quickstart",
        "SELECT * FROM t_ints_tuple",
        r#"tuple
-------------------------------------------------------------------------------------------------------------
 {"i_8": -25, "i_16": -3224, "i_32": -23443, "i_64": -2344353, "i_128": -234434897853, "i_256": -234434897853}
(1 row)"#,
    );

    test.assert_psql(
        "quickstart",
        "SELECT * FROM t_uints",
        r#"u_8 | u_16 | u_32  |  u_64    |    u_128      |    u_256
-----+------+-------+----------+---------------+---------------
 105 | 1050 | 83892 | 48937498 | 4378528978889 | 4378528978889
(1 row)"#,
    );

    test.assert_psql(
        "quickstart",
        "SELECT * FROM t_uints_tuple",
        r#"tuple
-----------------------------------------------------------------------------------------------------------
 {"u_8": 105, "u_16": 1050, "u_32": 83892, "u_64": 48937498, "u_128": 4378528978889, "u_256": 4378528978889}
(1 row)"#,
    );

    test.assert_psql(
        "quickstart",
        "SELECT * FROM t_simple_enum",
        r#"id |  action
----+----------
  1 | Inactive
  2 | Active
(2 rows)"#,
    );

    test.assert_psql(
        "quickstart",
        "SELECT * FROM t_enum",
        r#"id |     color
----+---------------
  1 | {"Gray": 128}
(1 row)"#,
    );
}

/// Test connecting to the database using a PostgreSQL client.
#[test]
fn test_sql_conn() {
    // This requires a local server because we don't have a clean way of providing
    // the remote server's PG port.
    require_local_server!();

    let mut test = Smoketest::builder()
        .precompiled_module("pg-wire")
        .pg_port(5435) // Use different port from test_sql_format/test_failures
        .autopublish(false)
        .build();

    test.publish_module_named("quickstart", true).unwrap();
    test.call("test", &[]).unwrap();

    let token = test.read_token().unwrap();
    let pg_port = test.pg_port().expect("PostgreSQL wire protocol not enabled");
    let host = test.server_host().split(':').next().unwrap_or("127.0.0.1");

    let mut cfg = tokio_postgres::Config::new();
    cfg.host(host);
    cfg.port(pg_port);
    cfg.user("postgres");
    cfg.password(token);
    cfg.dbname("quickstart");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async move {
        let (client, connection) = cfg.connect(tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(async move {
            let _ = connection.await;
        });

        let rows = client
            .simple_query("select * from t_uints where u8 = 105 and u16 = 1050")
            .await
            .unwrap();

        let row = rows
            .iter()
            .find_map(|m| match m {
                tokio_postgres::SimpleQueryMessage::Row(r) => Some(r),
                _ => None,
            })
            .expect("Expected at least one row");

        assert_eq!(row.get(0), Some("105"));
        assert_eq!(row.get(1), Some("1050"));
        assert_eq!(row.get(2), Some("83892"));
        assert_eq!(row.get(3), Some("48937498"));
        assert_eq!(row.get(4), Some("4378528978889"));
        assert_eq!(row.get(5), Some("4378528978889"));

        // Check long-lived connection.
        for _ in 0..10 {
            let rows = client.simple_query("select count(*) as t from t_uints").await.unwrap();

            let row = rows
                .iter()
                .find_map(|m| match m {
                    tokio_postgres::SimpleQueryMessage::Row(r) => Some(r),
                    _ => None,
                })
                .expect("Expected count row");

            assert_eq!(row.get(0), Some("1"));
        }
    });
}

/// Test failure cases
#[test]
fn test_failures() {
    require_psql!();
    // This requires a local server because we don't have a clean way of providing
    // the remote server's PG port.
    require_local_server!();

    let mut test = Smoketest::builder()
        .precompiled_module("pg-wire")
        .pg_port(5434) // Use different port from test_sql_format
        .autopublish(false)
        .build();

    test.publish_module_named("quickstart", true).unwrap();

    // Empty query returns empty result
    let output = test.psql("quickstart", "").unwrap();
    assert!(
        output.is_empty(),
        "Expected empty output for empty query, got: {}",
        output
    );

    let result = test.psql_with_token("quickstart", "invalid_token", "SELECT * FROM t_uints");
    assert!(result.is_err(), "Expected error for invalid token");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Invalid token"),
        "Expected 'Invalid token' in error message, got: {}",
        err
    );

    // Returns error for unsupported sql statements
    let result = test.psql(
        "quickstart",
        "SELECT CASE a WHEN 1 THEN 'one' ELSE 'other' END FROM t_uints",
    );
    assert!(result.is_err(), "Expected error for unsupported SQL");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Unsupported") || err.contains("unsupported"),
        "Expected 'Unsupported' in error message, got: {}",
        err
    );

    // And prepared statements
    let result = test.psql("quickstart", "SELECT * FROM t_uints where u8 = $1");
    assert!(result.is_err(), "Expected error for prepared statement");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Unsupported") || err.contains("unsupported"),
        "Expected 'Unsupported' in error message, got: {}",
        err
    );
}
