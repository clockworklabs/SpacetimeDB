use spacetimedb_smoketests::Smoketest;

/// Module code used for both the "receiver" and "caller" databases.
///
/// - `record_ping(payload)` is called by the caller via `call_reducer_on_db` and stores the
///   payload fields in `ping_log`.
/// - `call_remote(target, payload)` is the entry point: it BSATN-encodes `payload` and invokes
///   `record_ping` on `target` over the cross-DB ABI.
const MODULE_CODE: &str = r#"
use spacetimedb::{log, ReducerContext, Table, Identity, SpacetimeType};

/// A structured ping payload — used to exercise BSATN encoding of a multi-field struct.
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

/// Writes one row to `ping_log` from the payload. Called via the cross-DB ABI.
#[spacetimedb::reducer]
pub fn record_ping(ctx: &ReducerContext, payload: PingPayload) {
    log::info!("record_ping: got message={} priority={}", payload.message, payload.priority);
    ctx.db.ping_log().insert(PingLog { id: 0, message: payload.message, priority: payload.priority });
}

/// Calls `record_ping(payload)` on `target_hex` via the cross-database ABI.
///
/// `target_hex` is the hex-encoded identity of the target database.
/// Args are BSATN-encoded as a 1-tuple `(payload,)`.
#[spacetimedb::reducer]
pub fn call_remote(ctx: &ReducerContext, target_hex: String, message: String, priority: u32) {
    let target = Identity::from_hex(&target_hex).expect("invalid target identity hex");
    let payload = PingPayload { message, priority };
    let args = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(payload,)).expect("failed to encode args");
    match spacetimedb::remote_reducer::call_reducer_on_db(target, "record_ping", &args) {
        Ok(()) => {
            log::info!("call_remote: remote reducer succeeded");
        }
        Err(e) => {
            log::error!("call_remote: {}", e);
            panic!("call_reducer_on_db error: {e}");
        }
    }
}
"#;

/// Smoke test for the cross-database reducer call ABI.
///
/// Publishes the same module twice on one server, then calls `call_remote` on the
/// "caller" database with the "receiver" database's identity as an argument.
/// Passes a structured `PingPayload` (message + priority) to exercise multi-field
/// BSATN encoding over the cross-DB boundary.
/// Verifies that `receiver` has the expected row in `ping_log`.
#[test]
fn test_cross_db_reducer_call() {
    let pid = std::process::id();
    let receiver_name = format!("cross-db-receiver-{pid}");
    let caller_name = format!("cross-db-caller-{pid}");

    // Build one server with the shared module code.
    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    // Publish the receiver database first.
    test.publish_module_named(&receiver_name, false)
        .expect("failed to publish receiver module");
    let receiver_identity = test
        .database_identity
        .clone()
        .expect("receiver database_identity not set after publish");

    // Publish the caller database (same code, different name).
    test.publish_module_named(&caller_name, false)
        .expect("failed to publish caller module");
    // test.database_identity is now caller_name — calls/sql default to caller.

    // Invoke call_remote on the caller, passing the receiver's identity, message, and priority.
    test.call("call_remote", &[&receiver_identity, "hello from caller", "42"])
        .expect("call_remote failed");

    // Verify that the receiver's ping_log has the expected row.
    let result = test
        .spacetime(&[
            "sql",
            "--server",
            &test.server_url,
            &receiver_identity,
            "SELECT message, priority FROM ping_log",
        ])
        .expect("sql query failed");

    assert!(
        result.contains("hello from caller"),
        "Expected ping_log to contain 'hello from caller' after cross-DB call, got:\n{result}"
    );
    assert!(
        result.contains("42"),
        "Expected ping_log to contain priority 42 after cross-DB call, got:\n{result}"
    );
}
