use std::sync::Arc;

use spacetimedb_lib::{identity::AuthCtx, st_var::StVarValue, AlgebraicType, AlgebraicValue, ProductValue};
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_schema::schema::{ColumnSchema, TableSchema};
use spacetimedb_sql_parser::{
    ast::{
        sql::{SqlAst, SqlDelete, SqlInsert, SqlSelect, SqlSet, SqlShow, SqlUpdate},
        BinOp, SqlIdent, SqlLiteral,
    },
    parser::sql::parse_sql,
};
use thiserror::Error;

use crate::{
    check::Relvars,
    errors::InvalidLiteral,
    expr::{FieldProject, ProjectList, RelExpr, Relvar},
    type_limit,
};

use super::{
    check::{SchemaView, TypeChecker, TypingResult},
    errors::{InsertFieldsError, InsertValuesError, TypingError, UnexpectedType, Unresolved},
    expr::Expr,
    parse, type_expr, type_proj, type_select, StatementCtx, StatementSource,
};

pub enum Statement {
    Select(ProjectList),
    DML(DML),
}

pub enum DML {
    Insert(TableInsert),
    Update(TableUpdate),
    Delete(TableDelete),
}

impl DML {
    /// Returns the schema of the table on which this mutation applies
    pub fn table_schema(&self) -> &TableSchema {
        match self {
            Self::Insert(insert) => &insert.table,
            Self::Delete(delete) => &delete.table,
            Self::Update(update) => &update.table,
        }
    }

    /// Returns the id of the table on which this mutation applies
    pub fn table_id(&self) -> TableId {
        self.table_schema().table_id
    }

    /// Returns the name of the table on which this mutation applies
    pub fn table_name(&self) -> Box<str> {
        self.table_schema().table_name.clone()
    }
}

pub struct TableInsert {
    pub table: Arc<TableSchema>,
    pub rows: Box<[ProductValue]>,
}

pub struct TableDelete {
    pub table: Arc<TableSchema>,
    pub filter: Option<Expr>,
}

pub struct TableUpdate {
    pub table: Arc<TableSchema>,
    pub columns: Box<[(ColId, AlgebraicValue)]>,
    pub filter: Option<Expr>,
}

pub struct SetVar {
    pub name: String,
    pub value: AlgebraicValue,
}

pub struct ShowVar {
    pub name: String,
}

/// Type check an INSERT statement
pub fn type_insert(insert: SqlInsert, tx: &impl SchemaView) -> TypingResult<TableInsert> {
    let SqlInsert {
        table: SqlIdent(table_name),
        fields,
        values,
    } = insert;

    let schema = tx
        .schema(&table_name)
        .ok_or_else(|| Unresolved::table(&table_name))
        .map_err(TypingError::from)?;

    // Expect n fields
    let n = schema.columns().len();
    if fields.len() != schema.columns().len() {
        return Err(TypingError::from(InsertFieldsError {
            table: table_name.into_string(),
            nfields: fields.len(),
            ncols: schema.columns().len(),
        }));
    }

    let mut rows = Vec::new();
    for row in values.0 {
        // Expect each row to have n values
        if row.len() != n {
            return Err(TypingError::from(InsertValuesError {
                table: table_name.into_string(),
                values: row.len(),
                fields: n,
            }));
        }
        let mut values = Vec::new();
        for (value, ty) in row.into_iter().zip(
            schema
                .as_ref()
                .columns()
                .iter()
                .map(|ColumnSchema { col_type, .. }| col_type),
        ) {
            match (value, ty) {
                (SqlLiteral::Bool(v), AlgebraicType::Bool) => {
                    values.push(AlgebraicValue::Bool(v));
                }
                (SqlLiteral::Str(v), AlgebraicType::String) => {
                    values.push(AlgebraicValue::String(v));
                }
                (SqlLiteral::Bool(_), _) => {
                    return Err(UnexpectedType::new(&AlgebraicType::Bool, ty).into());
                }
                (SqlLiteral::Str(_), _) => {
                    return Err(UnexpectedType::new(&AlgebraicType::String, ty).into());
                }
                (SqlLiteral::Hex(v), ty) | (SqlLiteral::Num(v), ty) => {
                    values.push(parse(&v, ty).map_err(|_| InvalidLiteral::new(v.into_string(), ty))?);
                }
            }
        }
        rows.push(ProductValue::from(values));
    }
    let into = schema;
    let rows = rows.into_boxed_slice();
    Ok(TableInsert { table: into, rows })
}

