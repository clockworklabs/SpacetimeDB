use crate::db::datastore::locking_tx_datastore::state_view::StateView;
use crate::db::datastore::locking_tx_datastore::MutTxId;
use crate::sql::ast::SchemaViewer;
use spacetimedb_expr::check::parse_and_type_sub;
use spacetimedb_expr::expr::ProjectName;
use spacetimedb_lib::db::auth::StAccess;
use spacetimedb_lib::db::raw_def::v9::RawRowLevelSecurityDefV9;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_schema::schema::RowLevelSecuritySchema;

pub struct RowLevelExpr {
    pub sql: ProjectName,
    pub def: RowLevelSecuritySchema,
}

impl RowLevelExpr {
    pub fn build_row_level_expr(
        tx: &mut MutTxId,
        auth_ctx: &AuthCtx,
        rls: &RawRowLevelSecurityDefV9,
    ) -> anyhow::Result<Self> {
        let (sql, _) = parse_and_type_sub(&rls.sql, &SchemaViewer::new(tx, auth_ctx), auth_ctx)?;
        let table_id = sql.return_table_id().unwrap();
        let schema = tx.schema_for_table(table_id)?;

        match schema.table_access {
            StAccess::Private => {
                anyhow::bail!(
                    "Cannot define RLS rule on private table: {}. \
                        Please make table public if you wish to restrict access using RLS.",
                    schema.table_name
                )
            }
            StAccess::Public => Ok(Self {
                def: RowLevelSecuritySchema {
                    table_id,
                    sql: rls.sql.clone(),
                },
                sql,
            }),
        }
    }
}
