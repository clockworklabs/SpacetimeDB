use serde_json::json;
use spacetimedb_smoketests::{require_dotnet, require_pnpm, Smoketest};

const TS_VIEWS_SUBSCRIBE_MODULE: &str = r#"import { schema, t, table } from "spacetimedb/server";

const playerState = table(
  { name: "player_state" },
  {
    identity: t.identity().primaryKey(),
    name: t.string().unique(),
  }
);

const spacetimedb = schema({ playerState });
export default spacetimedb;

export const my_player = spacetimedb.view(
  { public: true },
  t.option(playerState.rowType),
  ctx => ctx.db.playerState.identity.find(ctx.sender) ?? undefined
);

export const all_players = spacetimedb.anonymousView(
  { public: true },
  t.array(playerState.rowType),
  ctx => ctx.from.playerState
);

export const insert_player_proc = spacetimedb.procedure(
  { name: t.string() },
  t.unit(),
  (ctx, { name }) => {
    const sender = ctx.sender;
    ctx.withTx(tx => {
      tx.db.playerState.insert({ name, identity: sender });
    });
    return {};
  }
);
"#;

const CS_VIEWS_QUERY_BUILDER_MODULE: &str = r#"using SpacetimeDB;

public static partial class Module
{
    [Table(Accessor = "Table", Public = true)]
    public partial struct Table
    {
        public uint Value;
    }

    [Reducer]
    public static void InsertValue(ReducerContext ctx, uint value)
    {
        ctx.Db.Table.Insert(new Table { Value = value });
    }

    [View(Accessor = "all", Public = true)]
    public static IQuery<Table> All(ViewContext ctx)
    {
        return ctx.From.Table();
    }

    [View(Accessor = "some", Public = true)]
    public static IQuery<Table> Some(ViewContext ctx)
    {
        return ctx.From.Table().Where(Row => Row.Value.Eq(1));
    }
}
"#;

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
        // Can't be precompiled because the code is intentionally broken
        .module_code(include_str!("../../modules/views-broken-namespace/src/lib.rs"))
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
        // Can't be precompiled because the code is intentionally broken
        .module_code(include_str!("../../modules/views-broken-return-type/src/lib.rs"))
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

#[test]
fn test_view_materialization() {
    let test = Smoketest::builder().precompiled_module("views-sql").build();

    let player_called_log = "player view called";

    test.assert_sql(
        "SELECT * FROM player",
        r#" id | level
----+-------"#,
    );
    let logs = test.logs(100).unwrap();
    let count = logs.iter().filter(|l| l.contains(player_called_log)).count();
    assert_eq!(count, 1, "Unexpected logs: {logs:?}");

    test.sql("INSERT INTO player_state (id, level) VALUES (42, 7);")
        .unwrap();

    test.assert_sql(
        "SELECT * FROM player",
        r#" id | level
----+-------
 42 | 7"#,
    );
    let logs = test.logs(100).unwrap();
    let count = logs.iter().filter(|l| l.contains(player_called_log)).count();
    assert_eq!(count, 2, "Unexpected logs: {logs:?}");

    test.assert_sql(
        "SELECT * FROM player",
        r#" id | level
----+-------
 42 | 7"#,
    );
    let logs = test.logs(100).unwrap();
    let count = logs.iter().filter(|l| l.contains(player_called_log)).count();
    assert_eq!(count, 2, "Unexpected logs: {logs:?}");

    test.sql("INSERT INTO player_state (id, level) VALUES (22, 8);")
        .unwrap();

    test.assert_sql(
        "SELECT * FROM player",
        r#" id | level
----+-------
 42 | 7"#,
    );
    let logs = test.logs(100).unwrap();
    let count = logs.iter().filter(|l| l.contains(player_called_log)).count();
    assert_eq!(count, 2, "Unexpected logs: {logs:?}");

    test.sql("UPDATE player_state SET level = 9 WHERE id = 42;").unwrap();
    let logs = test.logs(100).unwrap();
    let count = logs.iter().filter(|l| l.contains(player_called_log)).count();
    assert_eq!(count, 3, "Unexpected logs: {logs:?}");

    test.sql("UPDATE player_state SET level = 7 WHERE id = 42;").unwrap();
}

