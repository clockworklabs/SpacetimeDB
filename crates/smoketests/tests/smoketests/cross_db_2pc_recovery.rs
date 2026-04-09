use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::{require_local_server, Smoketest};
use std::time::Duration;

/// Module code used for all recovery tests.
///
/// All three databases (A = coordinator, B and C = participants) use the same module.
///
/// `transfer_funds_slow` calls `debit_slow` on B and regular `debit` on C, creating
/// a reliable ~2-3s window while B's slow reducer is executing — useful for crash tests.
const MODULE_CODE: &str = r#"
use spacetimedb::{log, ReducerContext, Table, Identity};

#[spacetimedb::table(accessor = ledger, public)]
pub struct Ledger {
    #[primary_key]
    account: String,
    balance: i64,
}

#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.ledger().insert(Ledger { account: "alice".to_string(), balance: 100 });
}

/// Returns the current balance for `account`.
#[spacetimedb::reducer]
pub fn balance(ctx: &ReducerContext, account: String) -> Result<i64, String> {
    ctx.db.ledger().account().find(&account)
        .map(|r| r.balance)
        .ok_or_else(|| format!("account '{}' not found", account))
}

#[spacetimedb::reducer]
pub fn debit(ctx: &ReducerContext, account: String, amount: i64) {
    let row = ctx.db.ledger().account().find(&account)
        .unwrap_or_else(|| panic!("account '{}' not found", account));
    let new_balance = row.balance - amount;
    if new_balance < 0 {
        panic!("insufficient funds: account '{}' has {} but tried to debit {}", account, row.balance, amount);
    }
    ctx.db.ledger().account().update(Ledger { account, balance: new_balance });
}

/// Same as `debit` but wastes ~2-3 seconds of CPU first.
/// This creates a reliable timing window for crash recovery tests:
/// the server can be killed while this reducer is executing or just after.
#[spacetimedb::reducer]
pub fn debit_slow(ctx: &ReducerContext, account: String, amount: i64) {
    // Busy-wait loop.  ~100M multiply-add iterations ≈ 2-3s in WASM.
    let mut x: u64 = ctx.timestamp.to_micros_since_unix_epoch() as u64;
    for i in 0u64..100_000_000 {
        x = x.wrapping_mul(6364136223846793005u64).wrapping_add(i | 1);
    }
    if x == 0 { panic!("impossible: loop result was zero"); }
    debit(ctx, account, amount);
}

#[spacetimedb::reducer]
pub fn credit(ctx: &ReducerContext, account: String, amount: i64) {
    match ctx.db.ledger().account().find(&account) {
        Some(row) => {
            ctx.db.ledger().account().update(Ledger { account, balance: row.balance + amount });
        }
        None => {
            ctx.db.ledger().insert(Ledger { account, balance: amount });
        }
    }
}

/// Transfer `amount` from `from_account` on both B and C to `to_account` on A.
/// A credits `amount * 2` locally, then calls `debit(from_account, amount)` on each
/// of B and C via 2PC.  If either fails, all three roll back atomically.
#[spacetimedb::reducer]
pub fn transfer_funds(ctx: &ReducerContext, b_hex: String, c_hex: String, from_account: String, to_account: String, amount: i64) {
    credit(ctx, to_account.clone(), amount * 2);

    let b = Identity::from_hex(&b_hex).expect("invalid B identity");
    let args_b = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(from_account.clone(), amount)).expect("failed to encode args");
    match spacetimedb::remote_reducer::call_reducer_on_db_2pc(b, "debit", &args_b) {
        Ok(()) => log::info!("transfer_funds: debit on B succeeded"),
        Err(e) => panic!("debit on B failed: {e}"),
    }

    let c = Identity::from_hex(&c_hex).expect("invalid C identity");
    let args_c = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(from_account, amount)).expect("failed to encode args");
    match spacetimedb::remote_reducer::call_reducer_on_db_2pc(c, "debit", &args_c) {
        Ok(()) => log::info!("transfer_funds: debit on C succeeded"),
        Err(e) => panic!("debit on C failed: {e}"),
    }
}