/// Type check a DELETE statement
pub fn type_delete(delete: SqlDelete, tx: &impl SchemaView) -> TypingResult<TableDelete> {
    let SqlDelete {
        table: SqlIdent(table_name),
        filter,
    } = delete;
    let from = tx
        .schema(&table_name)
        .ok_or_else(|| Unresolved::table(&table_name))
        .map_err(TypingError::from)?;
    let mut vars = Relvars::default();
    vars.insert(table_name.clone(), from.clone());
    let expr = filter
        .map(|expr| type_expr(&vars, expr, Some(&AlgebraicType::Bool)))
        .transpose()?;
    Ok(TableDelete {
        table: from,
        filter: expr,
    })
}

/// Type check an UPDATE statement
pub fn type_update(update: SqlUpdate, tx: &impl SchemaView) -> TypingResult<TableUpdate> {
    let SqlUpdate {
        table: SqlIdent(table_name),
        assignments,
        filter,
    } = update;
    let schema = tx
        .schema(&table_name)
        .ok_or_else(|| Unresolved::table(&table_name))
        .map_err(TypingError::from)?;
    let mut values = Vec::new();
    for SqlSet(SqlIdent(field), lit) in assignments {
        let ColumnSchema {
            col_pos: col_id,
            col_type: ty,
            ..
        } = schema
            .as_ref()
            .get_column_by_name(&field)
            .ok_or_else(|| Unresolved::field(&table_name, &field))?;
        match (lit, ty) {
            (SqlLiteral::Bool(v), AlgebraicType::Bool) => {
                values.push((*col_id, AlgebraicValue::Bool(v)));
            }
            (SqlLiteral::Str(v), AlgebraicType::String) => {
                values.push((*col_id, AlgebraicValue::String(v)));
            }
            (SqlLiteral::Bool(_), _) => {
                return Err(UnexpectedType::new(&AlgebraicType::Bool, ty).into());
            }
            (SqlLiteral::Str(_), _) => {
                return Err(UnexpectedType::new(&AlgebraicType::String, ty).into());
            }
            (SqlLiteral::Hex(v), ty) | (SqlLiteral::Num(v), ty) => {
                values.push((
                    *col_id,
                    parse(&v, ty).map_err(|_| InvalidLiteral::new(v.into_string(), ty))?,
                ));
            }
        }
    }
    let mut vars = Relvars::default();
    vars.insert(table_name.clone(), schema.clone());
    let values = values.into_boxed_slice();
    let filter = filter
        .map(|expr| type_expr(&vars, expr, Some(&AlgebraicType::Bool)))
        .transpose()?;
    Ok(TableUpdate {
        table: schema,
        columns: values,
        filter,
    })
}

#[derive(Error, Debug)]
#[error("{name} is not a valid system variable")]
pub struct InvalidVar {
    pub name: String,
}

const VAR_ROW_LIMIT: &str = "row_limit";
const VAR_SLOW_QUERY: &str = "slow_ad_hoc_query_ms";
const VAR_SLOW_UPDATE: &str = "slow_tx_update_ms";
const VAR_SLOW_SUB: &str = "slow_subscription_query_ms";

fn is_var_valid(var: &str) -> bool {
    var == VAR_ROW_LIMIT || var == VAR_SLOW_QUERY || var == VAR_SLOW_UPDATE || var == VAR_SLOW_SUB
}

const ST_VAR_NAME: &str = "st_var";
const VALUE_COLUMN: &str = "value";

