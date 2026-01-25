//! Tests for server restart behavior.
//! Translated from smoketests/tests/zz_docker.py

use spacetimedb_smoketests::Smoketest;

const PERSON_MODULE: &str = r#"
use spacetimedb::{log, ReducerContext, Table};

#[spacetimedb::table(name = person, index(name = name_idx, btree(columns = [name])))]
pub struct Person {
    #[primary_key]
    #[auto_inc]
    id: u32,
    name: String,
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext, name: String) {
    ctx.db.person().insert(Person { id: 0, name });
}

#[spacetimedb::reducer]
pub fn say_hello(ctx: &ReducerContext) {
    for person in ctx.db.person().iter() {
        log::info!("Hello, {}!", person.name);
    }
    log::info!("Hello, World!");
}
"#;

const CONNECTED_CLIENT_MODULE: &str = r#"
use log::info;
use spacetimedb::{ConnectionId, Identity, ReducerContext, Table};

#[spacetimedb::table(name = connected_client)]
pub struct ConnectedClient {
    identity: Identity,
    connection_id: ConnectionId,
}

#[spacetimedb::reducer(client_connected)]
fn on_connect(ctx: &ReducerContext) {
    ctx.db.connected_client().insert(ConnectedClient {
        identity: ctx.sender,
        connection_id: ctx.connection_id.expect("sender connection id unset"),
    });
}

#[spacetimedb::reducer(client_disconnected)]
fn on_disconnect(ctx: &ReducerContext) {
    let sender_identity = &ctx.sender;
    let sender_connection_id = ctx.connection_id.as_ref().expect("sender connection id unset");
    let match_client = |row: &ConnectedClient| {
        &row.identity == sender_identity && &row.connection_id == sender_connection_id
    };
    if let Some(client) = ctx.db.connected_client().iter().find(match_client) {
        ctx.db.connected_client().delete(client);
    }
}

#[spacetimedb::reducer]
fn print_num_connected(ctx: &ReducerContext) {
    let n = ctx.db.connected_client().count();
    info!("CONNECTED CLIENTS: {n}")
}
"#;

/// Test data persistence across server restart.
///
/// This tests to see if SpacetimeDB can be queried after a restart.
#[test]
fn test_restart_module() {
    let mut test = Smoketest::builder().module_code(PERSON_MODULE).build();

    test.call("add", &["Robert"]).unwrap();

    test.restart_server();

    test.call("add", &["Julie"]).unwrap();
    test.call("add", &["Samantha"]).unwrap();
    test.call("say_hello", &[]).unwrap();

    let logs = test.logs(100).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("Hello, Robert!")),
        "Missing 'Hello, Robert!' in logs"
    );
    assert!(
        logs.iter().any(|l| l.contains("Hello, Julie!")),
        "Missing 'Hello, Julie!' in logs"
    );
    assert!(
        logs.iter().any(|l| l.contains("Hello, Samantha!")),
        "Missing 'Hello, Samantha!' in logs"
    );
    assert!(
        logs.iter().any(|l| l.contains("Hello, World!")),
        "Missing 'Hello, World!' in logs"
    );
}

/// Test SQL queries work after restart.
#[test]
fn test_restart_sql() {
    let mut test = Smoketest::builder().module_code(PERSON_MODULE).build();

    test.call("add", &["Robert"]).unwrap();
    test.call("add", &["Julie"]).unwrap();
    test.call("add", &["Samantha"]).unwrap();

    test.restart_server();

    let output = test.sql("SELECT name FROM person WHERE id = 3").unwrap();
    assert!(
        output.contains("Samantha"),
        "Expected 'Samantha' in SQL output: {}",
        output
    );
}

