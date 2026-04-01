use spacetimedb_smoketests::Smoketest;

/// Module code shared by all three databases (A = initiator, B = relay, C = receiver).
///
/// Tables:
/// - `PingLog(id auto_inc PK, message: String, priority: u32)` — records pings received.
///
/// Reducers:
/// - `record_ping(payload)` — terminal: inserts payload into ping_log.
/// - `relay_ping(c_hex, payload)` — middle hop: forwards payload to C via `record_ping`,
///     then records a "relay:<message>" entry locally so B's participation is verifiable.
/// - `chain_ping(b_hex, c_hex, message, priority)` — initiator: encodes a PingPayload and
///     calls `relay_ping` on B (which in turn calls `record_ping` on C), then records a
///     "chain:<message>" entry locally.
///
/// This exercises a two-hop cross-DB call chain (A → B → C).
const MODULE_CODE: &str = r#"
use spacetimedb::{log, ReducerContext, Table, Identity, SpacetimeType};

#[derive(SpacetimeType)]
pub struct PingPayload {
    pub message: String,
    pub priority: u32,
}

#[spacetimedb::table(accessor = ping_log, public)]
pub struct PingLog {
    #[primary_key]
    #[auto_inc]
    id: u64,
    message: String,
    priority: u32,
}

/// Terminal hop: stores the payload in ping_log.
#[spacetimedb::reducer]
pub fn record_ping(ctx: &ReducerContext, payload: PingPayload) {
    log::info!("record_ping: message={} priority={}", payload.message, payload.priority);
    ctx.db.ping_log().insert(PingLog { id: 0, message: payload.message, priority: payload.priority });
}

/// Middle hop: forwards payload to C via `record_ping`, then records locally.
#[spacetimedb::reducer]
pub fn relay_ping(ctx: &ReducerContext, c_hex: String, payload: PingPayload) {
    log::info!("relay_ping: forwarding to {c_hex}");
    let c = Identity::from_hex(&c_hex).expect("invalid C identity hex");
    let args = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(PingPayload { message: payload.message.clone(), priority: payload.priority },))
        .expect("failed to encode args for record_ping");
    spacetimedb::remote_reducer::call_reducer_on_db(c, "record_ping", &args)
        .unwrap_or_else(|e| panic!("relay_ping: call to C failed: {e}"));
    ctx.db.ping_log().insert(PingLog { id: 0, message: format!("relay:{}", payload.message), priority: payload.priority });
}

/// Initiating hop: calls `relay_ping` on B (which calls `record_ping` on C), then records locally.
#[spacetimedb::reducer]
pub fn chain_ping(ctx: &ReducerContext, b_hex: String, c_hex: String, message: String, priority: u32) {
    log::info!("chain_ping: starting A->B->C chain, message={message}");
    let b = Identity::from_hex(&b_hex).expect("invalid B identity hex");
    let payload = PingPayload { message: message.clone(), priority };
    let args = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(c_hex, payload))
        .expect("failed to encode args for relay_ping");
    spacetimedb::remote_reducer::call_reducer_on_db(b, "relay_ping", &args)
        .unwrap_or_else(|e| panic!("chain_ping: call to B failed: {e}"));
    ctx.db.ping_log().insert(PingLog { id: 0, message: format!("chain:{message}"), priority });
}
"#;

fn query_ping_log(test: &Smoketest, db_identity: &str) -> String {
    test.spacetime(&[
        "sql",
        "--server",
        &test.server_url,
        db_identity,
        "SELECT message, priority FROM ping_log ORDER BY id",
    ])
    .unwrap_or_else(|e| panic!("sql query on {db_identity} failed: {e}"))
}

/// Two-hop chain: A.chain_ping → B.relay_ping → C.record_ping.
///
/// After the call:
/// - C's ping_log has the original message.
/// - B's ping_log has "relay:<message>", confirming B was the relay.
/// - A's ping_log has "chain:<message>", confirming A initiated the chain.
#[test]
fn test_cross_db_chain_call() {
    let pid = std::process::id();
    let db_a_name = format!("chain-a-{pid}");
    let db_b_name = format!("chain-b-{pid}");
    let db_c_name = format!("chain-c-{pid}");

    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    // Publish C first (terminal), then B (relay), then A (initiator).
    test.publish_module_named(&db_c_name, false)
        .expect("failed to publish C");
    let db_c_identity = test.database_identity.clone().expect("C identity not set");

    test.publish_module_named(&db_b_name, false)
        .expect("failed to publish B");
    let db_b_identity = test.database_identity.clone().expect("B identity not set");

    test.publish_module_named(&db_a_name, false)
        .expect("failed to publish A");
    let db_a_identity = test.database_identity.clone().expect("A identity not set");

    // Initiate the A → B → C chain.
    test.call("chain_ping", &[&db_b_identity, &db_c_identity, "hello-chain", "7"])
        .expect("chain_ping call failed");

    // C should have the original message.
    let c_log = query_ping_log(&test, &db_c_identity);
    assert!(
        c_log.contains("hello-chain"),
        "C ping_log should contain 'hello-chain' (original message), got:\n{c_log}"
    );
    assert!(
        c_log.contains('7'),
        "C ping_log should contain priority 7, got:\n{c_log}"
    );

    // B should have "relay:hello-chain", confirming it was the relay hop.
    let b_log = query_ping_log(&test, &db_b_identity);
    assert!(
        b_log.contains("relay:hello-chain"),
        "B ping_log should contain 'relay:hello-chain', got:\n{b_log}"
    );

    // A should have "chain:hello-chain", confirming it initiated the chain.
    let a_log = query_ping_log(&test, &db_a_identity);
    assert!(
        a_log.contains("chain:hello-chain"),
        "A ping_log should contain 'chain:hello-chain', got:\n{a_log}"
    );
}
