use std::sync::Arc;

use spacetimedb_lib::{AlgebraicType, AlgebraicValue};
use spacetimedb_primitives::ColId;
use spacetimedb_schema::schema::{ColumnSchema, TableSchema};
use spacetimedb_sql_parser::{
    ast::{
        sql::{SqlAst, SqlDelete, SqlInsert, SqlSelect, SqlSet, SqlShow, SqlUpdate},
        SqlIdent, SqlLiteral,
    },
    parser::sql::parse_sql,
};
use thiserror::Error;

use crate::{check::Relvars, expr::Project};

use super::{
    check::{SchemaView, TypeChecker, TypingResult},
    errors::{InsertFieldsError, InsertValuesError, TypingError, UnexpectedType, Unresolved},
    expr::Expr,
    parse, type_expr, type_proj, type_select, StatementCtx, StatementSource,
};

pub enum Statement {
    Select(Project),
    Insert(TableInsert),
    Update(TableUpdate),
    Delete(TableDelete),
    Set(SetVar),
    Show(ShowVar),
}

/// A resolved row of literal values for an insert
pub type Row = Box<[AlgebraicValue]>;

pub struct TableInsert {
    pub into: Arc<TableSchema>,
    pub rows: Box<[Row]>,
}

pub struct TableDelete {
    pub from: Arc<TableSchema>,
    pub expr: Option<Expr>,
}

pub struct TableUpdate {
    pub schema: Arc<TableSchema>,
    pub values: Box<[(ColId, AlgebraicValue)]>,
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
        for (value, ty) in row
            .into_iter()
            .zip(schema.columns().iter().map(|ColumnSchema { col_type, .. }| col_type))
        {
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
                    values.push(parse(v.into_string(), ty)?);
                }
            }
        }
        rows.push(values.into_boxed_slice());
    }
    let into = schema;
    let rows = rows.into_boxed_slice();
    Ok(TableInsert { into, rows })
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
    Ok(TableDelete { from, expr })
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
                values.push((*col_id, parse(v.into_string(), ty)?));
            }
        }
    }
    let mut vars = Relvars::default();
    vars.insert(table_name.clone(), schema.clone());
    let values = values.into_boxed_slice();
    let filter = filter
        .map(|expr| type_expr(&vars, expr, Some(&AlgebraicType::Bool)))
        .transpose()?;
    Ok(TableUpdate { schema, values, filter })
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

pub fn type_set(set: SqlSet) -> TypingResult<SetVar> {
    let SqlSet(SqlIdent(name), lit) = set;
    if !is_var_valid(&name) {
        return Err(InvalidVar {
            name: name.into_string(),
        }
        .into());
    }
    match lit {
        SqlLiteral::Bool(_) => Err(UnexpectedType::new(&AlgebraicType::U64, &AlgebraicType::Bool).into()),
        SqlLiteral::Str(_) => Err(UnexpectedType::new(&AlgebraicType::U64, &AlgebraicType::String).into()),
        SqlLiteral::Hex(_) => Err(UnexpectedType::new(&AlgebraicType::U64, &AlgebraicType::bytes()).into()),
        SqlLiteral::Num(n) => Ok(SetVar {
            name: name.into_string(),
            value: parse(n.into_string(), &AlgebraicType::U64)?,
        }),
    }
}

pub fn type_show(show: SqlShow) -> TypingResult<ShowVar> {
    let SqlShow(SqlIdent(name)) = show;
    if !is_var_valid(&name) {
        return Err(InvalidVar {
            name: name.into_string(),
        }
        .into());
    }
    Ok(ShowVar {
        name: name.into_string(),
    })
}

/// Type-checker for regular `SQL` queries
struct SqlChecker;

impl TypeChecker for SqlChecker {
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

fn parse_and_type_sql(sql: &str, tx: &impl SchemaView) -> TypingResult<Statement> {
    match parse_sql(sql)? {
        SqlAst::Insert(insert) => Ok(Statement::Insert(type_insert(insert, tx)?)),
        SqlAst::Delete(delete) => Ok(Statement::Delete(type_delete(delete, tx)?)),
        SqlAst::Update(update) => Ok(Statement::Update(type_update(update, tx)?)),
        SqlAst::Select(ast) => Ok(Statement::Select(SqlChecker::type_ast(ast, tx)?)),
        SqlAst::Set(set) => Ok(Statement::Set(type_set(set)?)),
        SqlAst::Show(show) => Ok(Statement::Show(type_show(show)?)),
    }
}

/// Parse and type check a *general* query into a [StatementCtx].
pub fn compile_sql_stmt<'a>(sql: &'a str, tx: &impl SchemaView) -> TypingResult<StatementCtx<'a>> {
    let statement = parse_and_type_sql(sql, tx)?;
    Ok(StatementCtx {
        statement,
        sql,
        source: StatementSource::Query,
    })
}

#[cfg(test)]
mod tests {
    use spacetimedb_lib::{AlgebraicType, ProductType};
    use spacetimedb_schema::def::ModuleDef;

    use crate::{
        check::test_utils::{build_module_def, SchemaViewer},
        statement::parse_and_type_sql,
    };

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
            "select str from t",
            "select str, arr from t",
            "select t.str, arr from t",
        ] {
            let result = parse_and_type_sql(sql, &tx);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn invalid() {
        let tx = SchemaViewer(module_def());

        // Unqualified columns in a join
        let sql = "select id, str from s join t";
        let result = parse_and_type_sql(sql, &tx);
        assert!(result.is_err());
    }
}
