use super::execution_unit::QueryHash;
use super::module_subscription_manager::Plan;
use crate::db::relational_db::Tx;
use crate::error::{DBError, SubscriptionError};
use crate::sql::ast::SchemaViewer;
use once_cell::sync::Lazy;
use regex::Regex;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_execution::Datastore;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_subscription::SubscriptionPlan;

static WHITESPACE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*$").unwrap());
static SUBSCRIBE_TO_ALL_TABLES_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(?i)\bSELECT\s+\*\s+FROM\s+\*\s*$").unwrap());

/// Is this string all whitespace?
pub fn is_whitespace_or_empty(sql: &str) -> bool {
    WHITESPACE.is_match_at(sql, 0)
}

/// Is this a `SELECT * FROM *` query?
pub fn is_subscribe_to_all_tables(sql: &str) -> bool {
    SUBSCRIBE_TO_ALL_TABLES_REGEX.is_match_at(sql, 0)
}

/// Compile a string into a single read-only query.
pub fn compile_read_only_query(auth: &AuthCtx, tx: &Tx, input: &str) -> Result<Plan, DBError> {
    if is_whitespace_or_empty(input) {
        return Err(SubscriptionError::Empty.into());
    }

    let tx = SchemaViewer::new(tx, auth);
    let (plans, has_param) = SubscriptionPlan::compile(input, &tx, auth)?;
    let hash = QueryHash::from_string(input, auth.caller(), has_param);
    Ok(Plan::new(plans, hash, input.to_owned()))
}

/// Compile a string into a single read-only query with externally-computed hashes.
pub fn compile_query_with_hashes<Tx: Datastore + StateView>(
    auth: &AuthCtx,
    tx: &Tx,
    input: &str,
    hash: QueryHash,
    hash_with_param: QueryHash,
) -> Result<Plan, DBError> {
    if is_whitespace_or_empty(input) {
        return Err(SubscriptionError::Empty.into());
    }

    let tx = SchemaViewer::new(tx, auth);
    let (plans, has_param) = SubscriptionPlan::compile(input, &tx, auth)?;

    if auth.bypass_rls() || has_param {
        return Ok(Plan::new(plans, hash_with_param, input.to_owned()));
    }
    Ok(Plan::new(plans, hash, input.to_owned()))
}
