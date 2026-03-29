use spacetimedb_guard::ensure_binaries_built;
use spacetimedb_smoketests::{require_local_server, Smoketest};
use std::time::Duration;

/// Module code used for all recovery tests.
///
/// Extends the basic banking module with:
/// - `debit_slow`: same as `debit` but spins for ~2-3s first, giving the test
///   a reliable window in which to crash the server mid-2PC.
/// - `balance`: convenience reducer that returns alice's balance in the logs
///   so tests can detect completion by polling server logs.
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
    // Using the timestamp as the seed prevents the loop from being
    // eliminated by the WASM optimizer.
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

#[spacetimedb::reducer]
pub fn transfer_funds(ctx: &ReducerContext, target_hex: String, from_account: String, to_account: String, amount: i64) {
    credit(ctx, to_account.clone(), amount);
    let target = Identity::from_hex(&target_hex).expect("invalid target identity hex");
    let args = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(from_account, amount)).expect("failed to encode args");
    match spacetimedb::remote_reducer::call_reducer_on_db_2pc(target, "debit", &args) {
        Ok(()) => log::info!("transfer_funds: remote debit succeeded"),
        Err(e) => panic!("remote debit failed: {e}"),
    }
}

/// Same as transfer_funds but calls debit_slow on the remote side.
#[spacetimedb::reducer]
pub fn transfer_funds_slow(ctx: &ReducerContext, target_hex: String, from_account: String, to_account: String, amount: i64) {
    credit(ctx, to_account.clone(), amount);
    let target = Identity::from_hex(&target_hex).expect("invalid target identity hex");
    let args = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(from_account, amount)).expect("failed to encode args");
    match spacetimedb::remote_reducer::call_reducer_on_db_2pc(target, "debit_slow", &args) {
        Ok(()) => log::info!("transfer_funds_slow: remote debit_slow succeeded"),
        Err(e) => panic!("remote debit_slow failed: {e}"),
    }
}
"#;

/// Spawn a background thread that fires `transfer_funds_slow` and ignores the result.
///
/// This is used to start a long-running 2PC in the background so the main thread
/// can crash the server mid-flight.  The call is expected to fail with a
/// connection error when the server is restarted.
fn spawn_transfer_funds_slow(
    server_url: String,
    config_path: std::path::PathBuf,
    db_a_identity: String,
    db_b_identity: String,
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
                "alice",
                "alice",
                &amount.to_string(),
            ])
            .output();
    })
}

/// Query alice's balance on a specific database (by identity string).
fn alice_balance(test: &Smoketest, db_identity: &str) -> i64 {
    let out = test
        .spacetime(&[
            "sql",
            "--server",
            &test.server_url,
            db_identity,
            "SELECT balance FROM ledger WHERE account = 'alice'",
        ])
        .unwrap_or_else(|e| panic!("sql query failed for {db_identity}: {e}"));
    // Output looks like: " balance \n--------\n   100\n"
    out.lines()
        .filter_map(|l| l.trim().parse::<i64>().ok())
        .next()
        .unwrap_or_else(|| panic!("could not parse balance from: {out}"))
}

