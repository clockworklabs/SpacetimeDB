use std::sync::Arc;

use crate::statement::Statement;
use crate::ty::TyId;
use spacetimedb_schema::schema::{ColumnSchema, TableSchema};
use spacetimedb_sql_parser::{
    ast::{
        self,
        sub::{SqlAst, SqlSelect},
        SqlFrom, SqlIdent, SqlJoin,
    },
    parser::sub::parse_subscription,
};

use super::{
    assert_eq_types,
    errors::{DuplicateName, TypingError, Unresolved, Unsupported},
    expr::{Expr, Let, RelExpr},
    ty::{Symbol, TyCtx, TyEnv},
    type_expr, type_proj, type_select, StatementCtx, StatementSource,
};

/// The result of type checking and name resolution
pub type TypingResult<T> = core::result::Result<T, TypingError>;

pub trait SchemaView {
    fn schema(&self, name: &str) -> Option<Arc<TableSchema>>;
}

pub trait TypeChecker {
    type Ast;
    type Set;

    fn type_ast(ctx: &mut TyCtx, ast: Self::Ast, tx: &impl SchemaView) -> TypingResult<RelExpr>;

    fn type_set(ctx: &mut TyCtx, ast: Self::Set, tx: &impl SchemaView) -> TypingResult<RelExpr>;

    fn type_from(
        ctx: &mut TyCtx,
        from: SqlFrom<Self::Ast>,
        tx: &impl SchemaView,
    ) -> TypingResult<(RelExpr, Option<Symbol>)> {
        match from {
            SqlFrom::Expr(expr, None) => Self::type_rel(ctx, expr, tx),
            SqlFrom::Expr(expr, Some(SqlIdent(alias))) => {
                let (expr, _) = Self::type_rel(ctx, expr, tx)?;
                let symbol = ctx.gen_symbol(alias);
                Ok((expr, Some(symbol)))
            }
            SqlFrom::Join(r, SqlIdent(alias), joins) => {
                // The type environment with which to type the join expressions
                let mut env = TyEnv::default();
                // The lowered inputs to the join operator
                let mut inputs = Vec::new();
                // The join expressions or predicates
                let mut exprs = Vec::new();
                // The types of the join variables or aliases
                let mut types = Vec::new();

                let input = Self::type_rel(ctx, r, tx)?.0;
                let ty = input.ty_id();
                let name = ctx.gen_symbol(alias);

                env.add(name, ty);
                inputs.push(input);
                types.push((name, ty));

                for SqlJoin {
                    expr,
                    alias: SqlIdent(alias),
                    on,
                } in joins
                {
                    let input = Self::type_rel(ctx, expr, tx)?.0;
                    let ty = input.ty_id();
                    let name = ctx.gen_symbol(&alias);

                    // New join variable is now in scope
                    if env.add(name, ty).is_some() {
                        return Err(DuplicateName(alias.into_string()).into());
                    }

                    inputs.push(input);
                    types.push((name, ty));

                    // Type check join expression with current type environment
                    if let Some(on) = on {
                        exprs.push(type_expr(ctx, &env, on, Some(TyId::BOOL))?);
                    }
                }

                let ty = ctx.add_row_type(types.clone());
                let input = RelExpr::Join(inputs.into(), ty);
                let vars = types
                    .into_iter()
                    .enumerate()
                    .map(|(i, (name, ty))| (name, Expr::Field(Box::new(Expr::Input(input.ty_id())), i, ty)))
                    .collect();
                Ok((RelExpr::select(input, Let { vars, exprs }), None))
            }
        }
    }

    fn type_rel(
        ctx: &mut TyCtx,
        expr: ast::RelExpr<Self::Ast>,
        tx: &impl SchemaView,
    ) -> TypingResult<(RelExpr, Option<Symbol>)> {
        match expr {
            ast::RelExpr::Var(SqlIdent(var)) => {
                let schema = tx
                    .schema(&var)
                    .ok_or_else(|| Unresolved::table(&var))
                    .map_err(TypingError::from)?;
                let mut types = Vec::new();
                for ColumnSchema { col_name, col_type, .. } in schema.columns() {
                    let id = ctx.add_algebraic_type(col_type);
                    let name = ctx.gen_symbol(col_name);
                    types.push((name, id));
                }
                let id = ctx.add_var_type(schema.table_id, types);
                let symbol = ctx.gen_symbol(var);
                Ok((RelExpr::RelVar(schema, id), Some(symbol)))
            }
            ast::RelExpr::Ast(ast) => Ok((Self::type_ast(ctx, *ast, tx)?, None)),
        }
    }
}

