#![allow(clippy::disallowed_macros)]
use spacetimedb_smoketests::{have_psql, Smoketest};

/// Test SQL output formatting via psql
#[test]
fn test_sql_format() {
    if !have_psql() {
        eprintln!("Skipping test_sql_format: psql not available");
        return;
    }

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
        r#"i8  |  i16  |  i32   |   i64    |     i128      |     i256
-----+-------+--------+----------+---------------+---------------
 -25 | -3224 | -23443 | -2344353 | -234434897853 | -234434897853
(1 row)"#,
    );

    test.assert_psql(
        "quickstart",
        "SELECT * FROM t_ints_tuple",
        r#"tuple
---------------------------------------------------------------------------------------------------------
 {"i8": -25, "i16": -3224, "i32": -23443, "i64": -2344353, "i128": -234434897853, "i256": -234434897853}
(1 row)"#,
    );

    test.assert_psql(
        "quickstart",
        "SELECT * FROM t_uints",
        r#"u8  | u16  |  u32  |   u64    |     u128      |     u256
-----+------+-------+----------+---------------+---------------
 105 | 1050 | 83892 | 48937498 | 4378528978889 | 4378528978889
(1 row)"#,
    );

    test.assert_psql(
        "quickstart",
        "SELECT * FROM t_uints_tuple",
        r#"tuple
-------------------------------------------------------------------------------------------------------
 {"u8": 105, "u16": 1050, "u32": 83892, "u64": 48937498, "u128": 4378528978889, "u256": 4378528978889}
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

/// Test failure cases
#[test]
fn test_failures() {
    if !have_psql() {
        eprintln!("Skipping test_failures: psql not available");
        return;
    }

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

    // Connection fails with invalid token - we can't easily test this without
    // modifying the token, so skip this part

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