/// Same as `transfer_funds` but calls `debit_slow` on B and regular `debit` on C.
/// The slow call on B creates a ~2-3s window for crash recovery tests.
#[spacetimedb::reducer]
pub fn transfer_funds_slow(ctx: &ReducerContext, b_hex: String, c_hex: String, from_account: String, to_account: String, amount: i64) {
    credit(ctx, to_account.clone(), amount * 2);

    let b = Identity::from_hex(&b_hex).expect("invalid B identity");
    let args_b = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(from_account.clone(), amount)).expect("failed to encode args");
    match spacetimedb::remote_reducer::call_reducer_on_db_2pc(b, "debit_slow", &args_b) {
        Ok(()) => log::info!("transfer_funds_slow: debit_slow on B succeeded"),
        Err(e) => panic!("debit_slow on B failed: {e}"),
    }

    let c = Identity::from_hex(&c_hex).expect("invalid C identity");
    let args_c = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(from_account, amount)).expect("failed to encode args");
    match spacetimedb::remote_reducer::call_reducer_on_db_2pc(c, "debit", &args_c) {
        Ok(()) => log::info!("transfer_funds_slow: debit on C succeeded"),
        Err(e) => panic!("debit on C failed: {e}"),
    }
}
"#;

/// Spawn a background thread that fires `transfer_funds_slow` and ignores the result.
///
/// Used to start a long-running 2PC in the background so the main thread can crash
/// the server mid-flight.  The call is expected to fail with a connection error when
/// the server is restarted.
fn spawn_transfer_funds_slow(
    server_url: String,
    config_path: std::path::PathBuf,
    db_a_identity: String,
    db_b_identity: String,
    db_c_identity: String,
    amount: i64,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let cli = ensure_binaries_built();
        let _ = std::process::Command::new(&cli)
            .arg("--config-path")
            .arg(&config_path)
            .args([
                "call",
                "--server",
                &server_url,
                "--",
                &db_a_identity,
                "transfer_funds_slow",
                &db_b_identity,
                &db_c_identity,
                "alice",
                "alice",
                &amount.to_string(),
            ])
            .output();
    })
}

/// Call the `balance(account)` reducer on `db_identity` and return the i64 result.
fn alice_balance(test: &Smoketest, db_identity: &str) -> i64 {
    let resp = test
        .api_call_json(
            "POST",
            &format!("/v1/database/{db_identity}/call/balance"),
            "[\"alice\"]",
        )
        .unwrap_or_else(|e| panic!("balance call failed for {db_identity}: {e}"));
    assert!(resp.is_success(), "balance reducer returned {}", resp.status_code);
    resp.json()
        .unwrap_or_else(|e| panic!("failed to parse balance JSON: {e}"))
        .as_i64()
        .unwrap_or_else(|| panic!("balance JSON was not an integer"))
}

