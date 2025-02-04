use std::{collections::HashSet, ops::Deref};

use crate::statement::Statement;
use check::{Relvars, TypingResult};
use errors::{DuplicateName, InvalidLiteral, InvalidOp, InvalidWildcard, UnexpectedType, Unresolved};
use expr::{Expr, FieldProject, ProjectList, ProjectName, RelExpr};
use spacetimedb_lib::{from_hex_pad, Address, AlgebraicType, AlgebraicValue, Identity};
use spacetimedb_schema::schema::ColumnSchema;
use spacetimedb_sql_parser::ast::{self, BinOp, ProjectElem, SqlExpr, SqlIdent, SqlLiteral};

pub mod check;
pub mod errors;
pub mod expr;
pub mod statement;

/// Type check and lower a [SqlExpr]
pub(crate) fn type_select(input: RelExpr, expr: SqlExpr, vars: &Relvars) -> TypingResult<RelExpr> {
    Ok(RelExpr::Select(
        Box::new(input),
        type_expr(vars, expr, Some(&AlgebraicType::Bool))?,
    ))
}

/// Type check and lower a [ast::Project]
pub(crate) fn type_proj(input: RelExpr, proj: ast::Project, vars: &Relvars) -> TypingResult<ProjectList> {
    match proj {
        ast::Project::Star(None) if input.nfields() > 1 => Err(InvalidWildcard::Join.into()),
        ast::Project::Star(None) => Ok(ProjectList::Name(ProjectName::None(input))),
        ast::Project::Star(Some(SqlIdent(var))) if input.has_field(&var) => {
            Ok(ProjectList::Name(ProjectName::Some(input, var)))
        }
        ast::Project::Star(Some(SqlIdent(var))) => Err(Unresolved::var(&var).into()),
        ast::Project::Exprs(elems) => {
            let mut projections = vec![];
            let mut names = HashSet::new();

            for ProjectElem(expr, SqlIdent(alias)) in elems {
                if !names.insert(alias.clone()) {
                    return Err(DuplicateName(alias.into_string()).into());
                }

                if let Expr::Field(p) = type_expr(vars, expr.into(), None)? {
                    projections.push((alias, p));
                }
            }

            Ok(ProjectList::List(input, projections))
        }
    }
}

/// Type check and lower a [SqlExpr] into a logical [Expr].
pub(crate) fn type_expr(vars: &Relvars, expr: SqlExpr, expected: Option<&AlgebraicType>) -> TypingResult<Expr> {
    match (expr, expected) {
        (SqlExpr::Lit(SqlLiteral::Bool(v)), None | Some(AlgebraicType::Bool)) => Ok(Expr::bool(v)),
        (SqlExpr::Lit(SqlLiteral::Bool(_)), Some(ty)) => Err(UnexpectedType::new(&AlgebraicType::Bool, ty).into()),
        (SqlExpr::Lit(SqlLiteral::Str(v)), None | Some(AlgebraicType::String)) => Ok(Expr::str(v)),
        (SqlExpr::Lit(SqlLiteral::Str(_)), Some(ty)) => Err(UnexpectedType::new(&AlgebraicType::String, ty).into()),
        (SqlExpr::Lit(SqlLiteral::Num(_) | SqlLiteral::Hex(_)), None) => Err(Unresolved::Literal.into()),
        (SqlExpr::Lit(SqlLiteral::Num(v) | SqlLiteral::Hex(v)), Some(ty)) => {
            Ok(Expr::Value(parse(v.into_string(), ty)?, ty.clone()))
        }
        (SqlExpr::Field(SqlIdent(table), SqlIdent(field)), None) => {
            let table_type = vars.deref().get(&table).ok_or_else(|| Unresolved::var(&table))?;
            let ColumnSchema { col_pos, col_type, .. } = table_type
                .get_column_by_name(&field)
                .ok_or_else(|| Unresolved::var(&field))?;
            Ok(Expr::Field(FieldProject {
                table,
                field: col_pos.idx(),
                ty: col_type.clone(),
            }))
        }
        (SqlExpr::Field(SqlIdent(table), SqlIdent(field)), Some(ty)) => {
            let table_type = vars.deref().get(&table).ok_or_else(|| Unresolved::var(&table))?;
            let ColumnSchema { col_pos, col_type, .. } = table_type
                .as_ref()
                .get_column_by_name(&field)
                .ok_or_else(|| Unresolved::var(&field))?;
            if col_type != ty {
                return Err(UnexpectedType::new(col_type, ty).into());
            }
            Ok(Expr::Field(FieldProject {
                table,
                field: col_pos.idx(),
                ty: col_type.clone(),
            }))
        }
        (SqlExpr::Log(a, b, op), None | Some(AlgebraicType::Bool)) => {
            let a = type_expr(vars, *a, Some(&AlgebraicType::Bool))?;
            let b = type_expr(vars, *b, Some(&AlgebraicType::Bool))?;
            Ok(Expr::LogOp(op, Box::new(a), Box::new(b)))
        }
        (SqlExpr::Bin(a, b, op), None | Some(AlgebraicType::Bool)) => match (*a, *b) {
            (a, b @ SqlExpr::Lit(_)) | (b @ SqlExpr::Lit(_), a) | (a, b) => {
                let a = type_expr(vars, a, None)?;
                let b = type_expr(vars, b, Some(a.ty()))?;
                if !op_supports_type(op, a.ty()) {
                    return Err(InvalidOp::new(op, a.ty()).into());
                }
                Ok(Expr::BinOp(op, Box::new(a), Box::new(b)))
            }
        },
        (SqlExpr::Bin(..) | SqlExpr::Log(..), Some(ty)) => Err(UnexpectedType::new(&AlgebraicType::Bool, ty).into()),
        (SqlExpr::Var(_), _) => unreachable!(),
    }
}