/// The concept of `SET` only exists in the ast.
/// We translate it here to an `INSERT` on the `st_var` system table.
/// That is:
///
/// ```sql
/// SET var TO ...
/// ```
///
/// is rewritten as
///
/// ```sql
/// INSERT INTO st_var (name, value) VALUES ('var', ...)
/// ```
pub fn type_and_rewrite_set(set: SqlSet, tx: &impl SchemaView) -> TypingResult<TableInsert> {
    let SqlSet(SqlIdent(var_name), lit) = set;
    if !is_var_valid(&var_name) {
        return Err(InvalidVar {
            name: var_name.into_string(),
        }
        .into());
    }

    match lit {
        SqlLiteral::Bool(_) => Err(UnexpectedType::new(&AlgebraicType::U64, &AlgebraicType::Bool).into()),
        SqlLiteral::Str(_) => Err(UnexpectedType::new(&AlgebraicType::U64, &AlgebraicType::String).into()),
        SqlLiteral::Hex(_) => Err(UnexpectedType::new(&AlgebraicType::U64, &AlgebraicType::bytes()).into()),
        SqlLiteral::Num(n) => {
            let table = tx.schema(ST_VAR_NAME).ok_or_else(|| Unresolved::table(ST_VAR_NAME))?;
            let var_name = AlgebraicValue::String(var_name);
            let sum_value = StVarValue::try_from_primitive(
                parse(&n, &AlgebraicType::U64)
                    .map_err(|_| InvalidLiteral::new(n.clone().into_string(), &AlgebraicType::U64))?,
            )
            .map_err(|_| InvalidLiteral::new(n.into_string(), &AlgebraicType::U64))?
            .into();
            Ok(TableInsert {
                table,
                rows: Box::new([ProductValue::from_iter([var_name, sum_value])]),
            })
        }
    }
}

/// The concept of `SHOW` only exists in the ast.
/// We translate it here to a `SELECT` on the `st_var` system table.
/// That is:
///
/// ```sql
/// SHOW var
/// ```
///
/// is rewritten as
///
/// ```sql
/// SELECT value FROM st_var WHERE name = 'var'
/// ```
pub fn type_and_rewrite_show(show: SqlShow, tx: &impl SchemaView) -> TypingResult<ProjectList> {
    let SqlShow(SqlIdent(var_name)) = show;
    if !is_var_valid(&var_name) {
        return Err(InvalidVar {
            name: var_name.into_string(),
        }
        .into());
    }

    let table_schema = tx.schema(ST_VAR_NAME).ok_or_else(|| Unresolved::table(ST_VAR_NAME))?;

    let value_col_ty = table_schema
        .as_ref()
        .get_column(1)
        .map(|ColumnSchema { col_type, .. }| col_type)
        .ok_or_else(|| Unresolved::field(ST_VAR_NAME, VALUE_COLUMN))?;

    // -------------------------------------------
    // SELECT value FROM st_var WHERE name = 'var'
    //                                ^^^^
    // -------------------------------------------
    let var_name_field = Expr::Field(FieldProject {
        table: ST_VAR_NAME.into(),
        // TODO: Avoid hard coding the field position.
        // See `StVarFields` for the schema of `st_var`.
        field: 0,
        ty: AlgebraicType::String,
    });

    // -------------------------------------------
    // SELECT value FROM st_var WHERE name = 'var'
    //                                        ^^^
    // -------------------------------------------
    let var_name_value = Expr::Value(AlgebraicValue::String(var_name), AlgebraicType::String);

    // -------------------------------------------
    // SELECT value FROM st_var WHERE name = 'var'
    //        ^^^^^
    // -------------------------------------------
    let column_list = vec![(
        VALUE_COLUMN.into(),
        FieldProject {
            table: ST_VAR_NAME.into(),
            // TODO: Avoid hard coding the field position.
            // See `StVarFields` for the schema of `st_var`.
            field: 1,
            ty: value_col_ty.clone(),
        },
    )];

    // -------------------------------------------
    // SELECT value FROM st_var WHERE name = 'var'
    //                   ^^^^^^
    // -------------------------------------------
    let relvar = RelExpr::RelVar(Relvar {
        schema: table_schema,
        alias: ST_VAR_NAME.into(),
        delta: None,
    });

    let filter = Expr::BinOp(
        // -------------------------------------------
        // SELECT value FROM st_var WHERE name = 'var'
        //                                    ^^^
        // -------------------------------------------
        BinOp::Eq,
        Box::new(var_name_field),
        Box::new(var_name_value),
    );

    Ok(ProjectList::List(
        vec![RelExpr::Select(Box::new(relvar), filter)],
        column_list,
    ))
}

