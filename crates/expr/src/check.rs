use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use super::{
    errors::{DuplicateName, TypingError, Unresolved, Unsupported},
    expr::RelExpr,
    type_expr, type_proj, type_select, StatementCtx, StatementSource,
};
use crate::expr::LeftDeepJoin;
use crate::expr::{Expr, ProjectList, ProjectName, Relvar};
use crate::statement::Statement;
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::AlgebraicType;
use spacetimedb_primitives::TableId;
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_sql_parser::ast::BinOp;
use spacetimedb_sql_parser::{
    ast::{sub::SqlSelect, SqlFrom, SqlIdent, SqlJoin},
    parser::sub::parse_subscription,
};

/// The result of type checking and name resolution
pub type TypingResult<T> = core::result::Result<T, TypingError>;

/// A view of the database schema
pub trait SchemaView {
    fn table_id(&self, name: &str) -> Option<TableId>;
    fn schema_for_table(&self, table_id: TableId) -> Option<Arc<TableSchema>>;
    fn rls_rules_for_table(&self, table_id: TableId) -> anyhow::Result<Vec<Box<str>>>;

    fn schema(&self, name: &str) -> Option<Arc<TableSchema>> {
        self.table_id(name).and_then(|table_id| self.schema_for_table(table_id))
    }
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

    fn type_ast(ast: Self::Ast, tx: &impl SchemaView) -> TypingResult<ProjectList>;

    fn type_set(ast: Self::Set, vars: &mut Relvars, tx: &impl SchemaView) -> TypingResult<ProjectList>;

