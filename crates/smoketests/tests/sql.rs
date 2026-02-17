use spacetimedb_smoketests::Smoketest;

/// This test is designed to test the format of the output of sql queries
#[test]
fn test_sql_format() {
    let test = Smoketest::builder().precompiled_module("sql-format").build();

    test.call("test", &[]).unwrap();

    test.assert_sql(
        "SELECT * FROM t_ints",
        r#" i8  | i16   | i32    | i64      | i128          | i256
-----+-------+--------+----------+---------------+---------------
 -25 | -3224 | -23443 | -2344353 | -234434897853 | -234434897853"#,
    );

    test.assert_sql(
        "SELECT * FROM t_ints_tuple",
        r#" tuple
---------------------------------------------------------------------------------------------------
 (i8 = -25, i16 = -3224, i32 = -23443, i64 = -2344353, i128 = -234434897853, i256 = -234434897853)"#,
    );

    test.assert_sql(
        "SELECT * FROM t_uints",
        r#" u8  | u16  | u32   | u64      | u128          | u256
-----+------+-------+----------+---------------+---------------
 105 | 1050 | 83892 | 48937498 | 4378528978889 | 4378528978889"#,
    );

    test.assert_sql(
        "SELECT * FROM t_uints_tuple",
        r#" tuple
-------------------------------------------------------------------------------------------------
 (u8 = 105, u16 = 1050, u32 = 83892, u64 = 48937498, u128 = 4378528978889, u256 = 4378528978889)"#,
    );

    test.assert_sql(
        "SELECT * FROM t_others",
        r#" bool | f32       | f64                | str                   | bytes            | identity                                                           | connection_id                      | timestamp                 | duration  | uuid
------+-----------+--------------------+-----------------------+------------------+--------------------------------------------------------------------+------------------------------------+---------------------------+-----------+----------------------------------------
 true | 594806.56 | -3454353.345389043 | "This is spacetimedb" | 0x01020304050607 | 0x0000000000000000000000000000000000000000000000000000000000000001 | 0x00000000000000000000000000000000 | 1970-01-01T00:00:00+00:00 | +0.000000 | "00000000-0000-0000-0000-000000000000""#,
    );

    test.assert_sql(
        "SELECT * FROM t_others_tuple",
        r#" tuple
----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
 (bool = true, f32 = 594806.56, f64 = -3454353.345389043, str = "This is spacetimedb", bytes = 0x01020304050607, identity = 0x0000000000000000000000000000000000000000000000000000000000000001, connection_id = 0x00000000000000000000000000000000, timestamp = 1970-01-01T00:00:00+00:00, duration = +0.000000, uuid = "00000000-0000-0000-0000-000000000000")"#,
    );

    test.assert_sql(
        "SELECT * FROM t_enums",
        r#" bool_opt      | bool_result  | action
---------------+--------------+---------------
 (some = true) | (ok = false) | (Active = ())"#,
    );

    test.assert_sql(
        "SELECT * FROM t_enums_tuple",
        r#" tuple
--------------------------------------------------------------------------------
 (bool_opt = (some = true), bool_result = (ok = false), action = (Active = ()))"#,
    );
}

#[test]
fn test_sql_resolves_accessor_and_canonical_names() {
    let test = Smoketest::builder().precompiled_module("sql-format").build();

    test.assert_sql(
        "SELECT * FROM accessor_table",
        r#" id | accessor_value
----+----------------
 1  | 7"#,
    );

    test.assert_sql(
        "SELECT * FROM canonical_table",
        r#" id | accessor_value
----+----------------
 1  | 7"#,
    );
}

#[test]
fn test_sql_resolves_column_accessor_and_canonical_names() {
    let test = Smoketest::builder().precompiled_module("sql-format").build();

    test.assert_sql(
        "SELECT accessor_value FROM accessor_table",
        r#" accessor_value
----------------
 7"#,
    );

    test.assert_sql(
        "SELECT canonical_value FROM accessor_table",
        r#" canonical_value
----------------
 7"#,
    );
}