/// Type checker for subscriptions
struct SubChecker;

impl TypeChecker for SubChecker {
    type Ast = SqlAst;
    type Set = SqlAst;

    fn type_ast(ctx: &mut TyCtx, ast: Self::Ast, tx: &impl SchemaView) -> TypingResult<RelExpr> {
        Self::type_set(ctx, ast, tx)
    }

    fn type_set(ctx: &mut TyCtx, ast: Self::Set, tx: &impl SchemaView) -> TypingResult<RelExpr> {
        match ast {
            SqlAst::Union(a, b) => {
                let a = Self::type_ast(ctx, *a, tx)?;
                let b = Self::type_ast(ctx, *b, tx)?;
                assert_eq_types(ctx, a.ty_id(), b.ty_id())?;
                Ok(RelExpr::Union(Box::new(a), Box::new(b)))
            }
            SqlAst::Minus(a, b) => {
                let a = Self::type_ast(ctx, *a, tx)?;
                let b = Self::type_ast(ctx, *b, tx)?;
                assert_eq_types(ctx, a.ty_id(), b.ty_id())?;
                Ok(RelExpr::Minus(Box::new(a), Box::new(b)))
            }
            SqlAst::Select(SqlSelect {
                project,
                from,
                filter: None,
            }) => {
                let (input, alias) = Self::type_from(ctx, from, tx)?;
                type_proj(ctx, input, alias, project)
            }
            SqlAst::Select(SqlSelect {
                project,
                from,
                filter: Some(expr),
            }) => {
                let (from, alias) = Self::type_from(ctx, from, tx)?;
                let input = type_select(ctx, from, alias, expr)?;
                type_proj(ctx, input, alias, project)
            }
        }
    }
}

/// Parse and type check a subscription query
pub fn parse_and_type_sub(ctx: &mut TyCtx, sql: &str, tx: &impl SchemaView) -> TypingResult<RelExpr> {
    let expr = SubChecker::type_ast(ctx, parse_subscription(sql)?, tx)?;
    expect_table_type(ctx, expr)
}

/// Parse and type check a *subscription* query into a `StatementCtx`
pub fn compile_sql_sub<'a>(ctx: &mut TyCtx, sql: &'a str, tx: &impl SchemaView) -> TypingResult<StatementCtx<'a>> {
    let expr = parse_and_type_sub(ctx, sql, tx)?;
    Ok(StatementCtx {
        statement: Statement::Select(expr),
        sql,
        source: StatementSource::Subscription,
    })
}

/// Returns an error if the input type is not a table type or relvar
fn expect_table_type(ctx: &TyCtx, expr: RelExpr) -> TypingResult<RelExpr> {
    let _ = expr.ty(ctx)?.expect_relvar().map_err(|_| Unsupported::ReturnType)?;
    Ok(expr)
}

pub mod test_utils {
    use super::SchemaView;
    use spacetimedb_lib::db::raw_def::v9::RawIndexAlgorithm;
    use spacetimedb_lib::{db::raw_def::v9::RawModuleDefV9Builder, ProductType};
    use spacetimedb_primitives::{ColList, TableId};
    use spacetimedb_schema::{
        def::ModuleDef,
        schema::{Schema, TableSchema},
    };
    use std::sync::Arc;