    fn type_from(from: SqlFrom, vars: &mut Relvars, tx: &impl SchemaView) -> TypingResult<RelExpr> {
        match from {
            SqlFrom::Expr(SqlIdent(name), SqlIdent(alias)) => {
                let schema = Self::type_relvar(tx, &name)?;
                vars.insert(alias.clone(), schema.clone());
                Ok(RelExpr::RelVar(Relvar {
                    schema,
                    alias,
                    delta: None,
                }))
            }
            SqlFrom::Join(SqlIdent(name), SqlIdent(alias), joins) => {
                let schema = Self::type_relvar(tx, &name)?;
                vars.insert(alias.clone(), schema.clone());
                let mut join = RelExpr::RelVar(Relvar {
                    schema,
                    alias,
                    delta: None,
                });

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

                    let lhs = Box::new(join);
                    let rhs = Relvar {
                        schema: Self::type_relvar(tx, &name)?,
                        alias,
                        delta: None,
                    };

                    vars.insert(rhs.alias.clone(), rhs.schema.clone());

                    if let Some(on) = on {
                        if let Expr::BinOp(BinOp::Eq, a, b) = type_expr(vars, on, Some(&AlgebraicType::Bool))? {
                            if let (Expr::Field(a), Expr::Field(b)) = (*a, *b) {
                                join = RelExpr::EqJoin(LeftDeepJoin { lhs, rhs }, a, b);
                                continue;
                            }
                        }
                        unreachable!("Unreachability guaranteed by parser")
                    }

                    join = RelExpr::LeftDeepJoin(LeftDeepJoin { lhs, rhs });
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

    fn type_ast(ast: Self::Ast, tx: &impl SchemaView) -> TypingResult<ProjectList> {
        Self::type_set(ast, &mut Relvars::default(), tx)
    }

    fn type_set(ast: Self::Set, vars: &mut Relvars, tx: &impl SchemaView) -> TypingResult<ProjectList> {
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
pub fn parse_and_type_sub(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> TypingResult<(ProjectName, bool)> {
    let ast = parse_subscription(sql)?;
    let has_param = ast.has_parameter();
    let ast = ast.resolve_sender(auth.caller);
    expect_table_type(SubChecker::type_ast(ast, tx)?).map(|plan| (plan, has_param))
}

/// Returns an error if the input type is not a table type or relvar
fn expect_table_type(expr: ProjectList) -> TypingResult<ProjectName> {
    match expr {
        // Note, this is called before we do any RLS resolution.
        // Hence this length should always be 1.
        ProjectList::Name(mut proj) if proj.len() == 1 => Ok(proj.pop().unwrap()),
        ProjectList::Limit(input, _) => expect_table_type(*input),
        ProjectList::Name(..) | ProjectList::List(..) | ProjectList::Agg(..) => Err(Unsupported::ReturnType.into()),
    }
}

/// Parse and type check a *subscription* query into a `StatementCtx`
pub fn compile_sql_sub<'a>(
    sql: &'a str,
    tx: &impl SchemaView,
    auth: &AuthCtx,
    with_timings: bool,
) -> TypingResult<StatementCtx<'a>> {
    let planning_time = if with_timings {
        Some(std::time::Instant::now())
    } else {
        None
    };
    let (plan, _) = parse_and_type_sub(sql, tx, auth)?;
    Ok(StatementCtx {
        statement: Statement::Select(ProjectList::Name(vec![plan])),
        sql,
        source: StatementSource::Subscription,
        planning_time: planning_time.map(|t| t.elapsed()),
    })
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
        fn table_id(&self, name: &str) -> Option<TableId> {
            match name {
                "t" => Some(TableId(0)),
                "s" => Some(TableId(1)),
                _ => None,
            }
        }

        fn schema_for_table(&self, table_id: TableId) -> Option<Arc<TableSchema>> {
            match table_id.idx() {
                0 => Some((TableId(0), "t")),
                1 => Some((TableId(1), "s")),
                _ => None,
            }
            .and_then(|(table_id, name)| {
                self.0
                    .table(name)
                    .map(|def| Arc::new(TableSchema::from_module_def(&self.0, def, (), table_id)))
            })
        }

        fn rls_rules_for_table(&self, _: TableId) -> anyhow::Result<Vec<Box<str>>> {
            Ok(vec![])
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        check::test_utils::{build_module_def, SchemaViewer},
        expr::ProjectName,
    };
    use spacetimedb_lib::{identity::AuthCtx, AlgebraicType, ProductType};
    use spacetimedb_schema::def::ModuleDef;

    use super::{SchemaView, TypingResult};

    fn module_def() -> ModuleDef {
        build_module_def(vec![
            (
                "t",
                ProductType::from([
                    ("ts", AlgebraicType::timestamp()),
                    ("i8", AlgebraicType::I8),
                    ("u8", AlgebraicType::U8),
                    ("i16", AlgebraicType::I16),
                    ("u16", AlgebraicType::U16),
                    ("i32", AlgebraicType::I32),
                    ("u32", AlgebraicType::U32),
                    ("i64", AlgebraicType::I64),
                    ("u64", AlgebraicType::U64),
                    ("int", AlgebraicType::U32),
                    ("f32", AlgebraicType::F32),
                    ("f64", AlgebraicType::F64),
                    ("i128", AlgebraicType::I128),
                    ("u128", AlgebraicType::U128),
                    ("i256", AlgebraicType::I256),
                    ("u256", AlgebraicType::U256),
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

    /// A wrapper around [super::parse_and_type_sub] that takes a dummy [AuthCtx]
    fn parse_and_type_sub(sql: &str, tx: &impl SchemaView) -> TypingResult<ProjectName> {
        super::parse_and_type_sub(sql, tx, &AuthCtx::for_testing()).map(|(plan, _)| plan)
    }

    #[test]
    fn valid_literals() {
        let tx = SchemaViewer(module_def());

        struct TestCase {
            sql: &'static str,
            msg: &'static str,
        }

        for TestCase { sql, msg } in [
            TestCase {
                sql: "select * from t where i32 = -1",
                msg: "Leading `-`",
            },
            TestCase {
                sql: "select * from t where u32 = +1",
                msg: "Leading `+`",
            },
            TestCase {
                sql: "select * from t where u32 = 1e3",
                msg: "Scientific notation",
            },
            TestCase {
                sql: "select * from t where u32 = 1E3",
                msg: "Case insensitive scientific notation",
            },
            TestCase {
                sql: "select * from t where f32 = 1e3",
                msg: "Integers can parse as floats",
            },
            TestCase {
                sql: "select * from t where f32 = 1e-3",
                msg: "Negative exponent",
            },
            TestCase {
                sql: "select * from t where f32 = 0.1",
                msg: "Standard decimal notation",
            },
            TestCase {
                sql: "select * from t where f32 = .1",
                msg: "Leading `.`",
            },
            TestCase {
                sql: "select * from t where f32 = 1e40",
                msg: "Infinity",
            },
            TestCase {
                sql: "select * from t where u256 = 1e40",
                msg: "u256",
            },
            TestCase {
                sql: "select * from t where ts = '2025-02-10T15:45:30Z'",
                msg: "timestamp",
            },
            TestCase {
                sql: "select * from t where ts = '2025-02-10T15:45:30.123Z'",
                msg: "timestamp ms",
            },
            TestCase {
                sql: "select * from t where ts = '2025-02-10T15:45:30.123456789Z'",
                msg: "timestamp ns",
            },
            TestCase {
                sql: "select * from t where ts = '2025-02-10 15:45:30+02:00'",
                msg: "timestamp with timezone",
            },
            TestCase {
                sql: "select * from t where ts = '2025-02-10 15:45:30.123+02:00'",
                msg: "timestamp ms with timezone",
            },
        ] {
            let result = parse_and_type_sub(sql, &tx);
            assert!(result.is_ok(), "name: {}, error: {}", msg, result.unwrap_err());
        }
    }

    #[test]
    fn valid_literals_for_type() {
        let tx = SchemaViewer(module_def());

        for ty in [
            "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "f32", "f64", "i128", "u128", "i256", "u256",
        ] {
            let sql = format!("select * from t where {ty} = 127");
            let result = parse_and_type_sub(&sql, &tx);
            assert!(result.is_ok(), "Faild to parse {ty}: {}", result.unwrap_err());
        }
    }

    #[test]
    fn invalid_literals() {
        let tx = SchemaViewer(module_def());

        struct TestCase {
            sql: &'static str,
            msg: &'static str,
        }

        for TestCase { sql, msg } in [
            TestCase {
                sql: "select * from t where u8 = -1",
                msg: "Negative integer for unsigned column",
            },
            TestCase {
                sql: "select * from t where u8 = 1e3",
                msg: "Out of bounds",
            },
            TestCase {
                sql: "select * from t where u8 = 0.1",
                msg: "Float as integer",
            },
            TestCase {
                sql: "select * from t where u32 = 1e-3",
                msg: "Float as integer",
            },
            TestCase {
                sql: "select * from t where i32 = 1e-3",
                msg: "Float as integer",
            },
        ] {
            let result = parse_and_type_sub(sql, &tx);
            assert!(result.is_err(), "{msg}");
        }
    }

    #[test]
    fn valid() {
        let tx = SchemaViewer(module_def());

        struct TestCase {
            sql: &'static str,
            msg: &'static str,
        }

        for TestCase { sql, msg } in [
            TestCase {
                sql: "select * from t",
                msg: "Can select * on any table",
            },
            TestCase {
                sql: "select * from t where true",
                msg: "Boolean literals are valid in WHERE clause",
            },
            TestCase {
                sql: "select * from t where t.u32 = 1",
                msg: "Can qualify column references with table name",
            },
            TestCase {
                sql: "select * from t where u32 = 1",
                msg: "Can leave columns unqualified when unambiguous",
            },
            TestCase {
                sql: "select * from s where id = :sender",
                msg: "Can use :sender as an Identity",
            },
            TestCase {
                sql: "select * from s where bytes = :sender",
                msg: "Can use :sender as a byte array",
            },
            TestCase {
                sql: "select * from t where t.u32 = 1 or t.str = ''",
                msg: "Type OR with qualified column references",
            },
            TestCase {
                sql: "select * from s where s.bytes = 0xABCD or bytes = X'ABCD'",
                msg: "Type OR with mixed qualified and unqualified column references",
            },
            TestCase {
                sql: "select * from s as r where r.bytes = 0xABCD or bytes = X'ABCD'",
                msg: "Type OR with table alias",
            },
            TestCase {
                sql: "select t.* from t join s",
                msg: "Type cross join + projection",
            },
            TestCase {
                sql: "select t.* from t join s join s as r where t.u32 = s.u32 and s.u32 = r.u32",
                msg: "Type self join + projection",
            },
            TestCase {
                sql: "select t.* from t join s on t.u32 = s.u32 where t.f32 = 0.1",
                msg: "Type inner join + projection",
            },
        ] {
            let result = parse_and_type_sub(sql, &tx);
            assert!(result.is_ok(), "{msg}");
        }
    }

    #[test]
    fn invalid() {
        let tx = SchemaViewer(module_def());

        struct TestCase {
            sql: &'static str,
            msg: &'static str,
        }

        for TestCase { sql, msg } in [
            TestCase {
                sql: "select * from r",
                msg: "Table r does not exist",
            },
            TestCase {
                sql: "select * from t where arr = :sender",
                msg: "The :sender param is an identity",
            },
            TestCase {
                sql: "select * from t where t.a = 1",
                msg: "Field a does not exist on table t",
            },
            TestCase {
                sql: "select * from t as r where r.a = 1",
                msg: "Field a does not exist on table t",
            },
            TestCase {
                sql: "select * from t where u32 = 'str'",
                msg: "Field u32 is not a string",
            },
            TestCase {
                sql: "select * from t where t.u32 = 1.3",
                msg: "Field u32 is not a float",
            },
            TestCase {
                sql: "select * from t as r where t.u32 = 5",
                msg: "t is not in scope after alias",
            },
            TestCase {
                sql: "select u32 from t",
                msg: "Subscriptions must be typed to a single table",
            },
            TestCase {
                sql: "select * from t join s",
                msg: "Subscriptions must be typed to a single table",
            },
            TestCase {
                sql: "select t.* from t join t",
                msg: "Self join requires aliases",
            },
            TestCase {
                sql: "select t.* from t join s on t.arr = s.arr",
                msg: "Product values are not comparable",
            },
            TestCase {
                sql: "select t.* from t join s on t.u32 = r.u32 join s as r",
                msg: "Alias r is not in scope when it is referenced",
            },
            TestCase {
                sql: "select * from t limit 5",
                msg: "Subscriptions do not support limit",
            },
            TestCase {
                sql: "select t.* from t join s on t.u32 = s.u32 where bytes = 0xABCD",
                msg: "Columns must be qualified in join expressions",
            },
        ] {
            let result = parse_and_type_sub(sql, &tx);
            assert!(result.is_err(), "{msg}");
        }
    }
}
