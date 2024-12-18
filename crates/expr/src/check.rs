use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::expr::{Expr, Project};
use crate::{expr::LeftDeepJoin, statement::Statement};
use spacetimedb_lib::AlgebraicType;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_sql_parser::ast::BinOp;
use spacetimedb_sql_parser::{
    ast::{sub::SqlSelect, SqlFrom, SqlIdent, SqlJoin},
    parser::sub::parse_subscription,
};

use super::{
    errors::{DuplicateName, TypingError, Unresolved, Unsupported},
    expr::RelExpr,
    type_expr, type_proj, type_select, StatementCtx, StatementSource,
};

/// The result of type checking and name resolution
pub type TypingResult<T> = core::result::Result<T, TypingError>;

/// A view of the database schema
pub trait SchemaView {
    fn schema(&self, name: &str) -> Option<Arc<TableSchema>>;
}

#[derive(Default)]
pub struct Relvars(HashMap<Box<str>, Arc<TableSchema>>);

impl Deref for Relvars {
    type Target = HashMap<Box<str>, Arc<TableSchema>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Relvars {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub trait TypeChecker {
    type Ast;
    type Set;

    fn type_ast(ast: Self::Ast, tx: &impl SchemaView) -> TypingResult<Project>;

    fn type_set(ast: Self::Set, vars: &mut Relvars, tx: &impl SchemaView) -> TypingResult<Project>;

    fn type_from(from: SqlFrom, vars: &mut Relvars, tx: &impl SchemaView) -> TypingResult<RelExpr> {
        match from {
            SqlFrom::Expr(SqlIdent(name), SqlIdent(alias)) => {
                let schema = Self::type_relvar(tx, &name)?;
                vars.insert(alias.clone(), schema.clone());
                Ok(RelExpr::RelVar(schema, alias))
            }
            SqlFrom::Join(SqlIdent(name), SqlIdent(alias), joins) => {
                let schema = Self::type_relvar(tx, &name)?;
                vars.insert(alias.clone(), schema.clone());
                let mut join = RelExpr::RelVar(schema, alias);

                for SqlJoin {
                    var: SqlIdent(name),
                    alias: SqlIdent(alias),
                    on,
                } in joins
                {
                    // Check for duplicate aliases
                    if vars.contains_key(&alias) {
                        return Err(DuplicateName(alias.into_string()).into());
                    }

                    let rhs = Self::type_relvar(tx, &name)?;
                    let lhs = Box::new(join);
                    let var = alias;

                    vars.insert(var.clone(), rhs.clone());

                    if let Some(on) = on {
                        if let Expr::BinOp(BinOp::Eq, a, b) = type_expr(vars, on, Some(&AlgebraicType::Bool))? {
                            if let (Expr::Field(a), Expr::Field(b)) = (*a, *b) {
                                join = RelExpr::EqJoin(LeftDeepJoin { lhs, rhs, var }, a, b);
                                continue;
                            }
                        }
                        unreachable!("Unreachability guaranteed by parser")
                    }

                    join = RelExpr::LeftDeepJoin(LeftDeepJoin { lhs, rhs, var });
                }

                Ok(join)
            }
        }
    }

    fn type_relvar(tx: &impl SchemaView, name: &str) -> TypingResult<Arc<TableSchema>> {
        tx.schema(name)
            .ok_or_else(|| Unresolved::table(name))
            .map_err(TypingError::from)
    }
}

/// Type checker for subscriptions
struct SubChecker;

impl TypeChecker for SubChecker {
    type Ast = SqlSelect;
    type Set = SqlSelect;

    fn type_ast(ast: Self::Ast, tx: &impl SchemaView) -> TypingResult<Project> {
        Self::type_set(ast, &mut Relvars::default(), tx)
    }

    fn type_set(ast: Self::Set, vars: &mut Relvars, tx: &impl SchemaView) -> TypingResult<Project> {
        match ast {
            SqlSelect {
                project,
                from,
                filter: None,
            } => {
                let input = Self::type_from(from, vars, tx)?;
                type_proj(input, project, vars)
            }
            SqlSelect {
                project,
                from,
                filter: Some(expr),
            } => {
                let input = Self::type_from(from, vars, tx)?;
                type_proj(type_select(input, expr, vars)?, project, vars)
            }
        }
    }
}

/// Parse and type check a subscription query
pub fn parse_and_type_sub(sql: &str, tx: &impl SchemaView) -> TypingResult<Project> {
    expect_table_type(SubChecker::type_ast(parse_subscription(sql)?, tx)?)
}

/// Parse and type check a *subscription* query into a `StatementCtx`
pub fn compile_sql_sub<'a>(sql: &'a str, tx: &impl SchemaView) -> TypingResult<StatementCtx<'a>> {
    let expr = parse_and_type_sub(sql, tx)?;
    Ok(StatementCtx {
        statement: Statement::Select(expr),
        sql,
        source: StatementSource::Subscription,
    })
}

/// Returns an error if the input type is not a table type or relvar
fn expect_table_type(expr: Project) -> TypingResult<Project> {
    if let Project::Fields(..) = expr {
        return Err(Unsupported::ReturnType.into());
    }
    Ok(expr)
}

pub mod test_utils {
    use spacetimedb_lib::{db::raw_def::v9::RawModuleDefV9Builder, ProductType};
    use spacetimedb_primitives::TableId;
    use spacetimedb_schema::{
        def::ModuleDef,
        schema::{Schema, TableSchema},
    };
    use std::sync::Arc;

    use super::SchemaView;

    pub fn build_module_def(types: Vec<(&str, ProductType)>) -> ModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        for (name, ty) in types {
            builder.build_table_with_new_type(name, ty, true);
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
    use spacetimedb_lib::{AlgebraicType, ProductType};
    use spacetimedb_schema::def::ModuleDef;

    use super::parse_and_type_sub;

    fn module_def() -> ModuleDef {
        build_module_def(vec![
            (
                "t",
                ProductType::from([
                    ("int", AlgebraicType::U32),
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
            "select t.* from t join s",
            "select t.* from t join s join s as r where t.u32 = s.u32 and s.u32 = r.u32",
            "select t.* from t join s on t.u32 = s.u32 where t.f32 = 0.1",
        ] {
            let result = parse_and_type_sub(sql, &tx);
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
            "select t.* from t join t",
            // Product values are not comparable
            "select t.* from t join s on t.arr = s.arr",
            // Alias r is not in scope when it is referenced
            "select t.* from t join s on t.u32 = r.u32 join s as r",
        ] {
            let result = parse_and_type_sub(sql, &tx);
            assert!(result.is_err());
        }
    }
}
