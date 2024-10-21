use std::sync::Arc;

use spacetimedb_lib::AlgebraicValue;
use spacetimedb_primitives::ColId;
use spacetimedb_schema::schema::{ColumnSchema, TableSchema};
use spacetimedb_sql_parser::{
    ast::{
        sql::{QueryAst, SqlAst, SqlDelete, SqlInsert, SqlSelect, SqlSet, SqlSetOp, SqlShow, SqlUpdate},
        SqlIdent, SqlLiteral,
    },
    parser::sql::parse_sql,
};
use thiserror::Error;

use super::{
    assert_eq_types,
    bind::{SchemaView, TypeChecker, TypingResult},
    errors::{InsertFieldsError, InsertValuesError, TypingError, UnexpectedType, Unresolved, Unsupported},
    expr::{Expr, RelExpr},
    parse,
    ty::{TyCtx, TyEnv, TyId, Type},
    type_expr, type_proj, type_select,
};

pub enum Stmt {
    Select(RelExpr),
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
pub fn type_insert(ctx: &mut TyCtx, insert: SqlInsert, tx: &impl SchemaView) -> TypingResult<TableInsert> {
    let SqlInsert {
        table: SqlIdent { name, case_sensitive },
        fields,
        values,
    } = insert;

    let schema = tx
        .schema(&name, case_sensitive)
        .ok_or_else(|| Unresolved::table(&name))
        .map_err(TypingError::from)?;

    // Expect n fields
    let n = schema.columns().len();
    if fields.len() != schema.columns().len() {
        return Err(TypingError::from(InsertFieldsError {
            table: name,
            nfields: fields.len(),
            ncols: schema.columns().len(),
        }));
    }

    let mut types = Vec::new();
    for ColumnSchema { col_type, .. } in schema.columns() {
        let id = ctx.add(Type::Alg(col_type.clone()));
        types.push(id);
    }

    let mut rows = Vec::new();
    for row in values.0 {
        // Expect each row to have n values
        if row.len() != n {
            return Err(TypingError::from(InsertValuesError {
                table: name,
                values: row.len(),
                fields: n,
            }));
        }
        let mut values = Vec::new();
        for (i, v) in row.into_iter().enumerate() {
            match (v, types[i]) {
                (SqlLiteral::Bool(v), TyId::BOOL) => {
                    values.push(AlgebraicValue::Bool(v));
                }
                (SqlLiteral::Str(v), TyId::STR) => {
                    values.push(AlgebraicValue::String(v.into_boxed_str()));
                }
                (SqlLiteral::Bool(_), id) => {
                    return Err(UnexpectedType::new(&ctx.bool(), &id.try_with_ctx(ctx)?).into());
                }
                (SqlLiteral::Str(_), id) => {
                    return Err(UnexpectedType::new(&ctx.str(), &id.try_with_ctx(ctx)?).into());
                }
                (SqlLiteral::Hex(v), id) | (SqlLiteral::Num(v), id) => {
                    let ty = id.try_with_ctx(ctx)?;
                    values.push(parse(v, ty)?);
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
pub fn type_delete(ctx: &mut TyCtx, delete: SqlDelete, tx: &impl SchemaView) -> TypingResult<TableDelete> {
    let SqlDelete {
        table: SqlIdent { name, case_sensitive },
        filter,
    } = delete;
    let schema = tx
        .schema(&name, case_sensitive)
        .ok_or_else(|| Unresolved::table(&name))
        .map_err(TypingError::from)?;

    let table_name = ctx.gen_symbol(name);

    let mut types = Vec::new();
    let mut env = TyEnv::default();

    for ColumnSchema { col_name, col_type, .. } in schema.columns() {
        let ty = Type::Alg(col_type.clone());
        let id = ctx.add(ty);
        let name = ctx.gen_symbol(col_name);
        env.add(name, id);
        types.push((name, id));
    }

    let ty = Type::Var(types.into_boxed_slice());
    let ty = ctx.add(ty);
    env.add(table_name, ty);

    let from = schema;
    let expr = filter
        .map(|expr| type_expr(ctx, &env, expr, Some(TyId::BOOL)))
        .transpose()?;
    Ok(TableDelete { from, expr })
}

/// Type check an UPDATE statement
pub fn type_update(ctx: &mut TyCtx, update: SqlUpdate, tx: &impl SchemaView) -> TypingResult<TableUpdate> {
    let SqlUpdate {
        table,
        assignments,
        filter,
    } = update;
    let schema = tx
        .schema(&table.name, table.case_sensitive)
        .ok_or_else(|| Unresolved::table(&table.name))
        .map_err(TypingError::from)?;
    let mut env = TyEnv::default();
    for ColumnSchema { col_name, col_type, .. } in schema.columns() {
        let id = ctx.add(Type::Alg(col_type.clone()));
        let name = ctx.gen_symbol(col_name);
        env.add(name, id);
    }
    let mut values = Vec::new();
    for SqlSet(field, lit) in assignments {
        let col_id = schema
            .get_column_id_by_name(&field.name)
            .ok_or_else(|| Unresolved::field(&table.name, &field.name))?;
        let field_name = ctx
            .get_symbol(&field.name)
            .ok_or_else(|| Unresolved::field(&table.name, &field.name))?;
        let ty = env
            .find(field_name)
            .ok_or_else(|| Unresolved::field(&table.name, &field.name))?;
        match (lit, ty) {
            (SqlLiteral::Bool(v), TyId::BOOL) => {
                values.push((col_id, AlgebraicValue::Bool(v)));
            }
            (SqlLiteral::Str(v), TyId::STR) => {
                values.push((col_id, AlgebraicValue::String(v.into_boxed_str())));
            }
            (SqlLiteral::Bool(_), id) => {
                return Err(UnexpectedType::new(&ctx.bool(), &id.try_with_ctx(ctx)?).into());
            }
            (SqlLiteral::Str(_), id) => {
                return Err(UnexpectedType::new(&ctx.str(), &id.try_with_ctx(ctx)?).into());
            }
            (SqlLiteral::Hex(v), id) | (SqlLiteral::Num(v), id) => {
                let ty = id.try_with_ctx(ctx)?;
                values.push((col_id, parse(v, ty)?));
            }
        }
    }
    let values = values.into_boxed_slice();
    let filter = filter
        .map(|expr| type_expr(ctx, &env, expr, Some(TyId::BOOL)))
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

pub fn type_set(ctx: &TyCtx, set: SqlSet) -> TypingResult<SetVar> {
    let SqlSet(SqlIdent { name, .. }, lit) = set;
    if !is_var_valid(&name) {
        return Err(InvalidVar { name }.into());
    }
    match lit {
        SqlLiteral::Bool(_) => Err(UnexpectedType::new(&ctx.u64(), &ctx.bool()).into()),
        SqlLiteral::Str(_) => Err(UnexpectedType::new(&ctx.u64(), &ctx.str()).into()),
        SqlLiteral::Hex(_) => Err(UnexpectedType::new(&ctx.u64(), &ctx.bytes()).into()),
        SqlLiteral::Num(n) => Ok(SetVar {
            name,
            value: parse(n, ctx.u64())?,
        }),
    }
}

pub fn type_show(show: SqlShow) -> TypingResult<ShowVar> {
    let SqlShow(SqlIdent { name, .. }) = show;
    if !is_var_valid(&name) {
        return Err(InvalidVar { name }.into());
    }
    Ok(ShowVar { name })
}

/// Type-checker for regular `SQL` queries
struct SqlChecker;

impl TypeChecker for SqlChecker {
    type Ast = QueryAst;
    type Set = SqlSetOp;

    fn type_ast(ctx: &mut TyCtx, ast: Self::Ast, tx: &impl SchemaView) -> TypingResult<RelExpr> {
        let QueryAst { query, order, limit } = ast;
        if !order.is_empty() {
            return Err(Unsupported::OrderBy.into());
        }
        if limit.is_some() {
            return Err(Unsupported::Limit.into());
        }
        Self::type_set(ctx, query, tx)
    }

    fn type_set(ctx: &mut TyCtx, ast: Self::Set, tx: &impl SchemaView) -> TypingResult<RelExpr> {
        match ast {
            SqlSetOp::Union(a, b, true) => {
                let a = Self::type_set(ctx, *a, tx)?;
                let b = Self::type_set(ctx, *b, tx)?;
                assert_eq_types(ctx, a.ty_id(), b.ty_id())?;
                Ok(RelExpr::Union(Box::new(a), Box::new(b)))
            }
            SqlSetOp::Union(a, b, false) => {
                let a = Self::type_set(ctx, *a, tx)?;
                let b = Self::type_set(ctx, *b, tx)?;
                assert_eq_types(ctx, a.ty_id(), b.ty_id())?;
                Ok(RelExpr::Dedup(Box::new(RelExpr::Union(Box::new(a), Box::new(b)))))
            }
            SqlSetOp::Minus(a, b, true) => {
                let a = Self::type_set(ctx, *a, tx)?;
                let b = Self::type_set(ctx, *b, tx)?;
                assert_eq_types(ctx, a.ty_id(), b.ty_id())?;
                Ok(RelExpr::Minus(Box::new(a), Box::new(b)))
            }
            SqlSetOp::Minus(a, b, false) => {
                let a = Self::type_set(ctx, *a, tx)?;
                let b = Self::type_set(ctx, *b, tx)?;
                assert_eq_types(ctx, a.ty_id(), b.ty_id())?;
                Ok(RelExpr::Dedup(Box::new(RelExpr::Minus(Box::new(a), Box::new(b)))))
            }
            SqlSetOp::Query(ast) => Self::type_ast(ctx, *ast, tx),
            SqlSetOp::Select(SqlSelect {
                project,
                distinct: false,
                from,
                filter: None,
            }) => {
                let (input, alias) = Self::type_from(ctx, from, tx)?;
                type_proj(ctx, input, alias, project)
            }
            SqlSetOp::Select(SqlSelect {
                project,
                distinct: true,
                from,
                filter: None,
            }) => {
                let (input, alias) = Self::type_from(ctx, from, tx)?;
                Ok(RelExpr::Dedup(Box::new(type_proj(ctx, input, alias, project)?)))
            }
            SqlSetOp::Select(SqlSelect {
                project,
                distinct: false,
                from,
                filter: Some(expr),
            }) => {
                let (from, alias) = Self::type_from(ctx, from, tx)?;
                let input = type_select(ctx, from, alias, expr)?;
                type_proj(ctx, input, alias, project)
            }
            SqlSetOp::Select(SqlSelect {
                project,
                distinct: true,
                from,
                filter: Some(expr),
            }) => {
                let (from, alias) = Self::type_from(ctx, from, tx)?;
                let input = type_select(ctx, from, alias, expr)?;
                Ok(RelExpr::Dedup(Box::new(type_proj(ctx, input, alias, project)?)))
            }
        }
    }
}

pub fn parse_and_type_sql(sql: &str, tx: &impl SchemaView) -> TypingResult<Stmt> {
    match parse_sql(sql)? {
        SqlAst::Insert(insert) => Ok(Stmt::Insert(type_insert(&mut TyCtx::default(), insert, tx)?)),
        SqlAst::Delete(delete) => Ok(Stmt::Delete(type_delete(&mut TyCtx::default(), delete, tx)?)),
        SqlAst::Update(update) => Ok(Stmt::Update(type_update(&mut TyCtx::default(), update, tx)?)),
        SqlAst::Query(ast) => Ok(Stmt::Select(SqlChecker::type_ast(&mut TyCtx::default(), ast, tx)?)),
        SqlAst::Set(set) => Ok(Stmt::Set(type_set(&TyCtx::default(), set)?)),
        SqlAst::Show(show) => Ok(Stmt::Show(type_show(show)?)),
    }
}