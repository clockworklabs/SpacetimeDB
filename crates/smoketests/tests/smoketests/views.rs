use spacetimedb_smoketests::Smoketest;

/// Tests that views populate the st_view_* system tables
#[test]
fn test_st_view_tables() {
    let test = Smoketest::builder().precompiled_module("views-basic").build();

    test.assert_sql(
        "SELECT * FROM st_view",
        r#" view_id | view_name | table_id      | is_public | is_anonymous
---------+-----------+---------------+-----------+--------------
 4096    | "player"  | (some = 4097) | true      | false"#,
    );

    test.assert_sql(
        "SELECT * FROM st_view_column",
        r#" view_id | col_pos | col_name | col_type
---------+---------+----------+----------
 4096    | 0       | "id"     | 0x0d
 4096    | 1       | "level"  | 0x0d"#,
    );
}

/// Publishing a module should fail if a table and view have the same name
#[test]
fn test_fail_publish_namespace_collision() {
    let mut test = Smoketest::builder()
        .use_precompiled_module("views-broken-namespace")
        .autopublish(false)
        .build();

    let result = test.publish_module();
    assert!(
        result.is_err(),
        "Expected publish to fail when table and view have same name"
    );
}

/// Publishing a module should fail if the inner return type is not a product type
#[test]
fn test_fail_publish_wrong_return_type() {
    let mut test = Smoketest::builder()
        .use_precompiled_module("views-broken-return-type")
        .autopublish(false)
        .build();

    let result = test.publish_module();
    assert!(
        result.is_err(),
        "Expected publish to fail when view return type is not a product type"
    );
}

/// Tests that views can be queried over HTTP SQL
#[test]
fn test_http_sql_views() {
    let test = Smoketest::builder().precompiled_module("views-sql").build();

    // Insert initial data
    test.sql("INSERT INTO player_state (id, level) VALUES (42, 7)").unwrap();

    test.assert_sql(
        "SELECT * FROM player",
        r#" id | level
----+-------
 42 | 7"#,
    );

    test.assert_sql(
        "SELECT * FROM player_none",
        r#" id | level
----+-------"#,
    );

    test.assert_sql(
        "SELECT * FROM player_vec",
        r#" id | level
----+-------
 42 | 7
 7  | 3"#,
    );
}

/// Tests that anonymous views are updated for reducers
#[test]
fn test_query_anonymous_view_reducer() {
    let test = Smoketest::builder().precompiled_module("views-sql").build();

    test.call("add_player_level", &["0", "1"]).unwrap();
    test.call("add_player_level", &["1", "2"]).unwrap();

    test.assert_sql(
        "SELECT * FROM my_player_and_level",
        r#" id | level
----+-------
 0  | 1"#,
    );

    test.assert_sql(
        "SELECT * FROM player_and_level",
        r#" id | level
----+-------
 1  | 2"#,
    );

    test.call("add_player_level", &["2", "2"]).unwrap();

    test.assert_sql(
        "SELECT * FROM player_and_level",
        r#" id | level
----+-------
 1  | 2
 2  | 2"#,
    );

    test.assert_sql(
        "SELECT * FROM player_and_level WHERE id = 2",
        r#" id | level
----+-------
 2  | 2"#,
    );
}