    pub fn build_module_def(types: Vec<(&str, ProductType)>) -> ModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        for (name, ty) in types {
            builder.build_table_with_new_type(name, ty, true);
        }
        builder.finish().try_into().expect("failed to generate module def")
    }

    pub fn build_module_def_with_index(types: Vec<(&str, ProductType, Vec<ColList>)>) -> ModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        for (name, ty, idxs) in types {
            let mut table = builder.build_table_with_new_type(name, ty, true);
            for idx in idxs {
                table = table.with_index(RawIndexAlgorithm::BTree { columns: idx }, name, None);
            }
        }
        builder.finish().try_into().expect("failed to generate module def")
    }

    pub struct SchemaViewer(pub ModuleDef);

    impl SchemaView for SchemaViewer {
        fn schema(&self, name: &str) -> Option<Arc<TableSchema>> {
            self.0.table(name).map(|def| {
                Arc::new(TableSchema::from_module_def(
                    &self.0,
                    def,
                    (),
                    TableId(if *def.name == *"t" { 0 } else { 1 }),
                ))
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::test_utils::{build_module_def, SchemaViewer};
    use crate::ty::TyCtx;
    use spacetimedb_lib::{AlgebraicType, ProductType};
    use spacetimedb_schema::def::ModuleDef;

    use super::parse_and_type_sub;

    fn module_def() -> ModuleDef {
        build_module_def(vec![
            (
                "t",
                ProductType::from([
                    ("u32", AlgebraicType::U32),
                    ("f32", AlgebraicType::F32),
                    ("str", AlgebraicType::String),
                    ("arr", AlgebraicType::array(AlgebraicType::String)),
                ]),
            ),
            (
                "s",
                ProductType::from([
                    ("id", AlgebraicType::identity()),
                    ("u32", AlgebraicType::U32),
                    ("arr", AlgebraicType::array(AlgebraicType::String)),
                    ("bytes", AlgebraicType::bytes()),
                ]),
            ),
        ])
    }

    #[test]
    fn valid() {
        let tx = SchemaViewer(module_def());

        for sql in [
            "select * from t",
            "select * from t where true",
            "select * from t where t.u32 = 1",
            "select * from t where u32 = 1",
            "select * from t where t.u32 = 1 or t.str = ''",
            "select * from s where s.bytes = 0xABCD or bytes = X'ABCD'",
            "select * from s as r where r.bytes = 0xABCD or bytes = X'ABCD'",
            "select * from (select t.* from t join s)",
            "select * from (select t.* from t join s join s as r where t.u32 = s.u32 and s.u32 = r.u32)",
            "select * from (select t.* from t join s on t.u32 = s.u32 where t.f32 = 0.1)",
            "select * from (select t.* from t join (select s.u32 from s) s on t.u32 = s.u32)",
            "select * from (select t.* from t join (select u32 as a from s) s on t.u32 = s.a)",
            "select * from (select * from t union all select * from t)",
        ] {
            let result = parse_and_type_sub(&mut TyCtx::default(), sql, &tx);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn invalid() {
        let tx = SchemaViewer(module_def());

        for sql in [
            // Table r does not exist
            "select * from r",
            // Field a does not exist on table t
            "select * from t where t.a = 1",
            // Field a does not exist on table t
            "select * from t as r where r.a = 1",
            // Field u32 is not a string
            "select * from t where u32 = 'str'",
            // Field u32 is not a float
            "select * from t where t.u32 = 1.3",
            // t is not in scope after alias
            "select * from t as r where t.u32 = 5",
            // Subscriptions must be typed to a single table
            "select u32 from t",
            // Subscriptions must be typed to a single table
            "select * from t join s",
            // Self join requires aliases
            "select * from (select t.* from t join t)",
            // Product values are not comparable
            "select * from (select t.* from t join s on t.arr = s.arr)",
            // Subscriptions must be typed to a single table
            "select * from (select s.* from t join (select u32 from s) s on t.u32 = s.u32)",
            // Field u32 has been renamed
            "select * from (select t.* from t join (select u32 as a from s) s on t.u32 = s.u32)",
            // Field bytes is no longer in scope
            "select * from (select t.* from t join (select u32 from s) s on s.bytes = 0xABCD)",
            // Alias r is not in scope when it is referenced
            "select * from (select t.* from t join s on t.u32 = r.u32 join s as r)",
            // Union arguments are of different types
            "select * from (select * from t union all select * from s)",
        ] {
            let result = parse_and_type_sub(&mut TyCtx::default(), sql, &tx);
            assert!(result.is_err());
        }
    }
}