/// Test clients are auto-disconnected on restart.
#[test]
fn test_restart_auto_disconnect() {
    let mut test = Smoketest::builder().module_code(CONNECTED_CLIENT_MODULE).build();

    // Start two subscribers in the background
    let sub1 = test
        .subscribe_background(&["SELECT * FROM connected_client"], 2)
        .unwrap();
    let sub2 = test
        .subscribe_background(&["SELECT * FROM connected_client"], 2)
        .unwrap();

    // Call print_num_connected and check we have 3 clients (2 subscribers + the call)
    test.call("print_num_connected", &[]).unwrap();
    let logs = test.logs(10).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("CONNECTED CLIENTS: 3")),
        "Expected 3 connected clients before restart, logs: {:?}",
        logs
    );

    // Restart the server - this should disconnect all clients
    test.restart_server();

    // The subscriptions should fail/complete since the server restarted
    // We don't wait for them, just drop the handles
    drop(sub1);
    drop(sub2);

    // After restart, only the current call should be connected
    test.call("print_num_connected", &[]).unwrap();
    let logs = test.logs(10).unwrap();
    assert!(
        logs.iter().any(|l| l.contains("CONNECTED CLIENTS: 1")),
        "Expected 1 connected client after restart, logs: {:?}",
        logs
    );
}

// Module code for add_remove_index test (without indices)
const ADD_REMOVE_MODULE: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = t1)]
pub struct T1 { id: u64 }

#[spacetimedb::table(name = t2)]
pub struct T2 { id: u64 }

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    for id in 0..1_000 {
        ctx.db.t1().insert(T1 { id });
        ctx.db.t2().insert(T2 { id });
    }
}
"#;

// Module code for add_remove_index test (with indices)
const ADD_REMOVE_MODULE_INDEXED: &str = r#"
use spacetimedb::{ReducerContext, Table};

#[spacetimedb::table(name = t1)]
pub struct T1 { #[index(btree)] id: u64 }

#[spacetimedb::table(name = t2)]
pub struct T2 { #[index(btree)] id: u64 }

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    for id in 0..1_000 {
        ctx.db.t1().insert(T1 { id });
        ctx.db.t2().insert(T2 { id });
    }
}

#[spacetimedb::reducer]
pub fn add(ctx: &ReducerContext) {
    let id = 1_001;
    ctx.db.t1().insert(T1 { id });
    ctx.db.t2().insert(T2 { id });
}
"#;

const JOIN_QUERY: &str = "select t1.* from t1 join t2 on t1.id = t2.id where t2.id = 1001";

/// Test autoinc sequences work correctly after restart.
///
/// This is the `AddRemoveIndex` test from add_remove_index.py,
/// but restarts the server between each publish.
///
/// This detects a bug we once had where the system autoinc sequences
/// were borked after restart, leading newly-created database objects
/// to re-use IDs.
#[test]
fn test_add_remove_index_after_restart() {
    let mut test = Smoketest::builder()
        .module_code(ADD_REMOVE_MODULE)
        .autopublish(false)
        .build();

    let name = format!("test-db-{}", std::process::id());

    // Publish and attempt subscribing to a join query.
    // There are no indices, resulting in an unsupported unindexed join.
    test.publish_module_named(&name, false).unwrap();
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(result.is_err(), "Expected subscription to fail without indices");

    // Restart before adding indices
    test.restart_server();

    // Publish the indexed version.
    // Now we have indices, so the query should be accepted.
    test.write_module_code(ADD_REMOVE_MODULE_INDEXED).unwrap();
    test.publish_module_named(&name, false).unwrap();

    // Subscription should work now
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(
        result.is_ok(),
        "Expected subscription to succeed with indices, got: {:?}",
        result.err()
    );

    // Verify call works too
    test.call("add", &[]).unwrap();

    // Restart before removing indices
    test.restart_server();

    // Publish the unindexed version again, removing the index.
    // The initial subscription should be rejected again.
    test.write_module_code(ADD_REMOVE_MODULE).unwrap();
    test.publish_module_named(&name, false).unwrap();
    let result = test.subscribe(&[JOIN_QUERY], 0);
    assert!(result.is_err(), "Expected subscription to fail after removing indices");
}
