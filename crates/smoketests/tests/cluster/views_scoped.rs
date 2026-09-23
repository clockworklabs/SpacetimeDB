//! Tests for scoped views, which are materialized once per scope key
//! and shared by every subscriber whose scope resolver returns that key.
//!
//! See `crates/smoketests/modules/views-scoped` for the module.

use serde_json::{json, Value};
use spacetimedb_smoketests::{random_string, require_pnpm, ModuleLanguage, Smoketest};

/// The TypeScript equivalent of the `views-scoped` module.
const TS_VIEWS_SCOPED_MODULE: &str = r#"import { schema, t, table } from "spacetimedb/server";

const players = table(
  { name: "player" },
  {
    identity: t.identity().primaryKey(),
    name: t.string(),
    teamId: t.u64(),
    chunkX: t.i32(),
    chunkY: t.i32(),
  }
);

const chatMessages = table(
  { name: "chat_message" },
  {
    id: t.u64().primaryKey().autoInc(),
    teamId: t.u64().index(),
    text: t.string(),
  }
);

const entities = table(
  { name: "entity" },
  {
    id: t.u64().primaryKey().autoInc(),
    chunkX: t.i32().index(),
    chunkY: t.i32(),
    name: t.string(),
  }
);

const spacetimedb = schema({ players, chatMessages, entities });
export default spacetimedb;

// Every player on a team shares one materialization of the team's chat.
export const team_chat = spacetimedb.scopedView(
  { name: "team_chat", public: true, scope: t.u64() },
  t.array(chatMessages.rowType),
  ctx => ctx.db.players.identity.find(ctx.sender)?.teamId,
  (ctx, teamId) => {
    console.log(`team_chat body evaluated for team ${teamId}`);
    return Array.from(ctx.db.chatMessages.teamId.filter(teamId));
  }
);

// Like `team_chat`, but the body returns a query.
export const team_chat_query = spacetimedb.scopedView(
  { name: "team_chat_query", public: true, scope: t.u64() },
  t.array(chatMessages.rowType),
  ctx => ctx.db.players.identity.find(ctx.sender)?.teamId,
  (ctx, teamId) => ctx.from.chatMessages.where(m => m.teamId.eq(teamId)).build()
);

const ChunkKey = t.object("ChunkKey", { chunkX: t.i32(), chunkY: t.i32() });

// Every player in a chunk shares one materialization of the chunk's entities.
export const regional_entities = spacetimedb.scopedView(
  { name: "regional_entities", public: true, scope: ChunkKey },
  t.array(entities.rowType),
  ctx => {
    const player = ctx.db.players.identity.find(ctx.sender);
    return player && { chunkX: player.chunkX, chunkY: player.chunkY };
  },
  (ctx, key) =>
    Array.from(ctx.db.entities.chunkX.filter(key.chunkX)).filter(e => e.chunkY === key.chunkY)
);

export const join = spacetimedb.reducer({ name: t.string(), teamId: t.u64() }, (ctx, { name, teamId }) => {
  ctx.db.players.insert({ identity: ctx.sender, name, teamId, chunkX: 0, chunkY: 0 });
});

export const leave = spacetimedb.reducer(ctx => {
  ctx.db.players.identity.delete(ctx.sender);
});

export const set_team = spacetimedb.reducer({ teamId: t.u64() }, (ctx, { teamId }) => {
  const player = ctx.db.players.identity.find(ctx.sender);
  if (player) {
    ctx.db.players.identity.update({ ...player, teamId });
  }
});

export const set_team_proc = spacetimedb.procedure({ teamId: t.u64() }, t.unit(), (ctx, { teamId }) => {
  const sender = ctx.sender;
  ctx.withTx(tx => {
    const player = tx.db.players.identity.find(sender);
    if (player) {
      tx.db.players.identity.update({ ...player, teamId });
    }
  });
  return {};
});

export const move_to = spacetimedb.reducer({ chunkX: t.i32(), chunkY: t.i32() }, (ctx, { chunkX, chunkY }) => {
  const player = ctx.db.players.identity.find(ctx.sender);
  if (player) {
    ctx.db.players.identity.update({ ...player, chunkX, chunkY });
  }
});

export const send = spacetimedb.reducer({ teamId: t.u64(), text: t.string() }, (ctx, { teamId, text }) => {
  ctx.db.chatMessages.insert({ id: 0n, teamId, text });
});

