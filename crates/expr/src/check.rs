use std::cmp::max;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use super::{
    errors::{DuplicateName, TypingError, Unresolved, Unsupported},
    expr::RelExpr,
    type_expr, type_proj, type_select,
};
use crate::errors::{TableFunc, UnexpectedFunctionType};
use crate::expr::{Expr, FieldProject, LeftDeepJoin, ProjectList, ProjectName, Relvar};
use spacetimedb_lib::identity::AuthCtx;
use spacetimedb_lib::AlgebraicType;
use spacetimedb_primitives::{ArgId, TableId};
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_schema::schema::TableOrViewSchema;
use spacetimedb_sql_parser::ast::{BinOp, SqlExpr, SqlLiteral};
use spacetimedb_sql_parser::{
    ast::{sub::SqlSelect, SqlFrom, SqlIdent, SqlJoin},
    parser::sub::parse_subscription,
};

/// The result of type checking and name resolution
pub type TypingResult<T> = core::result::Result<T, TypingError>;

/// A view of the database schema
pub trait SchemaView {
    fn table_id(&self, name: &str) -> Option<TableId>;
    fn schema_for_table(&self, table_id: TableId) -> Option<Arc<TableOrViewSchema>>;
    fn rls_rules_for_table(&self, table_id: TableId) -> anyhow::Result<Vec<Box<str>>>;

    fn schema(&self, name: &str) -> Option<Arc<TableOrViewSchema>> {
        self.table_id(name).and_then(|table_id| self.schema_for_table(table_id))
    }

    fn get_or_create_params(&mut self, params: ProductValue) -> TypingResult<ArgId>;
}

#[derive(Default)]
pub struct Relvars(HashMap<Box<str>, Arc<TableOrViewSchema>>);

impl Deref for Relvars {
    type Target = HashMap<Box<str>, Arc<TableOrViewSchema>>;
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

    fn type_ast(ast: Self::Ast, tx: &mut impl SchemaView) -> TypingResult<ProjectList>;

    fn type_set(ast: Self::Set, vars: &mut Relvars, tx: &mut impl SchemaView) -> TypingResult<ProjectList>;

    fn type_view_params(
        schema: &TableOrViewSchema,
        vars: &mut Relvars,
        args: Option<Vec<SqlLiteral>>,
    ) -> TypingResult<Option<ProductValue>> {
        if !schema.is_view() && args.is_some() {
            return Err(TypingError::from(TableFunc(schema.table_name.to_string())));
        }
        if schema.view_info.as_ref().is_none_or(|a| a.args.is_empty()) && args.as_ref().is_none_or(|a| a.is_empty()) {
            return Ok(None);
        }

        let params_def: Vec<_> = schema
            .view_info
            .as_ref()
            .map_or(Vec::new(), |info| info.args.clone())
            .into_iter()
            .collect();

        let args = args.unwrap_or_default();
        let len = max(params_def.len(), args.len());

        let mut expected = Vec::with_capacity(params_def.len());
        let mut inferred = Vec::with_capacity(params_def.len());
        let mut params = Vec::with_capacity(params_def.len());
        let mut failed = false;

        let ty_literal = |lit: &SqlLiteral| match lit {
            SqlLiteral::Bool(_) => fmt_algebraic_type(&AlgebraicType::Bool).to_string(),
            SqlLiteral::Hex(_) => "Bytes?".to_string(),
            SqlLiteral::Num(_) => "Num?".to_string(),
            SqlLiteral::Str(_) => fmt_algebraic_type(&AlgebraicType::String).to_string(),
        };
        for i in 0..len {
            match (params_def.get(i), args.get(i)) {
                (Some(param), Some(arg)) => match type_expr(vars, SqlExpr::Lit(arg.clone()), Some(&param.ty)) {
                    Ok(Expr::Value(value, inferred_ty)) if inferred_ty == param.ty => {
                        if let Some(col) = schema.public_columns().get(i) {
                            if inferred_ty != col.col_type {
                                failed = true;

                                inferred.push(fmt_algebraic_type(&col.col_type).to_string());
                                expected.push(fmt_algebraic_type(&inferred_ty).to_string());

                                continue;
                            };
                            expected.push(fmt_algebraic_type(&param.ty).to_string());
                            inferred.push(fmt_algebraic_type(&inferred_ty).to_string());
                        } else {
                            failed = true;
                            expected.push("?".to_string());
                            inferred.push(fmt_algebraic_type(&inferred_ty).to_string());
                            continue;
                        };

                        params.push(value);
                    }
                    _ => {
                        failed = true;
                        expected.push(fmt_algebraic_type(&param.ty).to_string());
                        inferred.push(ty_literal(arg));
                    }
                },
                (Some(param), None) => {
                    failed = true;
                    expected.push(fmt_algebraic_type(&param.ty).to_string());
                }
                (None, Some(arg)) => {
                    failed = true;
                    inferred.push(ty_literal(arg));
                }
                (None, None) => {}
            }
        }

        if failed {
            return Err(UnexpectedFunctionType {
                expected: expected.join(", "),
                inferred: inferred.join(", "),
            }
            .into());
        }
        let params = ProductValue::from_iter(params);

        Ok(Some(params))
    }

