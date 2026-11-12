use super::execution_unit::QueryHash;
use super::query::{compile_template, CompiledQuery, TemplateCache};
use crate::db::relational_db::RelationalDB;
use crate::db::sql::ast::SchemaViewer;
use crate::error::DBError;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_lib::db::auth::StTableType;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_schema::schema::TableSchema;
use std::sync::Arc;

/// Queries all visible user tables right now and turns them into subscription plans.
/// Each shares the template of a query in `templates` if one compiles to the same.
pub(crate) fn get_all<T, F, I>(
    get_all_tables: F,
    relational_db: &RelationalDB,
    tx: &T,
    auth: &AuthCtx,
    templates: &mut TemplateCache,
) -> Result<Vec<CompiledQuery>, DBError>
where
    T: StateView,
    F: Fn(&RelationalDB, &T) -> Result<I, DBError>,
    I: Iterator<Item = Arc<TableSchema>>,
{
    get_all_tables(relational_db, tx)?
        .filter(|t| t.table_type == StTableType::User && auth.has_read_access(t.table_access) && !t.is_event)
        // A view with args can't be queried without them.
        .filter(|t| t.view_info.as_ref().is_none_or(|view| view.params.elements.is_empty()))
        .map(|schema| {
            let sql = format!("SELECT * FROM {}", schema.table_name);
            let tx = SchemaViewer::new(tx, auth);
            compile_template(&sql, &tx, auth, templates).map(|compiled| {
                let hash = QueryHash::from_string(&sql, auth.caller(), auth.bypass_rls() || compiled.has_param);
                CompiledQuery::new(compiled.into_plan(hash, sql))
            })
        })
        .collect()
}
