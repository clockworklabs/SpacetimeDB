use std::{collections::HashSet, ops::Deref, str::FromStr};

use crate::statement::Statement;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::Context;
use bigdecimal::BigDecimal;
use bigdecimal::ToPrimitive;
use check::{Relvars, TypingResult};
use errors::{DuplicateName, InvalidLiteral, InvalidOp, InvalidWildcard, UnexpectedType, Unresolved};
use ethnum::i256;
use ethnum::u256;
use expr::AggType;
use expr::{Expr, FieldProject, ProjectList, ProjectName, RelExpr};
use spacetimedb_lib::ser::Serialize;
use spacetimedb_lib::Timestamp;
use spacetimedb_lib::{from_hex_pad, AlgebraicType, AlgebraicValue, ConnectionId, Identity};
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sats::algebraic_value::ser::ValueSerializer;
use spacetimedb_schema::schema::ColumnSchema;
use spacetimedb_sql_parser::ast::{self, BinOp, ProjectElem, SqlExpr, SqlIdent, SqlLiteral};

pub mod check;
pub mod errors;
pub mod expr;
pub mod rls;
pub mod statement;

/// Type check and lower a [SqlExpr]
pub(crate) fn type_select(input: RelExpr, expr: SqlExpr, vars: &Relvars) -> TypingResult<RelExpr> {
    Ok(RelExpr::Select(
        Box::new(input),
        type_expr(vars, expr, Some(&AlgebraicType::Bool))?,
    ))
}

/// Type check a LIMIT clause
pub(crate) fn type_limit(input: ProjectList, limit: &str) -> TypingResult<ProjectList> {
    Ok(
        parse_int(limit, AlgebraicType::U64, BigDecimal::to_u64, AlgebraicValue::U64)
            .map_err(|_| InvalidLiteral::new(limit.to_owned(), &AlgebraicType::U64))
            .and_then(|n| {
                n.into_u64()
                    .map_err(|_| InvalidLiteral::new(limit.to_owned(), &AlgebraicType::U64))
            })
            .map(|n| ProjectList::Limit(Box::new(input), n))?,
    )
}

/// Type check and lower a [ast::Project]
pub(crate) fn type_proj(input: RelExpr, proj: ast::Project, vars: &Relvars) -> TypingResult<ProjectList> {
    match proj {
        ast::Project::Star(None) if input.nfields() > 1 => Err(InvalidWildcard::Join.into()),
        ast::Project::Star(None) => Ok(ProjectList::Name(vec![ProjectName::None(input)])),
        ast::Project::Star(Some(SqlIdent(var))) if input.has_field(&var) => {
            Ok(ProjectList::Name(vec![ProjectName::Some(input, var)]))
        }
        ast::Project::Star(Some(SqlIdent(var))) => Err(Unresolved::var(&var).into()),
        ast::Project::Count(SqlIdent(alias)) => {
            Ok(ProjectList::Agg(vec![input], AggType::Count, alias, AlgebraicType::U64))
        }
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

            Ok(ProjectList::List(vec![input], projections))
        }
    }
}

