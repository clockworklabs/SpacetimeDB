use std::path::PathBuf;

use serde_json::{json, Value};
use spacetimedb_smoketests::{require_local_server, workspace_root, Smoketest};

const STALE_VIEW_BACKING_TABLE_FIXTURE_IDENTITY: &str =
    "c200f6ec405075e508c2ed6474019332d6a2a46c69614306cc4bd980e0b8b767";

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

fn stale_view_backing_table_fixture() -> PathBuf {
    workspace_root()
        .join("crates")
        .join("smoketests")
        .join("fixtures")
        .join("stale-view-backing-table-v2.6.0")
}

fn stale_view_backing_table_test() -> Smoketest {
    let test = Smoketest::builder()
        .data_dir_fixture(
            stale_view_backing_table_fixture(),
            STALE_VIEW_BACKING_TABLE_FIXTURE_IDENTITY,
        )
        .autopublish(false)
        .build();

    test.new_identity().unwrap();
    test
}

#[test]
fn test_repair_stale_sender_scoped_view_backing_table_on_startup() {
    require_local_server!();

    let test = stale_view_backing_table_test();

    let sender_view_sub = test
        .subscribe(&["select * from player"])
        .expect_rows(2)
        .background()
        .unwrap();

    test.call("set_player_state", &["42", "1"]).unwrap();
    test.call("set_player_state", &["42", "2"]).unwrap();

    let sender_view_events = sender_view_sub.collect().unwrap();
    let sender_view_projection = project_fields(sender_view_events, "player", &["id", "level"]);
    assert_eq!(
        serde_json::json!(sender_view_projection),
        json!([
            {
                "player": {
                    "deletes": [],
                    "inserts": [{ "id": 42, "level": 1 }]
                }
            },
            {
                "player": {
                    "deletes": [{ "id": 42, "level": 1 }],
                    "inserts": [{ "id": 42, "level": 2 }]
                }
            }
        ])
    );
}

#[test]
fn test_repair_stale_anonymous_view_backing_table_on_startup() {
    require_local_server!();

    let test = stale_view_backing_table_test();

    let anonymous_view_sub = test
        .subscribe(&["select * from player_and_level"])
        .expect_rows(2)
        .background()
        .unwrap();

    test.call("add_player_level", &["1", "2"]).unwrap();
    test.call("add_player_level", &["2", "2"]).unwrap();

    let anonymous_view_events = anonymous_view_sub.collect().unwrap();
    let anonymous_view_projection = project_fields(anonymous_view_events, "player_and_level", &["id", "level"]);
    assert_eq!(
        serde_json::json!(anonymous_view_projection),
        json!([
            {
                "player_and_level": {
                    "deletes": [],
                    "inserts": [{ "id": 1, "level": 2 }]
                }
            },
            {
                "player_and_level": {
                    "deletes": [],
                    "inserts": [{ "id": 2, "level": 2 }]
                }
            }
        ])
    );
}