/// Type-checker for regular `SQL` queries
struct SqlChecker;

impl TypeChecker for SqlChecker {
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
                limit: None,
            } => type_proj(Self::type_from(from, vars, tx)?, project, vars),
            SqlSelect {
                project,
                from,
                filter: None,
                limit: Some(n),
            } => type_limit(type_proj(Self::type_from(from, vars, tx)?, project, vars)?, &n),
            SqlSelect {
                project,
                from,
                filter: Some(expr),
                limit: None,
            } => type_proj(
                type_select(Self::type_from(from, vars, tx)?, expr, vars)?,
                project,
                vars,
            ),
            SqlSelect {
                project,
                from,
                filter: Some(expr),
                limit: Some(n),
            } => type_limit(
                type_proj(
                    type_select(Self::type_from(from, vars, tx)?, expr, vars)?,
                    project,
                    vars,
                )?,
                &n,
            ),
        }
    }
}

pub fn parse_and_type_sql(sql: &str, tx: &impl SchemaView, auth: &AuthCtx) -> TypingResult<Statement> {
    match parse_sql(sql)?.resolve_sender(auth.caller) {
        SqlAst::Select(ast) => Ok(Statement::Select(SqlChecker::type_ast(ast, tx)?)),
        SqlAst::Insert(insert) => Ok(Statement::DML(DML::Insert(type_insert(insert, tx)?))),
        SqlAst::Delete(delete) => Ok(Statement::DML(DML::Delete(type_delete(delete, tx)?))),
        SqlAst::Update(update) => Ok(Statement::DML(DML::Update(type_update(update, tx)?))),
        SqlAst::Set(set) => Ok(Statement::DML(DML::Insert(type_and_rewrite_set(set, tx)?))),
        SqlAst::Show(show) => Ok(Statement::Select(type_and_rewrite_show(show, tx)?)),
    }
}

/// Parse and type check a *general* query into a [StatementCtx].
pub fn compile_sql_stmt<'a>(
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
    let statement = parse_and_type_sql(sql, tx, auth)?;
    Ok(StatementCtx {
        statement,
        sql,
        source: StatementSource::Query,
        planning_time: planning_time.map(|t| t.elapsed()),
    })
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::{identity::AuthCtx, AlgebraicType, ProductType};
    use spacetimedb_schema::def::ModuleDef;

    use crate::check::{
        test_utils::{build_module_def, SchemaViewer},
        SchemaView, TypingResult,
    };

    use super::Statement;

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

    /// A wrapper around [super::parse_and_type_sql] that takes a dummy [AuthCtx]
    fn parse_and_type_sql(sql: &str, tx: &impl SchemaView) -> TypingResult<Statement> {
        super::parse_and_type_sql(sql, tx, &AuthCtx::for_testing())
    }

    #[test]
    fn valid() {
        let tx = SchemaViewer(module_def());

        for sql in [
            "select str from t",
            "select str, arr from t",
            "select t.str, arr from t",
            "select * from t limit 5",
        ] {
            let result = parse_and_type_sql(sql, &tx);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn invalid() {
        let tx = SchemaViewer(module_def());

        for sql in [
            // Unqualified columns in a join
            "select id, str from s join t",
            // Wrong type for limit
            "select * from t limit '5'",
            // Unqualified name in join expression
            "select t.* from t join s on t.u32 = s.u32 where bytes = 0xABCD",
        ] {
            let result = parse_and_type_sql(sql, &tx);
            assert!(result.is_err());
        }
    }
}