export const spawn = spacetimedb.reducer(
  { name: t.string(), chunkX: t.i32(), chunkY: t.i32() },
  (ctx, { name, chunkX, chunkY }) => {
    ctx.db.entities.insert({ id: 0n, chunkX, chunkY, name });
  }
);
"#;

/// Build a smoketest publishing the TypeScript `views-scoped` module.
fn typescript_test() -> Smoketest {
    require_pnpm!();
    let mut test = Smoketest::builder().autopublish(false).build();
    let database_name = format!("views-scoped-typescript-{}", random_string());
    test.publish()
        .name(&database_name)
        .source(
            ModuleLanguage::TypeScript,
            "views-scoped-typescript",
            TS_VIEWS_SCOPED_MODULE,
        )
        .run()
        .unwrap();
    test
}

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

#[test]
fn test_scoped_view_moves_subscriber_to_new_scope() {
    let test = Smoketest::builder().precompiled_module("views-scoped").build();

    test.call("send", &["1", "\"red one\""]).unwrap();
    test.call("send", &["2", "\"blue one\""]).unwrap();
    test.call("join", &["\"alice\"", "1"]).unwrap();

    let sub = test
        .subscribe(&["SELECT * FROM team_chat"])
        .expect_rows(3)
        .background()
        .unwrap();

    // Alice switches teams, and sees the blue team's chat instead of the red team's.
    test.call("set_team", &["2"]).unwrap();
    // She no longer sees the red team's messages.
    test.call("send", &["1", "\"red two\""]).unwrap();
    test.call("send", &["2", "\"blue two\""]).unwrap();
    // She leaves the game, and sees no team's chat.
    test.call("leave", &[]).unwrap();
    // Nor any further messages.
    test.call("send", &["2", "\"blue three\""]).unwrap();

    assert_eq!(
        json!(project(sub.collect().unwrap(), "team_chat", &["text"])),
        json!([
            {"deletes": [{"text": "red one"}], "inserts": [{"text": "blue one"}]},
            {"deletes": [], "inserts": [{"text": "blue two"}]},
            {"deletes": [{"text": "blue one"}, {"text": "blue two"}], "inserts": []},
        ])
    );
}

#[test]
fn test_scoped_view_moves_subscriber_into_shared_scope() {
    let test = Smoketest::builder().precompiled_module("views-scoped").build();

    let alice_token = test.read_token().unwrap();
    test.call("join", &["\"alice\"", "1"]).unwrap();
    let alice = test
        .subscribe(&["SELECT * FROM team_chat"])
        .expect_rows(2)
        .background()
        .unwrap();

    test.new_identity().unwrap();
    test.call("join", &["\"bob\"", "2"]).unwrap();
    test.call("send", &["2", "\"welcome\""]).unwrap();
    let bob = test
        .subscribe(&["SELECT * FROM team_chat"])
        .expect_rows(1)
        .background()
        .unwrap();

    // Alice joins Bob's team, whose chat is already materialized for Bob.
    test.login_with_token(&alice_token).unwrap();
    test.call("set_team", &["2"]).unwrap();
    // From then on, both see the team's messages.
    test.call("send", &["2", "\"hello both\""]).unwrap();

    assert_eq!(
        json!(project(alice.collect().unwrap(), "team_chat", &["text"])),
        json!([
            {"deletes": [], "inserts": [{"text": "welcome"}]},
            {"deletes": [], "inserts": [{"text": "hello both"}]},
        ])
    );
    assert_eq!(
        json!(project(bob.collect().unwrap(), "team_chat", &["text"])),
        json!([{"deletes": [], "inserts": [{"text": "hello both"}]}])
    );
}

#[test]
fn test_scoped_view_with_composite_key_follows_player() {
    let test = Smoketest::builder().precompiled_module("views-scoped").build();

    test.call("spawn", &["\"tree\"", "0", "0"]).unwrap();
    test.call("spawn", &["\"rock\"", "5", "5"]).unwrap();
    test.call("join", &["\"alice\"", "1"]).unwrap();

    let sub = test
        .subscribe(&["SELECT * FROM regional_entities"])
        .expect_rows(2)
        .background()
        .unwrap();

    test.call("move_to", &["5", "5"]).unwrap();
    test.call("spawn", &["\"bush\"", "5", "5"]).unwrap();

    assert_eq!(
        json!(project(sub.collect().unwrap(), "regional_entities", &["name"])),
        json!([
            {"deletes": [{"name": "tree"}], "inserts": [{"name": "rock"}]},
            {"deletes": [], "inserts": [{"name": "bush"}]},
        ])
    );
}