#[test]
fn test_view_multi_index_materialization() {
    let test = Smoketest::builder().precompiled_module("views-sql").build();

    let player_called_log = "player_info called";

    test.assert_sql(
        "SELECT * FROM player_info_multi_index",
        r#" id | age | level
----+-----+-------"#,
    );
    let logs = test.logs(100).unwrap();
    let count = logs.iter().filter(|l| l.contains(player_called_log)).count();
    assert_eq!(count, 1, "Unexpected logs: {logs:?}");

    test.sql("INSERT INTO player_info (id, age, level) VALUES (1, 25, 7);")
        .unwrap();
    test.assert_sql(
        "SELECT * FROM player_info_multi_index",
        r#" id | age | level
----+-----+-------
 1  | 25  | 7"#,
    );
    let logs = test.logs(100).unwrap();
    let count = logs.iter().filter(|l| l.contains(player_called_log)).count();
    assert_eq!(count, 2, "Unexpected logs: {logs:?}");

    test.sql("INSERT INTO player_info (id, age, level) VALUES (2, 25, 8);")
        .unwrap();
    let logs = test.logs(100).unwrap();
    let count = logs.iter().filter(|l| l.contains(player_called_log)).count();
    assert_eq!(count, 2, "Unexpected logs: {logs:?}");

    test.sql("UPDATE player_info SET age = 26 WHERE id = 1;").unwrap();
    let logs = test.logs(100).unwrap();
    let count = logs.iter().filter(|l| l.contains(player_called_log)).count();
    assert_eq!(count, 3, "Unexpected logs: {logs:?}");

    test.assert_sql(
        "SELECT * FROM player_info_multi_index",
        r#" id | age | level
----+-----+-------"#,
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

#[test]
fn test_views_auto_migration() {
    let mut test = Smoketest::builder().precompiled_module("views-auto-migrate").build();

    test.sql("INSERT INTO player_state (id, level) VALUES (1, 1);").unwrap();
    test.sql("INSERT INTO player_state (id, level) VALUES (2, 2);").unwrap();

    test.assert_sql(
        "SELECT * FROM player",
        r#" id | level
----+-------
 1  | 1"#,
    );

    test.use_precompiled_module("views-auto-migrate-updated");
    test.publish_module_clear(false).unwrap();

    test.assert_sql(
        "SELECT * FROM player",
        r#" id | level
----+-------
 2  | 2"#,
    );
}

#[test]
fn test_auto_migration_drop_view() {
    let mut test = Smoketest::builder().precompiled_module("views-auto-migrate").build();
    test.use_precompiled_module("views-drop-view");
    test.publish_module_clear(false).unwrap();
}

#[test]
fn test_auto_migration_add_view() {
    let mut test = Smoketest::builder().precompiled_module("views-drop-view").build();
    test.use_precompiled_module("views-auto-migrate");
    test.publish_module_clear(false).unwrap();
}

#[test]
fn test_view_accessibility() {
    let test = Smoketest::builder().precompiled_module("views-callable").build();

    test.new_identity().unwrap();
    test.call("baz", &[]).unwrap();

    test.assert_sql(
        "SELECT * FROM items",
        r#" value
-------
 7"#,
    );
}

#[test]
fn test_recovery_from_trapped_views_auto_migration() {
    let mut test = Smoketest::builder().precompiled_module("views-auto-migrate").build();

    test.sql("INSERT INTO player_state (id, level) VALUES (1, 1);").unwrap();

    test.assert_sql(
        "SELECT * FROM player",
        r#" id | level
----+-------
 1  | 1"#,
    );

    test.use_precompiled_module("views-trapped");
    let result = test.publish_module_clear(false);
    assert!(result.is_err(), "Expected trapped publish to fail");

    test.assert_sql(
        "SELECT * FROM player",
        r#" id | level
----+-------
 1  | 1"#,
    );

    test.use_precompiled_module("views-recovered");
    test.publish_module_clear(false).unwrap();

    test.assert_sql(
        "SELECT * FROM player",
        r#" id | level
----+-------"#,
    );
}

#[test]
fn test_subscribing_with_different_identities() {
    let test = Smoketest::builder().precompiled_module("views-subscribe").build();

    test.call("insert_player", &["Alice"]).unwrap();

    test.new_identity().unwrap();

    let sub = test.subscribe_background(&["select * from my_player"], 2).unwrap();
    test.call("insert_player", &["Bob"]).unwrap();
    let events = sub.collect().unwrap();

    let projection: Vec<serde_json::Value> = events
        .into_iter()
        .map(|event| {
            let deletes = event["my_player"]["deletes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| json!({"name": row["name"]}))
                .collect::<Vec<_>>();
            let inserts = event["my_player"]["inserts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| json!({"name": row["name"]}))
                .collect::<Vec<_>>();
            json!({"my_player": {"deletes": deletes, "inserts": inserts}})
        })
        .collect();

    assert_eq!(
        serde_json::json!(projection),
        serde_json::json!([
            {"my_player": {"deletes": [], "inserts": [{"name": "Bob"}]}}
        ])
    );
}

#[test]
fn test_query_view() {
    let test = Smoketest::builder().precompiled_module("views-query").build();
    test.assert_sql(
        "SELECT * FROM online_users",
        r#" identity | name    | online
----------+---------+--------
 1        | "Alice" | true"#,
    );
}

#[test]
fn test_query_right_semijoin_view() {
    let test = Smoketest::builder().precompiled_module("views-query").build();
    test.assert_sql(
        "SELECT * FROM online_users_age",
        r#" identity | name    | age
----------+---------+-----
 1        | "Alice" | 30"#,
    );
}

#[test]
fn test_query_left_semijoin_view() {
    let test = Smoketest::builder().precompiled_module("views-query").build();
    test.assert_sql(
        "SELECT * FROM users_whos_age_is_known",
        r#" identity | name    | online
----------+---------+--------
 1        | "Alice" | true
 2        | "BOB"   | false"#,
    );
}

#[test]
fn test_query_complex_right_semijoin_view() {
    let test = Smoketest::builder().precompiled_module("views-query").build();
    test.assert_sql(
        "SELECT * FROM offline_user_20_years_old",
        r#" identity | name  | online
----------+-------+--------
 2        | "BOB" | false"#,
    );
}

#[test]
fn test_where_expr_view() {
    let test = Smoketest::builder().precompiled_module("views-query").build();
    test.assert_sql(
        "SELECT * FROM users_who_are_above_20_and_below_30",
        r#" identity | name | age
----------+------+-----"#,
    );

    test.assert_sql(
        "SELECT * FROM users_who_are_above_eq_20_and_below_eq_30",
        r#" identity | name    | age
----------+---------+-----
 1        | "Alice" | 30
 2        | "BOB"   | 20"#,
    );
}

#[test]
fn test_procedure_triggers_subscription_updates() {
    let test = Smoketest::builder().precompiled_module("views-subscribe").build();
    let sub = test.subscribe_background(&["select * from my_player"], 1).unwrap();
    test.call("insert_player_proc", &["Alice"]).unwrap();
    let events = sub.collect().unwrap();

    let projection: Vec<serde_json::Value> = events
        .into_iter()
        .map(|event| {
            let deletes = event["my_player"]["deletes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| json!({"name": row["name"]}))
                .collect::<Vec<_>>();
            let inserts = event["my_player"]["inserts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| json!({"name": row["name"]}))
                .collect::<Vec<_>>();
            json!({"my_player": {"deletes": deletes, "inserts": inserts}})
        })
        .collect();

    assert_eq!(
        serde_json::json!(projection),
        serde_json::json!([
            {"my_player": {"deletes": [], "inserts": [{"name": "Alice"}]}}
        ])
    );
}

#[test]
fn test_typescript_procedure_triggers_subscription_updates() {
    require_pnpm!();
    let mut test = Smoketest::builder().autopublish(false).build();
    test.publish_typescript_module_source(
        "views-subscribe-typescript",
        "views-subscribe-typescript",
        TS_VIEWS_SUBSCRIBE_MODULE,
    )
    .unwrap();

    let sub = test.subscribe_background(&["select * from my_player"], 1).unwrap();
    test.call("insert_player_proc", &["Alice"]).unwrap();
    let events = sub.collect().unwrap();

    let projection: Vec<serde_json::Value> = events
        .into_iter()
        .map(|event| {
            let deletes = event["my_player"]["deletes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| json!({"name": row["name"]}))
                .collect::<Vec<_>>();
            let inserts = event["my_player"]["inserts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| json!({"name": row["name"]}))
                .collect::<Vec<_>>();
            json!({"my_player": {"deletes": deletes, "inserts": inserts}})
        })
        .collect();

    assert_eq!(
        serde_json::json!(projection),
        serde_json::json!([
            {"my_player": {"deletes": [], "inserts": [{"name": "Alice"}]}}
        ])
    );
}

#[test]
fn test_typescript_query_builder_view_query() {
    require_pnpm!();
    let mut test = Smoketest::builder().autopublish(false).build();
    test.publish_typescript_module_source(
        "views-subscribe-typescript",
        "views-subscribe-typescript",
        TS_VIEWS_SUBSCRIBE_MODULE,
    )
    .unwrap();

    test.call("insert_player_proc", &["Alice"]).unwrap();

    test.assert_sql(
        "SELECT name FROM all_players",
        r#" name
---------
 "Alice""#,
    );
}

#[test]
fn test_csharp_query_builder_view_query() {
    require_dotnet!();
    let mut test = Smoketest::builder().autopublish(false).build();
    test.publish_csharp_module_source("views-csharp", "views-csharp", CS_VIEWS_QUERY_BUILDER_MODULE)
        .unwrap();

    test.call("insert_value", &["0"]).unwrap();
    test.call("insert_value", &["1"]).unwrap();
    test.call("insert_value", &["2"]).unwrap();

    test.assert_sql(
        "SELECT * FROM all",
        r#" value
-------
 0
 1
 2"#,
    );

    test.assert_sql(
        "SELECT * FROM some",
        r#" value
-------
 1"#,
    );
}