/// Set up three databases (A = coordinator, B and C = participants) on the same server.
/// Returns `(db_a_identity, db_b_identity, db_c_identity)`.  `test.database_identity` points to A.
fn setup_three_banks(test: &mut Smoketest, pid: u32, suffix: &str) -> (String, String, String) {
    let db_b_name = format!("2pc-rec-b-{pid}-{suffix}");
    let db_c_name = format!("2pc-rec-c-{pid}-{suffix}");
    let db_a_name = format!("2pc-rec-a-{pid}-{suffix}");

    test.publish_module_named(&db_b_name, false)
        .expect("failed to publish bank B");
    let db_b_identity = test.database_identity.clone().expect("bank B identity");

    test.publish_module_named(&db_c_name, false)
        .expect("failed to publish bank C");
    let db_c_identity = test.database_identity.clone().expect("bank C identity");

    test.publish_module_named(&db_a_name, false)
        .expect("failed to publish bank A");
    let db_a_identity = test.database_identity.clone().expect("bank A identity");

    (db_a_identity, db_b_identity, db_c_identity)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: committed data survives a full server restart.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_committed_data_survives_restart() {
    require_local_server!();
    let pid = std::process::id();
    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    let (db_a_identity, db_b_identity, db_c_identity) = setup_three_banks(&mut test, pid, "dur");

    // Successful 2PC: transfer 30 from both B and C to A.
    test.call(
        "transfer_funds",
        &[&db_b_identity, &db_c_identity, "alice", "alice", "30"],
    )
    .expect("transfer_funds failed");

    assert_eq!(
        alice_balance(&test, &db_a_identity),
        160,
        "A should have 160 before restart"
    );
    assert_eq!(
        alice_balance(&test, &db_b_identity),
        70,
        "B should have 70 before restart"
    );
    assert_eq!(
        alice_balance(&test, &db_c_identity),
        70,
        "C should have 70 before restart"
    );

    test.restart_server();

    assert_eq!(
        alice_balance(&test, &db_a_identity),
        160,
        "A's committed data should survive restart"
    );
    assert_eq!(
        alice_balance(&test, &db_b_identity),
        70,
        "B's committed data should survive restart"
    );
    assert_eq!(
        alice_balance(&test, &db_c_identity),
        70,
        "C's committed data should survive restart"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: aborted 2PC rollback also survives a restart.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_aborted_state_survives_restart() {
    require_local_server!();
    let pid = std::process::id();
    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    let (db_a_identity, db_b_identity, db_c_identity) = setup_three_banks(&mut test, pid, "abort-dur");

    // Transfer 110 from each — both only have 100, so B's debit panics → abort.
    let _ = test.call(
        "transfer_funds",
        &[&db_b_identity, &db_c_identity, "alice", "alice", "110"],
    );

    assert_eq!(
        alice_balance(&test, &db_a_identity),
        100,
        "A should still be 100 after abort"
    );
    assert_eq!(
        alice_balance(&test, &db_b_identity),
        100,
        "B should still be 100 after abort"
    );
    assert_eq!(
        alice_balance(&test, &db_c_identity),
        100,
        "C should still be 100 after abort"
    );

    test.restart_server();

    assert_eq!(
        alice_balance(&test, &db_a_identity),
        100,
        "A's aborted rollback should survive restart"
    );
    assert_eq!(
        alice_balance(&test, &db_b_identity),
        100,
        "B's aborted rollback should survive restart"
    );
    assert_eq!(
        alice_balance(&test, &db_c_identity),
        100,
        "C's aborted rollback should survive restart"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: status endpoint returns "abort" for an unknown prepare_id.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_status_endpoint_unknown_returns_abort() {
    let pid = std::process::id();
    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    let (db_a_identity, _db_b_identity, _db_c_identity) = setup_three_banks(&mut test, pid, "status");

    let resp = test
        .api_call(
            "GET",
            &format!("/v1/database/{db_a_identity}/2pc/status/nonexistent-prepare-id"),
        )
        .expect("api_call failed");

    assert_eq!(resp.status_code, 200, "status endpoint should return 200");
    let body_text = resp.text().expect("response body is not UTF-8");
    assert_eq!(
        body_text.trim(),
        "abort",
        "unknown prepare_id should return 'abort', got: {:?}",
        body_text
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4: 2PC atomicity is maintained when the server crashes mid-flight.
//
// `transfer_funds_slow` calls `debit_slow` on B (~2-3s) then `debit` on C.
// We crash after 1s (B is definitely mid-execution).  After restart, all three
// databases must agree: either all committed (A=160, B=70, C=70) or all rolled
// back (A=100, B=100, C=100).
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_atomicity_under_crash() {
    require_local_server!();
    let pid = std::process::id();
    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    let (db_a_identity, db_b_identity, db_c_identity) = setup_three_banks(&mut test, pid, "crash");

    let _call_thread = spawn_transfer_funds_slow(
        test.server_url.clone(),
        test.config_path.clone(),
        db_a_identity.clone(),
        db_b_identity.clone(),
        db_c_identity.clone(),
        30,
    );

    std::thread::sleep(Duration::from_millis(1000));
    test.restart_server();

    std::thread::sleep(Duration::from_secs(5));

    let bal_a = alice_balance(&test, &db_a_identity);
    let bal_b = alice_balance(&test, &db_b_identity);
    let bal_c = alice_balance(&test, &db_c_identity);

    let both_committed = bal_a == 160 && bal_b == 70 && bal_c == 70;
    let both_rolled_back = bal_a == 100 && bal_b == 100 && bal_c == 100;
    assert!(
        both_committed || both_rolled_back,
        "2PC atomicity violated after crash: A={bal_a}, B={bal_b}, C={bal_c}. \
         Expected either (160, 70, 70) or (100, 100, 100)."
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: coordinator recovery — A crashes after writing its coordinator log,
// before B and C commit.
//
// `transfer_funds_slow` calls `debit_slow` on B (~2-3s) then `debit` on C.
// We poll until A=160 (A committed, coordinator log written for both B and C),
// then crash.  At this point B is still inside `debit_slow` awaiting COMMIT.
// Recovery must bring all three to the committed state: A=160, B=70, C=70.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_coordinator_recovery() {
    require_local_server!();
    let pid = std::process::id();
    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    let (db_a_identity, db_b_identity, db_c_identity) = setup_three_banks(&mut test, pid, "coord-rec");

    let _call_thread = spawn_transfer_funds_slow(
        test.server_url.clone(),
        test.config_path.clone(),
        db_a_identity.clone(),
        db_b_identity.clone(),
        db_c_identity.clone(),
        30,
    );

    // Wait until A has committed (alice_a=160), meaning both B and C have sent PREPARED
    // and A's coordinator log entries for both are on disk.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        std::thread::sleep(Duration::from_millis(100));
        if alice_balance(&test, &db_a_identity) == 160 {
            break;
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for A to commit");
        }
    }

    // Crash: A has coordinator log for both B and C; B is waiting in decision_rx for COMMIT.
    test.restart_server();

    // Allow recovery to complete.
    std::thread::sleep(Duration::from_secs(5));

    assert_eq!(alice_balance(&test, &db_a_identity), 160, "A should remain committed");
    assert_eq!(
        alice_balance(&test, &db_b_identity),
        70,
        "B should have committed via coordinator recovery"
    );
    assert_eq!(
        alice_balance(&test, &db_c_identity),
        70,
        "C should have committed via coordinator recovery"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 6: participant recovery — crash before A commits.
//
// We crash early (~500ms into the slow debit on B).  A has not yet received
// PREPARED from B, so A has no coordinator log.  After restart B (and possibly C)
// recover by polling A's status endpoint, which returns "abort".  Both sides
// must end up consistent.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_participant_recovery_polls_and_aborts() {
    require_local_server!();
    let pid = std::process::id();
    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    let (db_a_identity, db_b_identity, db_c_identity) = setup_three_banks(&mut test, pid, "part-rec");

    let _call_thread = spawn_transfer_funds_slow(
        test.server_url.clone(),
        test.config_path.clone(),
        db_a_identity.clone(),
        db_b_identity.clone(),
        db_c_identity.clone(),
        30,
    );

    // Crash early: B's slow reducer is mid-execution, A has no coordinator log yet.
    std::thread::sleep(Duration::from_millis(500));
    test.restart_server();

    // Allow participant recovery to settle (polls status every 5s).
    std::thread::sleep(Duration::from_secs(15));

    let bal_a = alice_balance(&test, &db_a_identity);
    let bal_b = alice_balance(&test, &db_b_identity);
    let bal_c = alice_balance(&test, &db_c_identity);

    let both_committed = bal_a == 160 && bal_b == 70 && bal_c == 70;
    let both_rolled_back = bal_a == 100 && bal_b == 100 && bal_c == 100;
    assert!(
        both_committed || both_rolled_back,
        "Inconsistent state after participant recovery: A={bal_a}, B={bal_b}, C={bal_c}"
    );
}
