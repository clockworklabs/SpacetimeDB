use super::execution_unit::QueryHash;
use super::module_subscription_manager::Plan;
use crate::db::relational_db::RelationalDB;
use crate::error::DBError;
use crate::sql::ast::SchemaViewer;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_lib::db::auth::StTableType;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_subscription::SubscriptionPlan;
use std::sync::Arc;

/// Queries all visible user tables right now and turns them into subscription plans.
pub(crate) fn get_all<T, F, I>(
    get_all_tables: F,
    relational_db: &RelationalDB,
    tx: &T,
    auth: &AuthCtx,
) -> Result<Vec<Plan>, DBError>
where
    T: StateView,
    F: Fn(&RelationalDB, &T) -> Result<I, DBError>,
    I: Iterator<Item = Arc<TableSchema>>,
{
    Ok(get_all_tables(relational_db, tx)?
        .filter(|t| t.table_type == StTableType::User && auth.has_read_access(t.table_access) && !t.is_event)
        .map(|schema| {
            let sql = format!("SELECT * FROM {}", schema.table_name);
            let tx = SchemaViewer::new(tx, auth);
            SubscriptionPlan::compile(&sql, &tx, auth).map(|(plans, has_param)| {
                Plan::new(
                    plans,
                    QueryHash::from_string(&sql, auth.caller(), auth.bypass_rls() || has_param),
                    sql,
                )
            })
        })
        .collect::<Result<_, _>>()?)
}