/// Type check and lower a [SqlExpr] into a logical [Expr].
pub(crate) fn type_expr(vars: &Relvars, expr: SqlExpr, expected: Option<&AlgebraicType>) -> TypingResult<Expr> {
    match (expr, expected) {
        (SqlExpr::Lit(SqlLiteral::Bool(v)), None | Some(AlgebraicType::Bool)) => Ok(Expr::bool(v)),
        (SqlExpr::Lit(SqlLiteral::Bool(_)), Some(ty)) => Err(UnexpectedType::new(&AlgebraicType::Bool, ty).into()),
        (SqlExpr::Lit(SqlLiteral::Str(_) | SqlLiteral::Num(_) | SqlLiteral::Hex(_)), None) => {
            Err(Unresolved::Literal.into())
        }
        (SqlExpr::Lit(SqlLiteral::Str(v) | SqlLiteral::Num(v) | SqlLiteral::Hex(v)), Some(ty)) => Ok(Expr::Value(
            parse(&v, ty).map_err(|_| InvalidLiteral::new(v.into_string(), ty))?,
            ty.clone(),
        )),
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
        (SqlExpr::Bin(a, b, op), None | Some(AlgebraicType::Bool)) if matches!(&*a, SqlExpr::Lit(_)) => {
            let b = type_expr(vars, *b, None)?;
            let a = type_expr(vars, *a, Some(b.ty()))?;
            if !op_supports_type(op, a.ty()) {
                return Err(InvalidOp::new(op, a.ty()).into());
            }
            Ok(Expr::BinOp(op, Box::new(a), Box::new(b)))
        }
        (SqlExpr::Bin(a, b, op), None | Some(AlgebraicType::Bool)) => {
            let a = type_expr(vars, *a, None)?;
            let b = type_expr(vars, *b, Some(a.ty()))?;
            if !op_supports_type(op, a.ty()) {
                return Err(InvalidOp::new(op, a.ty()).into());
            }
            Ok(Expr::BinOp(op, Box::new(a), Box::new(b)))
        }
        (SqlExpr::Bin(..) | SqlExpr::Log(..), Some(ty)) => Err(UnexpectedType::new(&AlgebraicType::Bool, ty).into()),
        // Both unqualified names as well as parameters are syntactic constructs.
        // Unqualified names are qualified and parameters are resolved before type checking.
        (SqlExpr::Var(_) | SqlExpr::Param(_), _) => unreachable!(),
    }
}

/// Is this type compatible with this binary operator?
fn op_supports_type(_op: BinOp, t: &AlgebraicType) -> bool {
    t.is_bool()
        || t.is_integer()
        || t.is_float()
        || t.is_string()
        || t.is_bytes()
        || t.is_identity()
        || t.is_connection_id()
        || t.is_timestamp()
}

/// Parse an integer literal into an [AlgebraicValue]
fn parse_int<Int, Val, ToInt, ToVal>(
    literal: &str,
    ty: AlgebraicType,
    to_int: ToInt,
    to_val: ToVal,
) -> anyhow::Result<AlgebraicValue>
where
    Int: Into<Val>,
    ToInt: FnOnce(&BigDecimal) -> Option<Int>,
    ToVal: FnOnce(Val) -> AlgebraicValue,
{
    // Why are we using an arbitrary precision type?
    // For scientific notation as well as i256 and u256.
    BigDecimal::from_str(literal)
        .ok()
        .filter(|decimal| decimal.is_integer())
        .ok_or_else(|| anyhow!("{literal} is not an integer"))
        .map(|decimal| to_int(&decimal).map(|val| val.into()).map(to_val))
        .transpose()
        .ok_or_else(|| anyhow!("{literal} is out of bounds for type {}", fmt_algebraic_type(&ty)))?
}

/// Parse a floating point literal into an [AlgebraicValue]
fn parse_float<Float, Value, ToFloat, ToValue>(
    literal: &str,
    ty: AlgebraicType,
    to_float: ToFloat,
    to_value: ToValue,
) -> anyhow::Result<AlgebraicValue>
where
    Float: Into<Value>,
    ToFloat: FnOnce(&BigDecimal) -> Option<Float>,
    ToValue: FnOnce(Value) -> AlgebraicValue,
{
    BigDecimal::from_str(literal)
        .ok()
        .and_then(|decimal| to_float(&decimal))
        .map(|value| value.into())
        .map(to_value)
        .ok_or_else(|| anyhow!("{literal} is not a valid {}", fmt_algebraic_type(&ty)))
}

