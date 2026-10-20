use super::ast::SchemaViewer;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
use spacetimedb_expr::check::parse_and_type_sub;
use spacetimedb_expr::expr::{ProjectName, RelExpr, Relvar};
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

        // Type checking only allows a view call with args as a lone relvar
        let (ProjectName::None(expr) | ProjectName::Some(expr, _)) = &sql;
        anyhow::ensure!(
            !matches!(expr, RelExpr::RelVar(Relvar { view_args: Some(_), .. })),
            "Row-level security is not supported on parameterized views: {}",
            rls.sql
        );
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
