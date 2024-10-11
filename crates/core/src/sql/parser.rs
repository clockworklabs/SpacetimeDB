use spacetimedb_expr::check::parse_and_type_sub;
use spacetimedb_expr::errors::TypingError;
use spacetimedb_expr::expr::RelExpr;
use spacetimedb_expr::ty::TyCtx;
use spacetimedb_lib::db::raw_def::v9::RawRowLevelSecurityDefV9;
use spacetimedb_schema::schema::{RowLevelSecuritySchema, TableSchema};
use std::sync::Arc;

pub struct RowLevelExpr {
    pub sql: RelExpr,
    pub def: RowLevelSecuritySchema,
}

impl TryFrom<(&RawRowLevelSecurityDefV9, &[Arc<TableSchema>])> for RowLevelExpr {
    type Error = TypingError;

    fn try_from((rls, tx): (&RawRowLevelSecurityDefV9, &[Arc<TableSchema>])) -> Result<Self, Self::Error> {
        let mut ctx = TyCtx::default();
        let sql = parse_and_type_sub(&mut ctx, &rls.sql, &tx)?;

        Ok(Self {
            def: RowLevelSecuritySchema {
                table_id: sql.table_id(&mut ctx)?,
                sql: rls.sql.clone(),
            },
            sql,
        })
    }
}
