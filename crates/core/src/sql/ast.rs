use anyhow::Context;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::system_tables::{StRowLevelSecurityFields, ST_ROW_LEVEL_SECURITY_ID};
use spacetimedb_expr::check::SchemaView;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_schema::schema::TableOrViewSchema;
use std::ops::Deref;
use std::sync::Arc;

pub struct SchemaViewer<'a, T> {
    tx: &'a T,
    auth: &'a AuthCtx,
}

impl<T> Deref for SchemaViewer<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.tx
    }
}

impl<T: StateView> SchemaView for SchemaViewer<'_, T> {
    fn table_id(&self, name: &str) -> Option<TableId> {
        self.tx
            .table_id_from_name_or_alias(name)
            .ok()
            .flatten()
            .and_then(|table_id| self.schema_for_table(table_id))
            .filter(|schema| self.auth.has_read_access(schema.table_access))
            .map(|schema| schema.table_id)
    }

    fn schema_for_table(&self, table_id: TableId) -> Option<Arc<TableOrViewSchema>> {
        self.tx
            .get_schema(table_id)
            .filter(|schema| self.auth.has_read_access(schema.table_access))
            .map(Arc::clone)
            .map(TableOrViewSchema::from)
            .map(Arc::new)
    }

    fn rls_rules_for_table(&self, table_id: TableId) -> anyhow::Result<Vec<Box<str>>> {
        self.tx
            .iter_by_col_eq(
                ST_ROW_LEVEL_SECURITY_ID,
                StRowLevelSecurityFields::TableId,
                &AlgebraicValue::from(table_id),
            )?
            .map(|row| {
                row.read_col::<AlgebraicValue>(StRowLevelSecurityFields::Sql)
                    .with_context(|| {
                        format!(
                            "Failed to read value from the `{}` column of `{}` for table_id `{}`",
                            "sql", "st_row_level_security", table_id
                        )
                    })
                    .and_then(|sql| {
                        sql.into_string().map_err(|_| {
                            anyhow::anyhow!(format!(
                                "Failed to read value from the `{}` column of `{}` for table_id `{}`",
                                "sql", "st_row_level_security", table_id
                            ))
                        })
                    })
            })
            .collect::<anyhow::Result<_>>()
    }
}

impl<'a, T> SchemaViewer<'a, T> {
    pub fn new(tx: &'a T, auth: &'a AuthCtx) -> Self {
        Self { tx, auth }
    }
}