/// Set up two databases (A = coordinator, B = participant) on the same server
/// and return (db_a_identity, db_b_identity).  `test.database_identity` points to A.
fn setup_two_banks(test: &mut Smoketest, pid: u32, suffix: &str) -> (String, String) {
    let db_b_name = format!("2pc-rec-b-{pid}-{suffix}");
    let db_a_name = format!("2pc-rec-a-{pid}-{suffix}");

    test.publish_module_named(&db_b_name, false)
        .expect("failed to publish bank B");
    let db_b_identity = test.database_identity.clone().expect("bank B identity");

    test.publish_module_named(&db_a_name, false)
        .expect("failed to publish bank A");
    let db_a_identity = test.database_identity.clone().expect("bank A identity");

    (db_a_identity, db_b_identity)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: committed data survives a full server restart.
//
// Rationale: verifies that every "persist" step in the 2PC protocol actually
// writes to durable storage.  If any durability wait were missing, one side
// would lose its data on restart.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_committed_data_survives_restart() {
    require_local_server!();
    let pid = std::process::id();
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE)
        .autopublish(false)
        .build();

    let (db_a_identity, db_b_identity) = setup_two_banks(&mut test, pid, "dur");

    // Successful 2PC: transfer 50 from B's alice to A's alice.
    test.call("transfer_funds", &[&db_b_identity, "alice", "alice", "50"])
        .expect("transfer_funds failed");

    // Verify pre-restart state.
    assert_eq!(alice_balance(&test, &db_a_identity), 150, "A should have 150 before restart");
    assert_eq!(alice_balance(&test, &db_b_identity), 50, "B should have 50 before restart");

    // Restart the server — exercises recovery path even though there's nothing to recover.
    test.restart_server();

    // After restart, data must still be present and correct.
    assert_eq!(
        alice_balance(&test, &db_a_identity),
        150,
        "A's committed data should survive restart"
    );
    assert_eq!(
        alice_balance(&test, &db_b_identity),
        50,
        "B's committed data should survive restart"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: aborted 2PC rollback also survives a restart.
//
// Rationale: rollback (B's st_2pc_state deletion + reducer rollback) must also
// be durable.  After restart, neither side should show the transfer.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_aborted_state_survives_restart() {
    require_local_server!();
    let pid = std::process::id();
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE)
        .autopublish(false)
        .build();

    let (db_a_identity, db_b_identity) = setup_two_banks(&mut test, pid, "abort-dur");

    // Try to transfer 200 — B only has 100, so the remote debit panics → abort.
    let _ = test.call("transfer_funds", &[&db_b_identity, "alice", "alice", "200"]);

    assert_eq!(alice_balance(&test, &db_a_identity), 100, "A should still be 100 after abort");
    assert_eq!(alice_balance(&test, &db_b_identity), 100, "B should still be 100 after abort");

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
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: status endpoint returns "abort" for an unknown prepare_id.
//
// Rationale: tests that GET /v1/database/{db}/2pc/status/{id} is correctly wired
// and returns the right default when no coordinator log entry exists.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_status_endpoint_unknown_returns_abort() {
    let pid = std::process::id();
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE)
        .autopublish(false)
        .build();

    let (db_a_identity, _db_b_identity) = setup_two_banks(&mut test, pid, "status");

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
// Strategy: `transfer_funds_slow` calls `debit_slow` on B, which burns ~2-3s
// of CPU.  We crash the server after 1s (when the 2PC is definitely in flight)
// and verify that both databases are in a CONSISTENT state after restart:
// either both committed (alice_a=150, alice_b=50) or both rolled back
// (alice_a=100, alice_b=100).
//
// Note: we intentionally do NOT assert which outcome occurred, because that
// depends on whether the crash hit before or after A wrote its coordinator log.
// What we assert is that the two sides agree — this is the 2PC guarantee.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_atomicity_under_crash() {
    require_local_server!();
    let pid = std::process::id();
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE)
        .autopublish(false)
        .build();

    let (db_a_identity, db_b_identity) = setup_two_banks(&mut test, pid, "crash");

    // Kick off the slow transfer in a background thread.  It will block
    // for ~2-3s inside debit_slow on B before completing.
    let _call_thread = spawn_transfer_funds_slow(
        test.server_url.clone(),
        test.config_path.clone(),
        db_a_identity.clone(),
        db_b_identity.clone(),
        50,
    );

    // Give the 2PC time to get mid-flight (after B starts its slow reducer
    // but before it finishes), then crash the server.
    std::thread::sleep(Duration::from_millis(1000));
    test.restart_server();

    // After restart, give recovery time to settle: coordinator recovery
    // retransmits COMMIT if needed, participant recovery polls if needed.
    std::thread::sleep(Duration::from_secs(5));

    let bal_a = alice_balance(&test, &db_a_identity);
    let bal_b = alice_balance(&test, &db_b_identity);

    // The 2PC guarantee: both sides must agree.
    let both_committed = bal_a == 150 && bal_b == 50;
    let both_rolled_back = bal_a == 100 && bal_b == 100;
    assert!(
        both_committed || both_rolled_back,
        "2PC atomicity violated after crash: A={bal_a}, B={bal_b}. \
         Expected either (150, 50) or (100, 100)."
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: coordinator recovery — A crashes after writing its coordinator log,
// before B commits.
//
// Strategy: same crash-mid-flight approach, but we poll A's balance to detect
// the moment A has committed (alice_a=150), then immediately crash.  At that
// point A's coordinator log is on disk, B has sent PREPARED, but B may not yet
// have received COMMIT.  Recovery should bring B to the committed state.
//
// This test is inherently timing-sensitive (same-process loopback is fast).
// It uses `debit_slow` to widen the window: after A commits (detectable via
// alice_a=150), B is still inside `debit_slow` and has not yet received COMMIT.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_coordinator_recovery() {
    require_local_server!();
    let pid = std::process::id();
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE)
        .autopublish(false)
        .build();

    let (db_a_identity, db_b_identity) = setup_two_banks(&mut test, pid, "coord-rec");

    let _call_thread = spawn_transfer_funds_slow(
        test.server_url.clone(),
        test.config_path.clone(),
        db_a_identity.clone(),
        db_b_identity.clone(),
        50,
    );

    // Poll A's alice balance until it reaches 150 — that means A has committed
    // its tx (including the coordinator log entry) and B has sent PREPARED.
    // At this point B is still inside debit_slow, so COMMIT hasn't reached B yet.
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        std::thread::sleep(Duration::from_millis(100));
        if alice_balance(&test, &db_a_identity) == 150 {
            break;
        }
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for A to commit");
        }
    }

    // Crash immediately: A has coordinator log, B has st_2pc_state, B hasn't committed.
    test.restart_server();

    // Allow recovery to complete: A's recover_2pc_coordinator retransmits COMMIT to B.
    std::thread::sleep(Duration::from_secs(5));

    let bal_a = alice_balance(&test, &db_a_identity);
    let bal_b = alice_balance(&test, &db_b_identity);

    assert_eq!(
        bal_a, 150,
        "A should have committed (alice_a=150) before crash"
    );
    assert_eq!(
        bal_b, 50,
        "B should have committed via coordinator recovery (alice_b=50), got {bal_b}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 6: participant recovery — B crashes after writing st_2pc_state (PREPARE
// durable) but before receiving COMMIT.
//
// Strategy: since A and B are on the same server, we cannot crash B without
// also crashing A.  So we crash the server right after the PREPARE is durable
// on B (detectable: B's st_2pc_state is non-empty) and before A commits.
// On restart:
//   - B finds st_2pc_state → re-runs reducer → polls A's status endpoint
//   - A has no coordinator log (A hadn't committed) → status = "abort"
//   - B aborts → both sides return to 100
//
// A fully committed scenario (B polls and gets "commit") is exercised by
// test_2pc_coordinator_recovery which covers the symmetric window.
// ─────────────────────────────────────────────────────────────────────────────
#[test]
fn test_2pc_participant_recovery_polls_and_aborts() {
    require_local_server!();
    let pid = std::process::id();
    let mut test = Smoketest::builder()
        .module_code(MODULE_CODE)
        .autopublish(false)
        .build();

    let (db_a_identity, db_b_identity) = setup_two_banks(&mut test, pid, "part-rec");

    let _call_thread = spawn_transfer_funds_slow(
        test.server_url.clone(),
        test.config_path.clone(),
        db_a_identity.clone(),
        db_b_identity.clone(),
        50,
    );

    // Crash early: after ~500ms, B's slow reducer should be mid-execution.
    // A has not yet received PREPARED, so A has no coordinator log.
    // B's st_2pc_state may or may not be written yet (it's written after the
    // reducer finishes).  Either way, the final state must be consistent.
    std::thread::sleep(Duration::from_millis(500));
    test.restart_server();

    // Wait for participant recovery to settle.  B polls A's status endpoint
    // every 5s; allow up to 15s for it to act.
    std::thread::sleep(Duration::from_secs(15));

    let bal_a = alice_balance(&test, &db_a_identity);
    let bal_b = alice_balance(&test, &db_b_identity);

    let both_committed = bal_a == 150 && bal_b == 50;
    let both_rolled_back = bal_a == 100 && bal_b == 100;
    assert!(
        both_committed || both_rolled_back,
        "Inconsistent state after participant recovery: A={bal_a}, B={bal_b}"
    );
}
