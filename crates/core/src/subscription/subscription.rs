use super::execution_unit::QueryHash;
use super::module_subscription_manager::Plan;
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::sql::ast::SchemaViewer;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_expr::expr::{BindEnv, ParamId};
use spacetimedb_lib::db::auth::StTableType;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_subscription::SubscriptionPlan;
use std::sync::Arc;

/// Queries all visible user tables right now and turns them into subscription plans.
pub(crate) fn get_all<F, I>(
    get_all_tables: F,
    relational_db: &RelationalDB,
    tx: &mut MutTxId,
    auth: &AuthCtx,
) -> Result<Vec<Plan>, DBError>
where
    F: Fn(&RelationalDB, &MutTxId) -> Result<I, DBError>,
    I: Iterator<Item = Arc<TableSchema>>,
{
    let schemas = get_all_tables(relational_db, tx)?
        .filter(|t| t.table_type == StTableType::User && auth.has_read_access(t.table_access) && !t.is_event)
        .collect::<Vec<_>>();
    let mut all = Vec::with_capacity(schemas.len());
    for schema in schemas {
        let sql = format!("SELECT * FROM {}", schema.table_name);
        let schema_tx = SchemaViewer::new(&*tx, auth);
        let (plans, requires_sender_binding) = SubscriptionPlan::compile(&sql, &schema_tx, auth)?;
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
        all.push(Plan::new(
            plans,
            QueryHash::from_string(&sql, auth.caller(), auth.bypass_rls() || requires_sender_binding),
            sql,
            bind_env,
        ));
    }
    Ok(all)
}
