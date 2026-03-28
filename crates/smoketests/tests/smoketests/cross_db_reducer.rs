use spacetimedb_smoketests::Smoketest;

/// Module code used for both the "receiver" and "caller" databases.
///
/// - `record_ping(message)` is called by the caller via `call_reducer_on_db` and stores the
///   message in `ping_log`.
/// - `call_remote(target, message)` is the entry point: it BSATN-encodes `message` and invokes
///   `record_ping` on `target` over the cross-DB ABI.
const MODULE_CODE: &str = r#"
use spacetimedb::{log, ReducerContext, Table, Identity};

#[spacetimedb::table(name = ping_log, public)]
pub struct PingLog {
    #[primary_key]
    #[auto_inc]
    id: u64,
    message: String,
}

/// Writes one row to `ping_log` with the given message. Called via the cross-DB ABI.
#[spacetimedb::reducer]
pub fn record_ping(ctx: &ReducerContext, message: String) {
    log::info!("record_ping: got message: {}", message);
    ctx.db.ping_log().insert(PingLog { id: 0, message });
}

/// Calls `record_ping(message)` on `target` via the cross-database ABI.
///
/// Args are BSATN-encoded as a 1-tuple `(message,)` — the same layout the host-side
/// `invoke_reducer` expects when decoding a single-`String` reducer.
#[spacetimedb::reducer]
pub fn call_remote(ctx: &ReducerContext, target: Identity, message: String) {
    let args = spacetimedb::bsatn::to_vec(&(message,)).expect("failed to encode args");
    match spacetimedb::remote_reducer::call_reducer_on_db(target, "record_ping", &args) {
        Ok((status, _body)) => {
            log::info!("call_remote: got HTTP status {}", status);
        }
        Err(err) => {
            log::error!("call_remote: transport failure: {}", err);
            panic!("call_reducer_on_db transport failure: {err}");
        }
    }
}
"#;

/// Smoke test for the cross-database reducer call ABI.
///
/// Publishes the same module twice on one server, then calls `call_remote` on the
/// "caller" database with the "receiver" database's identity as an argument.
/// Verifies that `receiver` has a new row in `ping_log` written by the cross-DB call.
#[test]
fn test_cross_db_reducer_call() {
    let pid = std::process::id();
    let receiver_name = format!("cross-db-receiver-{pid}");
    let caller_name = format!("cross-db-caller-{pid}");

    // Build one server with the shared module code.
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE)
        .autopublish(false)
        .build();

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

    // Invoke call_remote on the caller, passing the receiver's identity and a test message.
    test.call("call_remote", &[&receiver_identity, "hello from caller"])
        .expect("call_remote failed");

    // Verify that the receiver's ping_log has the expected message row.
    let result = test
        .spacetime(&[
            "sql",
            "--server",
            &test.server_url,
            &receiver_identity,
            "SELECT message FROM ping_log",
        ])
        .expect("sql query failed");

    assert!(
        result.contains("hello from caller"),
        "Expected ping_log to contain 'hello from caller' after cross-DB call, got:\n{result}"
    );
}
