use spacetimedb_smoketests::Smoketest;

/// Module code for the 2PC test.
///
/// Both the "bank A" and "bank B" databases use the same module.
///
/// Tables:
/// - `Ledger(account: String PK, balance: i64)` -- stores account balances.
///
/// Reducers:
/// - `init`: seeds "alice" with balance 100.
/// - `debit(account, amount)`: decrements balance, panics if insufficient funds.
/// - `credit(account, amount)`: increments balance (or inserts if absent).
/// - `transfer_funds(target_hex, from_account, to_account, amount)`:
///     Credits `to_account` locally, then calls `debit` on the remote database
///     using `call_reducer_on_db_2pc`. If the remote debit fails (panic/insufficient funds),
///     the local credit is also rolled back by the 2PC protocol.
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

/// Transfer `amount` from `from_account` on the remote database to `to_account` locally.
///
/// Uses 2PC: credits locally first, then calls debit on the remote database via
/// `call_reducer_on_db_2pc`. If the remote debit fails, the coordinator's reducer also
/// fails, triggering abort of all participants.
#[spacetimedb::reducer]
pub fn transfer_funds(ctx: &ReducerContext, target_hex: String, from_account: String, to_account: String, amount: i64) {
    // Credit locally first.
    credit(ctx, to_account.clone(), amount);

    // Now call debit on the remote database using 2PC.
    let target = Identity::from_hex(&target_hex).expect("invalid target identity hex");
    let args = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(from_account, amount)).expect("failed to encode args");
    match spacetimedb::remote_reducer::call_reducer_on_db_2pc(target, "debit", &args) {
        Ok(()) => {
            log::info!("transfer_funds: remote debit succeeded");
        }
        Err(e) => {
            log::error!("transfer_funds: remote debit failed: {}", e);
            panic!("remote debit failed: {e}");
        }
    }
}
"#;

/// Happy path: transfer 50 from B's alice to A's alice.
/// After: A alice = 150, B alice = 50.
#[test]
fn test_cross_db_2pc_happy_path() {
    let pid = std::process::id();
    let db_a_name = format!("2pc-bank-a-{pid}");
    let db_b_name = format!("2pc-bank-b-{pid}");

    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    // Publish bank B (the participant that will be debited).
    test.publish_module_named(&db_b_name, false)
        .expect("failed to publish bank B");
    let db_b_identity = test
        .database_identity
        .clone()
        .expect("bank B identity not set");

    // Publish bank A (the coordinator that will be credited).
    test.publish_module_named(&db_a_name, false)
        .expect("failed to publish bank A");
    let _db_a_identity = test
        .database_identity
        .clone()
        .expect("bank A identity not set");

    // Transfer 50 from B's alice to A's alice.
    // The coordinator is bank A. It credits locally, then calls debit on B via 2PC.
    test.call("transfer_funds", &[&db_b_identity, "alice", "alice", "50"])
        .expect("transfer_funds failed");

    // Verify bank A: alice should have 150.
    let result_a = test
        .spacetime(&[
            "sql",
            "--server",
            &test.server_url,
            test.database_identity.as_ref().unwrap(),
            "SELECT balance FROM ledger WHERE account = 'alice'",
        ])
        .expect("sql query on bank A failed");
    assert!(
        result_a.contains("150"),
        "Expected bank A alice balance = 150, got:\n{result_a}"
    );

    // Verify bank B: alice should have 50.
    let result_b = test
        .spacetime(&[
            "sql",
            "--server",
            &test.server_url,
            &db_b_identity,
            "SELECT balance FROM ledger WHERE account = 'alice'",
        ])
        .expect("sql query on bank B failed");
    assert!(
        result_b.contains("50"),
        "Expected bank B alice balance = 50, got:\n{result_b}"
    );
}

/// Abort path: try to transfer 200, but B only has 100.
/// The remote debit should fail, causing the coordinator reducer to panic,
/// which should roll back the local credit.
/// After: both A and B should still have alice = 100.
#[test]
fn test_cross_db_2pc_abort_insufficient_funds() {
    let pid = std::process::id();
    let db_a_name = format!("2pc-abort-a-{pid}");
    let db_b_name = format!("2pc-abort-b-{pid}");

    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    // Publish bank B.
    test.publish_module_named(&db_b_name, false)
        .expect("failed to publish bank B");
    let db_b_identity = test
        .database_identity
        .clone()
        .expect("bank B identity not set");

    // Publish bank A.
    test.publish_module_named(&db_a_name, false)
        .expect("failed to publish bank A");

    // Try to transfer 200 -- B only has 100, so the remote debit will fail.
    let result = test.call("transfer_funds", &[&db_b_identity, "alice", "alice", "200"]);
    // The call should fail because the remote debit panicked.
    assert!(result.is_err(), "Expected transfer_funds to fail due to insufficient funds");

    // Verify bank A: alice should still have 100 (the local credit was rolled back).
    let result_a = test
        .spacetime(&[
            "sql",
            "--server",
            &test.server_url,
            test.database_identity.as_ref().unwrap(),
            "SELECT balance FROM ledger WHERE account = 'alice'",
        ])
        .expect("sql query on bank A failed");
    assert!(
        result_a.contains("100"),
        "Expected bank A alice balance = 100 after failed transfer, got:\n{result_a}"
    );

    // Verify bank B: alice should still have 100.
    let result_b = test
        .spacetime(&[
            "sql",
            "--server",
            &test.server_url,
            &db_b_identity,
            "SELECT balance FROM ledger WHERE account = 'alice'",
        ])
        .expect("sql query on bank B failed");
    assert!(
        result_b.contains("100"),
        "Expected bank B alice balance = 100 after failed transfer, got:\n{result_b}"
    );
}