/// Parses a source text literal as a particular type
pub(crate) fn parse(value: &str, ty: &AlgebraicType) -> anyhow::Result<AlgebraicValue> {
    let to_timestamp = || {
        Timestamp::parse_from_rfc3339(value)?
            .serialize(ValueSerializer)
            .with_context(|| "Could not parse timestamp")
    };
    let to_bytes = || {
        from_hex_pad::<Vec<u8>, _>(value)
            .map(|v| v.into_boxed_slice())
            .map(AlgebraicValue::Bytes)
            .with_context(|| "Could not parse hex value")
    };
    let to_identity = || {
        Identity::from_hex(value)
            .map(AlgebraicValue::from)
            .with_context(|| "Could not parse identity")
    };
    let to_connection_id = || {
        ConnectionId::from_hex(value)
            .map(AlgebraicValue::from)
            .with_context(|| "Could not parse connection id")
    };
    let to_i256 = |decimal: &BigDecimal| {
        i256::from_str_radix(
            // Convert to decimal notation
            &decimal.to_plain_string(),
            10,
        )
        .ok()
    };
    let to_u256 = |decimal: &BigDecimal| {
        u256::from_str_radix(
            // Convert to decimal notation
            &decimal.to_plain_string(),
            10,
        )
        .ok()
    };
    match ty {
        AlgebraicType::I8 => parse_int(
            // Parse literal as I8
            value,
            AlgebraicType::I8,
            BigDecimal::to_i8,
            AlgebraicValue::I8,
        ),
        AlgebraicType::U8 => parse_int(
            // Parse literal as U8
            value,
            AlgebraicType::U8,
            BigDecimal::to_u8,
            AlgebraicValue::U8,
        ),
        AlgebraicType::I16 => parse_int(
            // Parse literal as I16
            value,
            AlgebraicType::I16,
            BigDecimal::to_i16,
            AlgebraicValue::I16,
        ),
        AlgebraicType::U16 => parse_int(
            // Parse literal as U16
            value,
            AlgebraicType::U16,
            BigDecimal::to_u16,
            AlgebraicValue::U16,
        ),
        AlgebraicType::I32 => parse_int(
            // Parse literal as I32
            value,
            AlgebraicType::I32,
            BigDecimal::to_i32,
            AlgebraicValue::I32,
        ),
        AlgebraicType::U32 => parse_int(
            // Parse literal as U32
            value,
            AlgebraicType::U32,
            BigDecimal::to_u32,
            AlgebraicValue::U32,
        ),
        AlgebraicType::I64 => parse_int(
            // Parse literal as I64
            value,
            AlgebraicType::I64,
            BigDecimal::to_i64,
            AlgebraicValue::I64,
        ),
        AlgebraicType::U64 => parse_int(
            // Parse literal as U64
            value,
            AlgebraicType::U64,
            BigDecimal::to_u64,
            AlgebraicValue::U64,
        ),
        AlgebraicType::F32 => parse_float(
            // Parse literal as F32
            value,
            AlgebraicType::F32,
            BigDecimal::to_f32,
            AlgebraicValue::F32,
        ),
        AlgebraicType::F64 => parse_float(
            // Parse literal as F64
            value,
            AlgebraicType::F64,
            BigDecimal::to_f64,
            AlgebraicValue::F64,
        ),
        AlgebraicType::I128 => parse_int(
            // Parse literal as I128
            value,
            AlgebraicType::I128,
            BigDecimal::to_i128,
            AlgebraicValue::I128,
        ),
        AlgebraicType::U128 => parse_int(
            // Parse literal as U128
            value,
            AlgebraicType::U128,
            BigDecimal::to_u128,
            AlgebraicValue::U128,
        ),
        AlgebraicType::I256 => parse_int(
            // Parse literal as I256
            value,
            AlgebraicType::I256,
            to_i256,
            AlgebraicValue::I256,
        ),
        AlgebraicType::U256 => parse_int(
            // Parse literal as U256
            value,
            AlgebraicType::U256,
            to_u256,
            AlgebraicValue::U256,
        ),
        AlgebraicType::String => Ok(AlgebraicValue::String(value.into())),
        t if t.is_timestamp() => to_timestamp(),
        t if t.is_bytes() => to_bytes(),
        t if t.is_identity() => to_identity(),
        t if t.is_connection_id() => to_connection_id(),
        t => bail!("Literal values for type {} are not supported", fmt_algebraic_type(t)),
    }
}

/// The source of a statement
#[derive(Debug, Clone, Copy)]
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
    pub planning_time: Option<std::time::Duration>,
}
