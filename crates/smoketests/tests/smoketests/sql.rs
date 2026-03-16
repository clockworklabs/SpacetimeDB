use spacetimedb_smoketests::Smoketest;

/// This test is designed to test the format of the output of sql queries
#[test]
fn test_sql_format() {
    let test = Smoketest::builder().precompiled_module("sql-format").build();

    test.call("test", &[]).unwrap();

    test.assert_sql(
        "SELECT * FROM t_ints",
        r#" i_8 | i_16  | i_32   | i_64     | i_128         | i_256
-----+-------+--------+----------+---------------+---------------
 -25 | -3224 | -23443 | -2344353 | -234434897853 | -234434897853"#,
    );

    test.assert_sql(
        "SELECT * FROM t_ints_tuple",
        r#" tuple
---------------------------------------------------------------------------------------------------------
 (i_8 = -25, i_16 = -3224, i_32 = -23443, i_64 = -2344353, i_128 = -234434897853, i_256 = -234434897853)"#,
    );

    test.assert_sql(
        "SELECT * FROM t_uints",
        r#" u_8 | u_16 | u_32  | u_64     | u_128         | u_256
-----+------+-------+----------+---------------+---------------
 105 | 1050 | 83892 | 48937498 | 4378528978889 | 4378528978889"#,
    );

    test.assert_sql(
        "SELECT * FROM t_uints_tuple",
        r#" tuple
-------------------------------------------------------------------------------------------------------
 (u_8 = 105, u_16 = 1050, u_32 = 83892, u_64 = 48937498, u_128 = 4378528978889, u_256 = 4378528978889)"#,
    );

    test.assert_sql(
        "SELECT * FROM t_others",
        r#" bool | f_32      | f_64               | str                   | bytes            | identity                                                           | connection_id                      | timestamp                 | duration  | uuid
------+-----------+--------------------+-----------------------+------------------+--------------------------------------------------------------------+------------------------------------+---------------------------+-----------+----------------------------------------
 true | 594806.56 | -3454353.345389043 | "This is spacetimedb" | 0x01020304050607 | 0x0000000000000000000000000000000000000000000000000000000000000001 | 0x00000000000000000000000000000000 | 1970-01-01T00:00:00+00:00 | +0.000000 | "00000000-0000-0000-0000-000000000000""#,
    );

    test.assert_sql(
        "SELECT * FROM t_others_tuple",
        r#" tuple
------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
 (bool = true, f_32 = 594806.56, f_64 = -3454353.345389043, str = "This is spacetimedb", bytes = 0x01020304050607, identity = 0x0000000000000000000000000000000000000000000000000000000000000001, connection_id = 0x00000000000000000000000000000000, timestamp = 1970-01-01T00:00:00+00:00, duration = +0.000000, uuid = "00000000-0000-0000-0000-000000000000")"#,
    );

    test.assert_sql(
        "SELECT * FROM t_enums",
        r#" bool_opt      | bool_result  | action
---------------+--------------+---------------
 (some = true) | (ok = false) | (active = ())"#,
    );

    test.assert_sql(
        "SELECT * FROM t_enums_tuple",
        r#" tuple
--------------------------------------------------------------------------------
 (bool_opt = (some = true), bool_result = (ok = false), action = (active = ()))"#,
    );
}

#[test]
fn test_sql_resolves_accessor_and_canonical_names_for_table() {
    let test = Smoketest::builder().precompiled_module("sql-format").build();

    test.assert_sql(
        "SELECT * FROM accessor_table",
        r#" id | accessor_value_1
----+------------------
 1  | 7"#,
    );

    test.assert_sql(
        "SELECT * FROM canonical_table",
        r#" id | accessor_value_1
----+------------------
 1  | 7"#,
    );
}

#[test]
fn test_sql_resolves_accessor_and_canonical_names_for_view() {
    let test = Smoketest::builder().precompiled_module("sql-format").build();

    test.assert_sql(
        "SELECT * FROM accessor_filtered",
        r#" id | accessor_value_1
----+------------------
 1  | 7"#,
    );

    test.assert_sql(
        "SELECT * FROM canonical_filtered",
        r#" id | accessor_value_1
----+------------------
 1  | 7"#,
    );
}

#[test]
fn test_sql_resolves_accessor_and_canonical_names_for_column() {
    let test = Smoketest::builder().precompiled_module("sql-format").build();

    test.assert_sql(
        "SELECT accessor_value_1 FROM accessor_table",
        r#" accessor_value_1
------------------
 7"#,
    );

    test.assert_sql(
        "SELECT accessor_value1 FROM accessor_table",
        r#" accessor_value1
-----------------
 7"#,
    );
}

#[test]
fn test_query_builder_resolves_accessor_and_canonical_names() {
    let test = Smoketest::builder().precompiled_module("sql-format").build();

    test.assert_sql(
        "SELECT * FROM accessor_filtered",
        r#" id | accessor_value_1
----+------------------
 1  | 7"#,
    );
}