/// Is this type compatible with this binary operator?
fn op_supports_type(_op: BinOp, t: &AlgebraicType) -> bool {
    t.is_bool() || t.is_integer() || t.is_float() || t.is_string() || t.is_bytes() || t.is_identity() || t.is_address()
}

/// Parses a source text literal as a particular type
pub(crate) fn parse(value: String, ty: &AlgebraicType) -> Result<AlgebraicValue, InvalidLiteral> {
    match ty {
        AlgebraicType::I8 => value
            .parse::<i8>()
            .map(AlgebraicValue::I8)
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::U8 => value
            .parse::<u8>()
            .map(AlgebraicValue::U8)
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::I16 => value
            .parse::<i16>()
            .map(AlgebraicValue::I16)
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::U16 => value
            .parse::<u16>()
            .map(AlgebraicValue::U16)
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::I32 => value
            .parse::<i32>()
            .map(AlgebraicValue::I32)
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::U32 => value
            .parse::<u32>()
            .map(AlgebraicValue::U32)
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::I64 => value
            .parse::<i64>()
            .map(AlgebraicValue::I64)
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::U64 => value
            .parse::<u64>()
            .map(AlgebraicValue::U64)
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::F32 => value
            .parse::<f32>()
            .map(|value| AlgebraicValue::F32(value.into()))
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::F64 => value
            .parse::<f64>()
            .map(|value| AlgebraicValue::F64(value.into()))
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::I128 => value
            .parse::<i128>()
            .map(|value| AlgebraicValue::I128(value.into()))
            .map_err(|_| InvalidLiteral::new(value, ty)),
        AlgebraicType::U128 => value
            .parse::<u128>()
            .map(|value| AlgebraicValue::U128(value.into()))
            .map_err(|_| InvalidLiteral::new(value, ty)),
        t if t.is_bytes() => from_hex_pad::<Vec<u8>, _>(&value)
            .map(|value| AlgebraicValue::Bytes(value.into_boxed_slice()))
            .map_err(|_| InvalidLiteral::new(value, ty)),
        t if t.is_identity() => Identity::from_hex(&value)
            .map(AlgebraicValue::from)
            .map_err(|_| InvalidLiteral::new(value, ty)),
        t if t.is_address() => Address::from_hex(&value)
            .map(AlgebraicValue::from)
            .map_err(|_| InvalidLiteral::new(value, ty)),
        _ => Err(InvalidLiteral::new(value, ty)),
    }
}

/// The source of a statement
pub enum StatementSource {
    Subscription,
    Query,
}

/// A statement context.
///
/// This is a wrapper around a statement, its source, and the original SQL text.
pub struct StatementCtx<'a> {
    pub statement: Statement,
    pub sql: &'a str,
    pub source: StatementSource,
}
