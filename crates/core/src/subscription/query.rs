use super::execution_unit::QueryHash;
use super::module_subscription_manager::Plan;
use crate::db::relational_db::Tx;
use crate::error::{DBError, SubscriptionError};
use crate::sql::ast::SchemaViewer;
use once_cell::sync::Lazy;
use regex::Regex;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_expr::expr::{BindEnv, ParamId};
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
    let (plans, requires_sender_binding) = SubscriptionPlan::compile(input, &tx, auth)?;
    let hash = QueryHash::from_string(input, auth.caller(), requires_sender_binding);
    let bind_env = BindEnv::for_sender_binding(requires_sender_binding, auth.caller());
    Ok(Plan::new(plans, hash, input.to_owned(), bind_env))
}

/// Compile a string into a single read-only query with externally-computed hashes.
pub fn compile_query_with_hashes(
    auth: &AuthCtx,
    tx: &mut MutTxId,
    input: &str,
    hash: QueryHash,
    hash_with_param: QueryHash,
) -> Result<Plan, DBError> {
    if is_whitespace_or_empty(input) {
        return Err(SubscriptionError::Empty.into());
    }

    let schema_tx = SchemaViewer::new(&*tx, auth);
    let (plans, requires_sender_binding) = SubscriptionPlan::compile(input, &schema_tx, auth)?;
    let requires_sender_view_arg = plans.iter().any(|plan| plan.requires_param(ParamId::SENDER_VIEW_ARG));
    let bind_env = if requires_sender_binding {
        if requires_sender_view_arg {
            BindEnv::sender_with_view_arg(auth.caller(), tx.view_arg_for_sender(auth.caller())?)
        } else {
            BindEnv::sender(auth.caller())
        }
    } else {
        BindEnv::empty()
    };

    if auth.bypass_rls() || requires_sender_binding {
        return Ok(Plan::new(plans, hash_with_param, input.to_owned(), bind_env));
    }
    Ok(Plan::new(plans, hash, input.to_owned(), bind_env))
}
