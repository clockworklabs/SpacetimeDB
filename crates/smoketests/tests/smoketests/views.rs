use serde_json::{json, Value};
use spacetimedb_smoketests::{require_dotnet, require_pnpm, Smoketest};

const TS_VIEWS_SUBSCRIBE_MODULE: &str = r#"import { schema, t, table } from "spacetimedb/server";

const playerState = table(
  { name: "player_state" },
  {
    identity: t.identity().primaryKey(),
    name: t.string().unique(),
    online: t.bool(),
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

export const online_players = spacetimedb.anonymousView(
  { public: true },
  t.array(playerState.rowType),
  ctx => ctx.from.playerState.where(row => row.online)
);

export const insert_player_proc = spacetimedb.procedure(
  { name: t.string() },
  t.unit(),
  (ctx, { name }) => {
    const sender = ctx.sender;
    ctx.withTx(tx => {
      tx.db.playerState.insert({ name, identity: sender, online: true });
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
        public bool Alive;
    }

    [Reducer]
    public static void InsertValue(ReducerContext ctx, uint value, bool alive)
    {
        ctx.Db.Table.Insert(new Table { Value = value, Alive = alive });
    }

    [View(Accessor = "all", Public = true)]
    public static IQuery<Table> All(ViewContext ctx)
    {
        return ctx.From.Table();
    }

    [View(Accessor = "some", Public = true)]
    public static IQuery<Table> Some(ViewContext ctx)
    {
        return ctx.From.Table().Where(Row => Row.Alive);
    }
}
"#;

const CS_COUNT_VIEW_MODULE: &str = r#"using SpacetimeDB;

[SpacetimeDB.Type]
public partial struct ItemCount
{
    public ulong count;
}

public static partial class Module
{
    [Table(Accessor = "item", Public = true)]
    public partial struct Item
    {
        [PrimaryKey]
        public uint id;
        public uint value;
    }

    [View(Accessor = "sender_table_count", Public = true)]
    public static ItemCount? sender_table_count(ViewContext ctx)
    {
        return new ItemCount { count = ctx.Db.item.Count };
    }

    [View(Accessor = "anon_table_count", Public = true)]
    public static ItemCount? anon_table_count(AnonymousViewContext ctx)
    {
        return new ItemCount { count = ctx.Db.item.Count };
    }

    [Reducer]
    public static void insert_item(ReducerContext ctx, uint id, uint value)
    {
        ctx.Db.item.Insert(new Item { id = id, value = value });
    }

    [Reducer]
    public static void replace_item(ReducerContext ctx, uint id, uint value)
    {
        ctx.Db.item.id.Delete(id);
        ctx.Db.item.Insert(new Item { id = id, value = value });
    }

    [Reducer]
    public static void delete_item(ReducerContext ctx, uint id)
    {
        ctx.Db.item.id.Delete(id);
    }
}
"#;

const TS_COUNT_VIEW_MODULE: &str = r#"import { schema, t, table } from "spacetimedb/server";

const item = table(
  { name: "item" },
  {
    id: t.u32().primaryKey(),
    value: t.u32(),
  }
);

const itemCount = t.object("ItemCountRow", {
  count: t.u64(),
});

const spacetimedb = schema({ item });
export default spacetimedb;

export const sender_table_count = spacetimedb.view(
  { public: true },
  t.option(itemCount),
  ctx => ({ count: ctx.db.item.count() })
);

export const anon_table_count = spacetimedb.anonymousView(
  { public: true },
  t.option(itemCount),
  ctx => ({ count: ctx.db.item.count() })
);

export const insert_item = spacetimedb.reducer(
  { id: t.u32(), value: t.u32() },
  (ctx, { id, value }) => {
    ctx.db.item.insert({ id, value });
  }
);

export const replace_item = spacetimedb.reducer(
  { id: t.u32(), value: t.u32() },
  (ctx, { id, value }) => {
    ctx.db.item.id.delete(id);
    ctx.db.item.insert({ id, value });
  }
);

export const delete_item = spacetimedb.reducer(
  { id: t.u32() },
  (ctx, { id }) => {
    ctx.db.item.id.delete(id);
  }
);
"#;

fn project_fields(events: Vec<Value>, view_name: &str, projected_fields: &[&str]) -> Vec<Value> {
    let project_row = |row: &Value| {
        if projected_fields.is_empty() {
            row.clone()
        } else {
            let mut projected = serde_json::Map::new();
            for field in projected_fields {
                if let Some(value) = row.get(*field) {
                    projected.insert((*field).to_string(), value.clone());
                }
            }
            Value::Object(projected)
        }
    };

    events
        .into_iter()
        .map(|event| {
            json!({
                view_name: {
                    "deletes": event[view_name]["deletes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(&project_row)
                        .collect::<Vec<_>>(),
                    "inserts": event[view_name]["inserts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(&project_row)
                        .collect::<Vec<_>>()
                }
            })
        })
        .collect()
}

fn assert_count_view_refresh_behavior(test: &Smoketest, view_name: &str, id: &str, value: &str, updated_value: &str) {
    let query = format!("select * from {view_name}");
    let sub = test.subscribe_background(&[&query], 2).unwrap();

    test.call("insert_item", &[id, value]).unwrap();
    test.call("replace_item", &[id, updated_value]).unwrap();
    test.call("delete_item", &[id]).unwrap();

    let events = sub.collect().unwrap();
    let projection = project_fields(events, view_name, &["count"]);
    assert_eq!(
        serde_json::json!(projection),
        serde_json::json!([
            {view_name: {"deletes": [{"count": 0}], "inserts": [{"count": 1}]}},
            {view_name: {"deletes": [{"count": 1}], "inserts": [{"count": 0}]}}
        ])
    );
}

fn assert_all_count_view_refreshes(test: &Smoketest) {
    assert_count_view_refresh_behavior(test, "sender_table_count", "1", "10", "11");
    assert_count_view_refresh_behavior(test, "anon_table_count", "2", "20", "21");
}

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

    let projection = project_fields(events, "my_player", &["name"]);
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
    let projection = project_fields(events, "my_player", &["name"]);
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

    let projection = project_fields(events, "my_player", &["name"]);
    assert_eq!(
        serde_json::json!(projection),
        serde_json::json!([
            {"my_player": {"deletes": [], "inserts": [{"name": "Alice"}]}}
        ])
    );
}

#[test]
fn test_rust_count_view_subscription_refreshes() {
    let test = Smoketest::builder().precompiled_module("views-count").build();
    assert_all_count_view_refreshes(&test);
}

#[test]
fn test_csharp_count_view_subscription_refreshes() {
    require_dotnet!();

    let mut test = Smoketest::builder().autopublish(false).build();
    test.publish_csharp_module_source("views-count-csharp", "views-count-csharp", CS_COUNT_VIEW_MODULE)
        .unwrap();

    assert_all_count_view_refreshes(&test);
}

#[test]
fn test_typescript_count_view_subscription_refreshes() {
    require_pnpm!();

    let mut test = Smoketest::builder().autopublish(false).build();
    test.publish_typescript_module_source("views-count-typescript", "views-count-typescript", TS_COUNT_VIEW_MODULE)
        .unwrap();

    assert_all_count_view_refreshes(&test);
}

#[test]
fn test_disconnect_does_not_break_sender_view() {
    let test = Smoketest::builder().precompiled_module("views-sql").build();

    test.call("set_player_state", &["42", "1"]).unwrap();

    // Two connections subscribe to the same view.
    let sub_keep = test.subscribe_background(&["SELECT * FROM player"], 2).unwrap();
    let sub_drop = test.subscribe_background(&["SELECT * FROM player"], 1).unwrap();

    // Both connections should receive the first update.
    // After one connection disconnects, the other should still receive updates.
    test.call("set_player_state", &["42", "2"]).unwrap();
    let _ = sub_drop.collect().unwrap();
    test.call("set_player_state", &["42", "3"]).unwrap();

    let events = sub_keep.collect().unwrap();

    assert_eq!(events.len(), 2, "Expected two updates for player, got: {events:?}");
    let inserts = events[1]["player"]["inserts"]
        .as_array()
        .expect("Expected inserts array on player update");
    assert!(
        inserts
            .iter()
            .any(|row| row["id"] == json!(42) && row["level"] == json!(3)),
        "Expected player id=42 level=3 insert after disconnect, got: {events:?}"
    );
}

#[test]
fn test_disconnect_does_not_break_anonymous_view() {
    let test = Smoketest::builder().precompiled_module("views-sql").build();

    // Seed a row in the anonymous-view source table.
    test.call("add_player_level", &["0", "2"]).unwrap();

    // Two connections subscribe to the same anonymous view.
    let sub_keep = test
        .subscribe_background(&["SELECT * FROM player_and_level"], 2)
        .unwrap();
    let sub_drop = test
        .subscribe_background(&["SELECT * FROM player_and_level"], 1)
        .unwrap();

    // Both connections should receive the first update.
    // After one connection disconnects, the other should still receive updates.
    test.call("add_player_level", &["1", "2"]).unwrap();
    let _ = sub_drop.collect().unwrap();
    test.call("add_player_level", &["2", "2"]).unwrap();

    let events = sub_keep.collect().unwrap();

    assert_eq!(
        events.len(),
        2,
        "Expected two updates for player_and_level, got: {events:?}"
    );
    let inserts = events[1]["player_and_level"]["inserts"]
        .as_array()
        .expect("Expected inserts array on player_and_level update");
    assert!(
        inserts
            .iter()
            .any(|row| row["id"] == json!(2) && row["level"] == json!(2)),
        "Expected player id=2 level=2 insert after disconnect, got: {events:?}"
    );
}

#[test]
fn test_typescript_query_builder_view_query() {
    require_pnpm!();
    let mut test = Smoketest::builder().autopublish(false).build();
    test.publish_typescript_module_source(
        "views-query-builder-typescript",
        "views-query-builder-typescript",
        TS_VIEWS_SUBSCRIBE_MODULE,
    )
    .unwrap();

    test.call("insert_player_proc", &["Alice"]).unwrap();

    test.assert_sql(
        "SELECT name FROM online_players",
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

    test.call("insert_value", &["0", "false"]).unwrap();
    test.call("insert_value", &["1", "true"]).unwrap();
    test.call("insert_value", &["2", "false"]).unwrap();

    test.assert_sql(
        "SELECT * FROM some",
        r#" value | alive
-------+-------
 1     | true"#,
    );
}

enum PkJoinMutation {
    UpdateLhs { id: u64, ok: bool },
    UpdateRhs { id: u64, ok: bool },
    DeleteLhs { id: u64 },
}

fn apply_pk_join_mutation(test: &Smoketest, mutation: &PkJoinMutation) {
    match mutation {
        PkJoinMutation::UpdateLhs { id, ok } => {
            let id = id.to_string();
            let ok = if *ok { "true" } else { "false" };
            test.call("update_pk_join_lhs", &[id.as_str(), ok]).unwrap();
        }
        PkJoinMutation::UpdateRhs { id, ok } => {
            let id = id.to_string();
            let ok = if *ok { "true" } else { "false" };
            test.call("update_pk_join_rhs", &[id.as_str(), ok]).unwrap();
        }
        PkJoinMutation::DeleteLhs { id } => {
            let id = id.to_string();
            test.call("delete_pk_join_lhs", &[id.as_str()]).unwrap();
        }
    }
}

fn expected_pk_join_projection(view_name: &str) -> Value {
    let mk_event = |deletes: Vec<Value>, inserts: Vec<Value>| {
        let mut event_payload = serde_json::Map::new();
        event_payload.insert("deletes".to_string(), Value::Array(deletes));
        event_payload.insert("inserts".to_string(), Value::Array(inserts));

        let mut event = serde_json::Map::new();
        event.insert(view_name.to_string(), Value::Object(event_payload));
        Value::Object(event)
    };

    Value::Array(vec![
        mk_event(vec![], vec![json!({"id": 1, "ok": true})]),
        mk_event(vec![json!({"id": 1, "ok": true})], vec![]),
        mk_event(vec![], vec![json!({"id": 2, "ok": true})]),
        mk_event(vec![json!({"id": 2, "ok": true})], vec![]),
    ])
}

fn run_pk_join_subscription_test(query: &str, projected_view_name: &str, mutations: &[PkJoinMutation]) {
    let test = Smoketest::builder().precompiled_module("views-query").build();

    // Seed rows for identity A in both underlying tables.
    test.call("update_pk_join_lhs", &["200", "true"]).unwrap();
    test.call("update_pk_join_rhs", &["200", "true"]).unwrap();

    // Switch to identity B so each underlying table has rows for 2 identities.
    test.new_identity().unwrap();

    let sub = test.subscribe_background(&[query], 4).unwrap();

    for mutation in mutations {
        apply_pk_join_mutation(&test, mutation);
    }

    let update_events = sub.collect().unwrap();
    assert_eq!(
        serde_json::json!(project_fields(update_events, projected_view_name, &["id", "ok"])),
        expected_pk_join_projection(projected_view_name)
    );
}

#[test]
fn test_subscribe_join_pk_views_with_filters_on_both_sides() {
    let query = "SELECT pk_join_lhs_sender_view.* \
                 FROM pk_join_lhs_sender_view \
                 JOIN pk_join_rhs_sender_view \
                 ON pk_join_lhs_sender_view.id = pk_join_rhs_sender_view.id \
                 WHERE pk_join_lhs_sender_view.ok = true \
                 AND pk_join_rhs_sender_view.ok = true";

    run_pk_join_subscription_test(
        query,
        "pk_join_lhs_sender_view",
        &[
            PkJoinMutation::UpdateLhs { id: 1, ok: true },
            PkJoinMutation::UpdateRhs { id: 1, ok: true },
            PkJoinMutation::UpdateRhs { id: 1, ok: false },
            PkJoinMutation::UpdateRhs { id: 2, ok: true },
            PkJoinMutation::UpdateLhs { id: 2, ok: true },
            PkJoinMutation::UpdateLhs { id: 2, ok: false },
        ],
    );
}

#[test]
fn test_subscribe_join_anon_pk_views_with_filters_on_both_sides() {
    let query = "SELECT pk_join_lhs_view.* \
                 FROM pk_join_lhs_view \
                 JOIN pk_join_rhs_view \
                 ON pk_join_lhs_view.id = pk_join_rhs_view.id \
                 WHERE pk_join_lhs_view.ok = true \
                 AND pk_join_rhs_view.ok = true";

    run_pk_join_subscription_test(
        query,
        "pk_join_lhs_view",
        &[
            PkJoinMutation::UpdateLhs { id: 1, ok: true },
            PkJoinMutation::UpdateRhs { id: 1, ok: true },
            PkJoinMutation::UpdateRhs { id: 1, ok: false },
            PkJoinMutation::UpdateRhs { id: 2, ok: true },
            PkJoinMutation::UpdateLhs { id: 2, ok: true },
            PkJoinMutation::UpdateLhs { id: 2, ok: false },
        ],
    );
}

#[test]
fn test_subscribe_join_anon_pk_view_with_table_and_filter() {
    let query = "SELECT pk_join_lhs_view.* \
                 FROM pk_join_lhs_view \
                 JOIN pk_join_rhs \
                 ON pk_join_lhs_view.id = pk_join_rhs.id \
                 WHERE pk_join_lhs_view.ok = true \
                 AND pk_join_rhs.identity = :sender";

    run_pk_join_subscription_test(
        query,
        "pk_join_lhs_view",
        &[
            PkJoinMutation::UpdateLhs { id: 1, ok: true },
            PkJoinMutation::UpdateRhs { id: 1, ok: true },
            PkJoinMutation::UpdateLhs { id: 1, ok: false },
            PkJoinMutation::UpdateRhs { id: 2, ok: true },
            PkJoinMutation::UpdateLhs { id: 2, ok: true },
            PkJoinMutation::UpdateLhs { id: 2, ok: false },
        ],
    );
}

#[test]
fn test_subscribe_join_anon_pk_view_with_table() {
    let query = "SELECT pk_join_lhs_view.* \
                 FROM pk_join_lhs_view \
                 JOIN pk_join_rhs \
                 ON pk_join_lhs_view.id = pk_join_rhs.id \
                 WHERE pk_join_rhs.identity = :sender";

    run_pk_join_subscription_test(
        query,
        "pk_join_lhs_view",
        &[
            PkJoinMutation::UpdateLhs { id: 1, ok: true },
            PkJoinMutation::UpdateRhs { id: 1, ok: true },
            PkJoinMutation::DeleteLhs { id: 1 },
            PkJoinMutation::UpdateRhs { id: 2, ok: true },
            PkJoinMutation::UpdateLhs { id: 2, ok: true },
            PkJoinMutation::DeleteLhs { id: 2 },
        ],
    );
}

#[test]
fn test_subscribe_join_pk_view_with_table() {
    let query = "SELECT pk_join_lhs_sender_view.* \
                 FROM pk_join_lhs_sender_view \
                 JOIN pk_join_rhs \
                 ON pk_join_lhs_sender_view.id = pk_join_rhs.id";

    run_pk_join_subscription_test(
        query,
        "pk_join_lhs_sender_view",
        &[
            PkJoinMutation::UpdateLhs { id: 1, ok: true },
            PkJoinMutation::UpdateRhs { id: 1, ok: true },
            PkJoinMutation::DeleteLhs { id: 1 },
            PkJoinMutation::UpdateRhs { id: 2, ok: true },
            PkJoinMutation::UpdateLhs { id: 2, ok: true },
            PkJoinMutation::DeleteLhs { id: 2 },
        ],
    );
}