    fn type_params(
        tx: &mut impl SchemaView,
        from: RelExpr,
        schema: Arc<TableOrViewSchema>,
        alias: Box<str>,
        params: Option<ProductValue>,
    ) -> TypingResult<RelExpr> {
        match params {
            None => Ok(from),
            Some(args) => {
                let new_arg_id = tx.get_or_create_params(args)?;
                let arg_id_col = schema.inner().get_column_by_name("arg_id").unwrap().col_pos;

                Ok(RelExpr::Select(
                    Box::new(from),
                    Expr::BinOp(
                        BinOp::Eq,
                        Box::new(Expr::Field(FieldProject {
                            table: alias,
                            field: arg_id_col.idx(),
                            ty: AlgebraicType::U64,
                        })),
                        Box::new(Expr::Value(AlgebraicValue::U64(new_arg_id.0), AlgebraicType::U64)),
                    ),
                ))
            }
        }
    }

    fn type_from(from: SqlFrom, vars: &mut Relvars, tx: &mut impl SchemaView) -> TypingResult<RelExpr> {
        match from {
            SqlFrom::Expr(SqlIdent(name), SqlIdent(alias)) => {
                let schema = Self::type_relvar(tx, &name)?;
                // Verify we don't have call `SELECT * FROM view` if the view requires parameters...
                Self::type_view_params(&schema, vars, None)?;

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

                for SqlJoin { from, on } in joins {
                    let (SqlIdent(name), SqlIdent(alias), params) = from.into_name_alias();
                    assert!(params.is_none(), "Function calls not allowed in JOINs");
                    // Check for duplicate aliases
                    if vars.contains_key(&alias) {
                        return Err(DuplicateName(alias.into_string()).into());
                    }
                    let schema = Self::type_relvar(tx, &name)?;
                    let arg = Self::type_view_params(&schema, vars, params)?;
                    let lhs = Box::new(Self::type_params(tx, join, schema.clone(), alias.clone(), arg)?);

                    let rhs = Relvar {
                        schema,
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
            SqlFrom::FuncCall(func, SqlIdent(alias)) => {
                let schema = Self::type_relvar(tx, &func.name.0)?;
                let arg = Self::type_view_params(&schema, vars, Some(func.args))?;
                vars.insert(alias.clone(), schema.clone());
                let from = RelExpr::RelVar(Relvar {
                    schema: schema.clone(),
                    alias: alias.clone(),
                    delta: None,
                });

                Self::type_params(tx, from, schema, alias, arg)
            }
        }
    }

    fn type_relvar(tx: &impl SchemaView, name: &str) -> TypingResult<Arc<TableOrViewSchema>> {
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

    fn type_ast(ast: Self::Ast, tx: &mut impl SchemaView) -> TypingResult<ProjectList> {
        Self::type_set(ast, &mut Relvars::default(), tx)
    }

    fn type_set(ast: Self::Set, vars: &mut Relvars, tx: &mut impl SchemaView) -> TypingResult<ProjectList> {
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
pub fn parse_and_type_sub(sql: &str, tx: &mut impl SchemaView, auth: &AuthCtx) -> TypingResult<(ProjectName, bool)> {
    let ast = parse_subscription(sql)?;
    let has_param = ast.has_parameter();
    let ast = ast.resolve_sender(auth.caller());
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

pub mod test_utils {
    use spacetimedb_lib::{db::raw_def::v9::RawModuleDefV9Builder, ProductType};
    use spacetimedb_primitives::{ArgId, TableId};
    use spacetimedb_sats::{AlgebraicType, ProductValue};
    use spacetimedb_schema::{
        def::ModuleDef,
        schema::{Schema, TableOrViewSchema, TableSchema},
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::{SchemaView, TypingResult};
    pub struct ViewInfo<'a> {
        pub(crate) name: &'a str,
        pub(crate) columns: &'a [(&'a str, AlgebraicType)],
        pub(crate) params: ProductType,
        pub(crate) is_anonymous: bool,
    }

    pub fn build_module_def(tables: Vec<(&str, ProductType)>, views: Vec<ViewInfo>) -> ModuleDef {
        let mut builder = RawModuleDefV9Builder::new();
        for (name, ty) in tables {
            builder.build_table_with_new_type(name, ty, true);
        }
        for view in views {
            let product_type = AlgebraicType::from(ProductType::from_iter(view.columns.iter().cloned()));
            let type_ref = builder.add_algebraic_type([], view.name, product_type, true);
            let return_type = AlgebraicType::array(AlgebraicType::Ref(type_ref));
            builder.add_view(view.name, 0, true, view.is_anonymous, view.params, return_type);
        }
        builder.finish().try_into().expect("failed to generate module def")
    }

    pub struct MockCallParams {
        counter: u64,
        params: HashMap<ProductValue, ArgId>,
    }

    impl Default for MockCallParams {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockCallParams {
        pub fn new() -> Self {
            Self {
                counter: 0,
                params: HashMap::new(),
            }
        }

        pub fn get_or_insert(&mut self, value: ProductValue) -> ArgId {
            if let Some(existing) = self.params.get(&value) {
                *existing
            } else {
                self.counter += 1;
                let arg_id = ArgId(self.counter - 1);
                self.params.insert(value, arg_id);
                arg_id
            }
        }
    }

    pub struct SchemaViewer(pub ModuleDef, pub MockCallParams);

    impl SchemaView for SchemaViewer {
        fn table_id(&self, name: &str) -> Option<TableId> {
            match name {
                "t" => Some(TableId(0)),
                "s" => Some(TableId(1)),
                "v" => Some(TableId(2)),
                "w" => Some(TableId(3)),
                "x" => Some(TableId(4)),
                _ => None,
            }
        }

        fn schema_for_table(&self, table_id: TableId) -> Option<Arc<TableOrViewSchema>> {
            let (table_id, name) = match table_id.idx() {
                0 => (TableId(0), "t"),
                1 => (TableId(1), "s"),
                2 => (TableId(2), "v"),
                3 => (TableId(3), "w"),
                4 => (TableId(4), "x"),
                _ => return None,
            };
            self.0
                .table(name)
                .map(|def| TableSchema::from_module_def(&self.0, def, (), table_id))
                .or_else(|| {
                    self.0
                        .view(name)
                        .map(|def| TableSchema::from_view_def_for_datastore(&self.0, def))
                })
                .map(|x| Arc::new(TableOrViewSchema::from(Arc::new(x))))
        }

        fn rls_rules_for_table(&self, _: TableId) -> anyhow::Result<Vec<Box<str>>> {
            Ok(vec![])
        }

        fn get_or_create_params(&mut self, params: ProductValue) -> TypingResult<ArgId> {
            Ok(self.1.get_or_insert(params))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SchemaView, TypingResult};
    use crate::{
        check::test_utils::{build_module_def, SchemaViewer},
        expr::ProjectName,
    };
    use spacetimedb_lib::{identity::AuthCtx, AlgebraicType, ProductType};
    use spacetimedb_schema::def::ModuleDef;
    fn module_def() -> ModuleDef {
        build_module_def(
            vec![
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
            ],
            vec![],
        )
    }

    /// A wrapper around [super::parse_and_type_sub] that takes a dummy [AuthCtx]
    fn parse_and_type_sub(sql: &str, tx: &mut impl SchemaView) -> TypingResult<ProjectName> {
        super::parse_and_type_sub(sql, tx, &AuthCtx::for_testing()).map(|(plan, _)| plan)
    }

    #[test]
    fn valid_literals() {
        let mut tx = SchemaViewer(module_def(), Default::default());

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
            let result = parse_and_type_sub(sql, &mut tx);
            assert!(result.is_ok(), "name: {}, error: {}", msg, result.unwrap_err());
        }
    }

    #[test]
    fn valid_literals_for_type() {
        let mut tx = SchemaViewer(module_def(), Default::default());

        for ty in [
            "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "f32", "f64", "i128", "u128", "i256", "u256",
        ] {
            let sql = format!("select * from t where {ty} = 127");
            let result = parse_and_type_sub(&sql, &mut tx);
            assert!(result.is_ok(), "Failed to parse {ty}: {}", result.unwrap_err());
        }
    }

    #[test]
    fn invalid_literals() {
        let mut tx = SchemaViewer(module_def(), Default::default());

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
            let result = parse_and_type_sub(sql, &mut tx);
            assert!(result.is_err(), "{msg}");
        }
    }

    #[test]
    fn valid() {
        let mut tx = SchemaViewer(module_def(), Default::default());

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
            let result = parse_and_type_sub(sql, &mut tx);
            assert!(result.is_ok(), "{msg}");
        }
    }

    #[test]
    fn invalid() {
        let mut tx = SchemaViewer(module_def(), Default::default());

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
            let result = parse_and_type_sub(sql, &mut tx);
            assert!(result.is_err(), "{msg}");
        }
    }
}
