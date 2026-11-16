use super::ast::SchemaViewer;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::locking_tx_datastore::MutTxId;
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

        // A rule replaces the relvar it restricts, so a view call in it would drop the query's own args
        let (ProjectName::None(expr) | ProjectName::Some(expr, _)) = &sql;
        anyhow::ensure!(
            !expr.calls_view_with_args(),
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

#[cfg(test)]
mod tests {
    use super::RowLevelExpr;
    use crate::relational_db::tests_utils::{
        begin_mut_tx, create_view_for_test, create_view_with_params_for_test, TestDB,
    };
    use spacetimedb_lib::db::raw_def::v9::RawRowLevelSecurityDefV9;
    use spacetimedb_lib::identity::AuthCtx;
    use spacetimedb_lib::{AlgebraicType, ProductType};

    /// A rule that calls a view with args is rejected when it is published, filtered or not
    #[test]
    fn rls_rule_on_a_view_call_with_args_is_rejected() -> anyhow::Result<()> {
        let db = TestDB::in_memory()?;
        let schema = [("x", AlgebraicType::U32)];
        let params = ProductType::from([("id", AlgebraicType::U32)]);
        create_view_with_params_for_test(&db, "v", params, &schema, true)?;
        create_view_for_test(&db, "w", &schema, true)?;

        let mut tx = begin_mut_tx(&db);
        let auth = AuthCtx::for_testing();
        let mut build = |sql: &str| {
            let rls = RawRowLevelSecurityDefV9 { sql: sql.into() };
            RowLevelExpr::build_row_level_expr(&mut tx, &auth, &rls)
        };

        for sql in ["select * from v(1)", "select * from v(1) where x = 1"] {
            let Err(error) = build(sql) else {
                panic!("`{sql}` should be rejected");
            };
            assert!(
                error.to_string().contains("not supported on parameterized views"),
                "`{sql}`: {error}"
            );
        }
        // A view without args can still have a rule
        build("select * from w where x = 1")?;

        Ok(())
    }
}