#[test]
fn test_typescript_scoped_view_sql_selects_callers_scope() {
    let test = typescript_test();

    test.call("send", &["1", "\"red one\""]).unwrap();
    test.call("send", &["2", "\"blue one\""]).unwrap();
    test.call("join", &["\"alice\"", "1"]).unwrap();

    for view in ["team_chat", "team_chat_query"] {
        test.assert_sql(
            &format!("SELECT text FROM {view}"),
            r#" text
-----------
 "red one""#,
        );
    }
}

#[test]
fn test_typescript_scoped_view_is_computed_once_per_scope() {
    let test = typescript_test();

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

    test.login_with_token(&owner_token).unwrap();
    assert_eq!(count_logs(&test, "team_chat body evaluated for team 7"), 2);
}

#[test]
fn test_typescript_scoped_view_moves_subscriber_to_new_scope() {
    let test = typescript_test();

    test.call("send", &["1", "\"red one\""]).unwrap();
    test.call("send", &["2", "\"blue one\""]).unwrap();
    test.call("join", &["\"alice\"", "1"]).unwrap();

    let sub = test
        .subscribe(&["SELECT * FROM team_chat"])
        .expect_rows(3)
        .background()
        .unwrap();

    test.call("set_team", &["2"]).unwrap();
    test.call("send", &["1", "\"red two\""]).unwrap();
    test.call("send", &["2", "\"blue two\""]).unwrap();
    test.call("leave", &[]).unwrap();
    test.call("send", &["2", "\"blue three\""]).unwrap();

    assert_eq!(
        json!(project(sub.collect().unwrap(), "team_chat", &["text"])),
        json!([
            {"deletes": [{"text": "red one"}], "inserts": [{"text": "blue one"}]},
            {"deletes": [], "inserts": [{"text": "blue two"}]},
            {"deletes": [{"text": "blue one"}, {"text": "blue two"}], "inserts": []},
        ])
    );
}

#[test]
fn test_typescript_scoped_view_with_composite_key_follows_player() {
    let test = typescript_test();

    test.call("spawn", &["\"tree\"", "0", "0"]).unwrap();
    test.call("spawn", &["\"rock\"", "5", "5"]).unwrap();
    test.call("join", &["\"alice\"", "1"]).unwrap();

    let sub = test
        .subscribe(&["SELECT * FROM regional_entities"])
        .expect_rows(2)
        .background()
        .unwrap();

    test.call("move_to", &["5", "5"]).unwrap();
    test.call("spawn", &["\"bush\"", "5", "5"]).unwrap();

    assert_eq!(
        json!(project(sub.collect().unwrap(), "regional_entities", &["name"])),
        json!([
            {"deletes": [{"name": "tree"}], "inserts": [{"name": "rock"}]},
            {"deletes": [], "inserts": [{"name": "bush"}]},
        ])
    );
}

/// Switch teams from a procedure, and check that the subscriber moves to the new team's chat.
fn check_procedure_moves_subscriber(test: &Smoketest) {
    test.call("send", &["1", "\"red one\""]).unwrap();
    test.call("send", &["2", "\"blue one\""]).unwrap();
    test.call("join", &["\"alice\"", "1"]).unwrap();

    let sub = test
        .subscribe(&["SELECT * FROM team_chat"])
        .expect_rows(1)
        .background()
        .unwrap();
    test.call("set_team_proc", &["2"]).unwrap();

    assert_eq!(
        json!(project(sub.collect().unwrap(), "team_chat", &["text"])),
        json!([{"deletes": [{"text": "red one"}], "inserts": [{"text": "blue one"}]}])
    );
}

#[test]
fn test_scoped_view_moves_subscriber_from_procedure() {
    let test = Smoketest::builder().precompiled_module("views-scoped").build();
    check_procedure_moves_subscriber(&test);
}

#[test]
fn test_typescript_scoped_view_moves_subscriber_from_procedure() {
    let test = typescript_test();
    check_procedure_moves_subscriber(&test);
}
