//! Tests translated from smoketests/tests/views.py

use spacetimedb_smoketests::Smoketest;

const MODULE_CODE_VIEWS: &str = r#"
use spacetimedb::ViewContext;

#[derive(Copy, Clone)]
#[spacetimedb::table(name = player_state)]
pub struct PlayerState {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u64,
}

#[spacetimedb::view(name = player, public)]
pub fn player(ctx: &ViewContext) -> Option<PlayerState> {
    ctx.db.player_state().id().find(0u64)
}
"#;

/// Tests that views populate the st_view_* system tables
#[test]
fn test_st_view_tables() {
    let test = Smoketest::builder().module_code(MODULE_CODE_VIEWS).build();

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

const MODULE_CODE_BROKEN_NAMESPACE: &str = r#"
use spacetimedb::ViewContext;

#[spacetimedb::table(name = person, public)]
pub struct Person {
    name: String,
}

#[spacetimedb::view(name = person, public)]
pub fn person(ctx: &ViewContext) -> Option<Person> {
    None
}
"#;

const MODULE_CODE_BROKEN_RETURN_TYPE: &str = r#"
use spacetimedb::{SpacetimeType, ViewContext};

#[derive(SpacetimeType)]
pub enum ABC {
    A,
    B,
    C,
}

#[spacetimedb::view(name = person, public)]
pub fn person(ctx: &ViewContext) -> Option<ABC> {
    None
}
"#;

/// Publishing a module should fail if a table and view have the same name
#[test]
fn test_fail_publish_namespace_collision() {
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE_BROKEN_NAMESPACE)
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
        .module_code(MODULE_CODE_BROKEN_RETURN_TYPE)
        .autopublish(false)
        .build();

    let result = test.publish_module();
    assert!(
        result.is_err(),
        "Expected publish to fail when view return type is not a product type"
    );
}

const MODULE_CODE_SQL_VIEWS: &str = r#"
use spacetimedb::{AnonymousViewContext, ReducerContext, Table, ViewContext};

#[derive(Copy, Clone)]
#[spacetimedb::table(name = player_state)]
#[spacetimedb::table(name = player_level)]
pub struct PlayerState {
    #[primary_key]
    id: u64,
    #[index(btree)]
    level: u64,
}

#[derive(Clone)]
#[spacetimedb::table(name = player_info, index(name=age_level_index, btree(columns = [age, level])))]
pub struct PlayerInfo {
    #[primary_key]
    id: u64,
    age: u64,
    level: u64,
}

#[spacetimedb::reducer]
pub fn add_player_level(ctx: &ReducerContext, id: u64, level: u64) {
    ctx.db.player_level().insert(PlayerState { id, level });
}

#[spacetimedb::view(name = my_player_and_level, public)]
pub fn my_player_and_level(ctx: &AnonymousViewContext) -> Option<PlayerState> {
    ctx.db.player_level().id().find(0)
}

#[spacetimedb::view(name = player_and_level, public)]
pub fn player_and_level(ctx: &AnonymousViewContext) -> Vec<PlayerState> {
    ctx.db.player_level().level().filter(2u64).collect()
}

#[spacetimedb::view(name = player, public)]
pub fn player(ctx: &ViewContext) -> Option<PlayerState> {
    log::info!("player view called");
    ctx.db.player_state().id().find(42)
}

#[spacetimedb::view(name = player_none, public)]
pub fn player_none(_ctx: &ViewContext) -> Option<PlayerState> {
    None
}

#[spacetimedb::view(name = player_vec, public)]
pub fn player_vec(ctx: &ViewContext) -> Vec<PlayerState> {
    let first = ctx.db.player_state().id().find(42).unwrap();
    let second = PlayerState { id: 7, level: 3 };
    vec![first, second]
}

#[spacetimedb::view(name = player_info_multi_index, public)]
pub fn player_info_view(ctx: &ViewContext) -> Option<PlayerInfo> {
    log::info!("player_info called");
    ctx.db.player_info().age_level_index().filter((25u64, 7u64)).next()
}
"#;

/// Tests that views can be queried over HTTP SQL
#[test]
fn test_http_sql_views() {
    let test = Smoketest::builder()
        .module_code(MODULE_CODE_SQL_VIEWS)
        .build();

    // Insert initial data
    test.sql("INSERT INTO player_state (id, level) VALUES (42, 7)")
        .unwrap();

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
    let test = Smoketest::builder()
        .module_code(MODULE_CODE_SQL_VIEWS)
        .build();

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
