use spacetimedb_smoketests::Smoketest;

/// Module code for the 2PC test.
///
/// All three databases (A = coordinator, B and C = participants) use the same module.
///
/// Tables:
/// - `Ledger(account: String PK, balance: i64)` -- stores account balances.
///
/// Reducers:
/// - `init`: seeds "alice" with balance 100.
/// - `balance(account) -> i64`: returns the current balance for an account.
/// - `debit(account, amount)`: decrements balance, panics if insufficient funds.
/// - `credit(account, amount)`: increments balance (or inserts if absent).
/// - `transfer_funds(b_hex, c_hex, from_account, to_account, amount) -> TransferResult`:
///     Credits `amount * 2` to `to_account` locally (collecting `amount` from each of B and C),
///     then calls `debit(from_account, amount)` on both B and C via `call_reducer_on_db_2pc`.
///     If either remote debit fails, all three databases are rolled back atomically.
///     On success, returns the new local balance so the caller can verify without a second query.
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

/// Transfer `amount` from `from_account` on both B and C to `to_account` on A (locally).
///
/// Returns the new local balance of `to_account` so the caller can verify correctness
/// without issuing a separate query.
///
/// If either remote debit fails (insufficient funds), returns Err and the 2PC protocol
/// rolls back all three databases atomically.
#[spacetimedb::reducer]
pub fn transfer_funds(ctx: &ReducerContext, b_hex: String, c_hex: String, from_account: String, to_account: String, amount: i64) -> Result<i64, String> {
    credit(ctx, to_account.clone(), amount * 2);

    let b = Identity::from_hex(&b_hex).map_err(|e| format!("invalid B identity: {e}"))?;
    let args_b = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(from_account.clone(), amount)).map_err(|e| format!("failed to encode args: {e}"))?;
    spacetimedb::remote_reducer::call_reducer_on_db_2pc(b, "debit", &args_b)
        .map_err(|e| format!("debit on B failed: {e}"))?;
    log::info!("transfer_funds: debit on B succeeded");

    let c = Identity::from_hex(&c_hex).map_err(|e| format!("invalid C identity: {e}"))?;
    let args_c = spacetimedb::spacetimedb_lib::bsatn::to_vec(&(from_account, amount)).map_err(|e| format!("failed to encode args: {e}"))?;
    spacetimedb::remote_reducer::call_reducer_on_db_2pc(c, "debit", &args_c)
        .map_err(|e| format!("debit on C failed: {e}"))?;
    log::info!("transfer_funds: debit on C succeeded");

    // Return new local balance so the caller can assert correctness immediately.
    ctx.db.ledger().account().find(&to_account)
        .map(|r| r.balance)
        .ok_or_else(|| format!("account '{}' not found after credit", to_account))
}
"#;

/// Call `balance(account)` on `db_identity` via the HTTP API and return the i64 result.
fn call_balance(test: &Smoketest, db_identity: &str, account: &str) -> i64 {
    let resp = test
        .api_call_json(
            "POST",
            &format!("/v1/database/{db_identity}/call/balance"),
            &format!("[\"{account}\"]"),
        )
        .unwrap_or_else(|e| panic!("balance call failed for {db_identity}: {e}"));
    assert!(resp.is_success(), "balance reducer returned {}", resp.status_code);
    resp.json()
        .unwrap_or_else(|e| panic!("failed to parse balance JSON: {e}"))
        .as_i64()
        .unwrap_or_else(|| panic!("balance JSON was not an integer"))
}

/// Happy path: transfer 30 from both B's alice and C's alice to A's alice.
///
/// The coordinator reducer returns the new local balance (160), which is used directly
/// to assert A's result.  B and C balances are verified via `balance` reducer calls.
///
/// Expected: A=160, B=70, C=70.
#[test]
fn test_cross_db_2pc_happy_path() {
    let pid = std::process::id();
    let db_a_name = format!("2pc-bank-a-{pid}");
    let db_b_name = format!("2pc-bank-b-{pid}");
    let db_c_name = format!("2pc-bank-c-{pid}");

    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    // Publish participants first, then coordinator.
    test.publish_module_named(&db_b_name, false)
        .expect("failed to publish bank B");
    let db_b_identity = test.database_identity.clone().expect("bank B identity not set");

    test.publish_module_named(&db_c_name, false)
        .expect("failed to publish bank C");
    let db_c_identity = test.database_identity.clone().expect("bank C identity not set");

    test.publish_module_named(&db_a_name, false)
        .expect("failed to publish bank A");
    let db_a_identity = test.database_identity.clone().expect("bank A identity not set");

    // Call transfer_funds; the return value is A's new alice balance.
    let resp = test
        .api_call_json(
            "POST",
            &format!("/v1/database/{db_a_identity}/call/transfer_funds"),
            &format!("[\"{db_b_identity}\", \"{db_c_identity}\", \"alice\", \"alice\", 30]"),
        )
        .expect("transfer_funds call failed");
    assert!(resp.is_success(), "transfer_funds failed: {}", resp.status_code);
    let new_a_balance = resp.json().expect("invalid JSON").as_i64().expect("not i64");
    assert_eq!(new_a_balance, 160, "transfer_funds return value: expected A alice=160");

    // Verify B and C via balance reducer.
    assert_eq!(call_balance(&test, &db_b_identity, "alice"), 70, "B alice should be 70");
    assert_eq!(call_balance(&test, &db_c_identity, "alice"), 70, "C alice should be 70");
}

/// Abort path: try to transfer 110 from B and C, but both only have 100.
///
/// B's debit fails (insufficient funds), so the coordinator reducer panics and the
/// 2PC protocol rolls back all three databases.  We verify via `balance` reducer calls
/// that every account is still at 100.
///
/// Expected: A=100, B=100, C=100.
#[test]
fn test_cross_db_2pc_abort_insufficient_funds() {
    let pid = std::process::id();
    let db_a_name = format!("2pc-abort-a-{pid}");
    let db_b_name = format!("2pc-abort-b-{pid}");
    let db_c_name = format!("2pc-abort-c-{pid}");

    let mut test = Smoketest::builder().module_code(MODULE_CODE).autopublish(false).build();

    test.publish_module_named(&db_b_name, false)
        .expect("failed to publish bank B");
    let db_b_identity = test.database_identity.clone().expect("bank B identity not set");

    test.publish_module_named(&db_c_name, false)
        .expect("failed to publish bank C");
    let db_c_identity = test.database_identity.clone().expect("bank C identity not set");

    test.publish_module_named(&db_a_name, false)
        .expect("failed to publish bank A");
    let db_a_identity = test.database_identity.clone().expect("bank A identity not set");

    // Transfer 110 from each — both only have 100, so B's debit panics → 2PC aborts all.
    let resp = test
        .api_call_json(
            "POST",
            &format!("/v1/database/{db_a_identity}/call/transfer_funds"),
            &format!("[\"{db_b_identity}\", \"{db_c_identity}\", \"alice\", \"alice\", 110]"),
        )
        .expect("api_call failed");
    assert!(
        !resp.is_success(),
        "Expected transfer_funds to fail due to insufficient funds"
    );

    // All three accounts must still be at 100.
    assert_eq!(
        call_balance(&test, &db_a_identity, "alice"),
        100,
        "A alice should still be 100"
    );
    assert_eq!(
        call_balance(&test, &db_b_identity, "alice"),
        100,
        "B alice should still be 100"
    );
    assert_eq!(
        call_balance(&test, &db_c_identity, "alice"),
        100,
        "C alice should still be 100"
    );
}
