//! Tests for scoped views, which are materialized once per scope key
//! and shared by every subscriber whose scope resolver returns that key.
//!
//! See `crates/smoketests/modules/views-scoped` for the module.

use serde_json::{json, Value};
use spacetimedb_smoketests::Smoketest;

/// Project the rows of `table` in each subscription update to `fields`, sorting them for comparison.
fn project(events: Vec<Value>, table: &str, fields: &[&str]) -> Vec<Value> {
    let project_rows = |rows: &Value| {
        let mut rows = rows
            .as_array()
            .map(|rows| {
                rows.iter()
                    .map(|row| {
                        let projected = fields
                            .iter()
                            .map(|field| ((*field).to_string(), row[*field].clone()))
                            .collect::<serde_json::Map<_, _>>();
                        Value::Object(projected)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        rows.sort_by_key(|row| row.to_string());
        rows
    };
    events
        .into_iter()
        .map(|event| {
            json!({
                "deletes": project_rows(&event[table]["deletes"]),
                "inserts": project_rows(&event[table]["inserts"]),
            })
        })
        .collect()
}

/// Returns the number of log lines containing `needle`.
fn count_logs(test: &Smoketest, needle: &str) -> usize {
    test.logs(1000)
        .unwrap()
        .iter()
        .filter(|line| line.contains(needle))
        .count()
}

#[test]
fn test_scoped_view_sql_selects_callers_scope() {
    let test = Smoketest::builder().precompiled_module("views-scoped").build();

    test.call("send", &["1", "\"red one\""]).unwrap();
    test.call("send", &["2", "\"blue one\""]).unwrap();

    // Not a player, so in no scope.
    test.assert_sql(
        "SELECT text FROM team_chat",
        r#" text
------"#,
    );

    test.call("join", &["\"alice\"", "1"]).unwrap();
    test.assert_sql(
        "SELECT text FROM team_chat",
        r#" text
-----------
 "red one""#,
    );
    test.assert_sql(
        "SELECT text FROM team_chat_query",
        r#" text
-----------
 "red one""#,
    );
}

#[test]
fn test_scoped_view_is_computed_once_per_scope() {
    let test = Smoketest::builder().precompiled_module("views-scoped").build();

    let owner_token = test.read_token().unwrap();
    test.call("join", &["\"alice\"", "7"]).unwrap();
    let alice = test
        .subscribe(&["SELECT * FROM team_chat"])
        .expect_rows(1)
        .background()
        .unwrap();

    test.new_identity().unwrap();
    test.call("join", &["\"bob\"", "7"]).unwrap();
    let bob = test
        .subscribe(&["SELECT * FROM team_chat"])
        .expect_rows(1)
        .background()
        .unwrap();

    test.call("send", &["7", "\"hello team\""]).unwrap();

    let expected = json!([{"deletes": [], "inserts": [{"text": "hello team"}]}]);
    assert_eq!(
        json!(project(alice.collect().unwrap(), "team_chat", &["text"])),
        expected
    );
    assert_eq!(json!(project(bob.collect().unwrap(), "team_chat", &["text"])), expected);

    // The body ran once when Alice subscribed, and once when the message was sent.
    // Bob shared Alice's materialization rather than computing his own.
    test.login_with_token(&owner_token).unwrap();
    assert_eq!(count_logs(&test, "team_chat body evaluated for team 7"), 2);
}

#[test]
fn test_scoped_view_updates_only_its_scope() {
    let test = Smoketest::builder().precompiled_module("views-scoped").build();

    test.call("join", &["\"alice\"", "1"]).unwrap();
    let sub = test
        .subscribe(&["SELECT * FROM team_chat"])
        .expect_rows(1)
        .background()
        .unwrap();

    // A message to another team is not seen.
    test.call("send", &["2", "\"blue secret\""]).unwrap();
    test.call("send", &["1", "\"red news\""]).unwrap();

    assert_eq!(
        json!(project(sub.collect().unwrap(), "team_chat", &["text"])),
        json!([{"deletes": [], "inserts": [{"text": "red news"}]}])
    );
}

#[test]
fn test_scoped_view_with_composite_key() {
    let test = Smoketest::builder().precompiled_module("views-scoped").build();

    test.call("spawn", &["\"tree\"", "0", "0"]).unwrap();
    test.call("spawn", &["\"rock\"", "0", "1"]).unwrap();
    test.call("join", &["\"alice\"", "1"]).unwrap();

    test.assert_sql(
        "SELECT name FROM regional_entities",
        r#" name
--------
 "tree""#,
    );
}
