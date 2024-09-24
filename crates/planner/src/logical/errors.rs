use spacetimedb_sql_parser::{ast::BinOp, parser::errors::SqlParseError};
use thiserror::Error;

use super::{
    stmt::InvalidVar,
    ty::{InvalidTyId, TypeWithCtx},
};

#[derive(Error, Debug)]
pub enum ConstraintViolation {
    #[error("(expected) {expected} != {inferred} (inferred)")]
    Eq { expected: String, inferred: String },
    #[error("`{ty}` is not a numeric type")]
    Num { ty: String },
    #[error("`{ty}` cannot be interpreted as a byte array")]
    Hex { ty: String },
    #[error("`{expr}` cannot be parsed as type `{ty}`")]
    Lit { expr: String, ty: String },
    #[error("The binary operator `{op}` does not support type `{ty}`")]
    Bin { op: BinOp, ty: String },
}

impl ConstraintViolation {
    // Types are not equal
    pub fn eq(expected: TypeWithCtx<'_>, inferred: TypeWithCtx<'_>) -> Self {
        Self::Eq {
            expected: expected.to_string(),
            inferred: inferred.to_string(),
        }
    }

    // Not a numeric type
    pub fn num(ty: TypeWithCtx<'_>) -> Self {
        Self::Num { ty: ty.to_string() }
    }

    // Not a type that can be compared to a hex value
    pub fn hex(ty: TypeWithCtx<'_>) -> Self {
        Self::Hex { ty: ty.to_string() }
    }

    // This literal expression cannot be parsed as this type
    pub fn lit(v: &str, ty: TypeWithCtx<'_>) -> Self {
        Self::Lit {
            expr: v.to_string(),
            ty: ty.to_string(),
        }
    }

    // This operator does not support this type
    pub fn bin(op: BinOp, ty: TypeWithCtx<'_>) -> Self {
        Self::Bin { op, ty: ty.to_string() }
    }
}

#[derive(Error, Debug)]
pub enum Unresolved {
    #[error("Cannot resolve `{0}`")]
    Var(String),
    #[error("Cannot resolve table `{0}`")]
    Table(String),
    #[error("Cannot resolve field `{1}` in `{0}`")]
    Field(String, String),
    #[error("Cannot resolve type for literal expression")]
    Literal,
}

impl Unresolved {
    /// Cannot resolve name
    pub fn var(name: &str) -> Self {
        Self::Var(name.to_string())
    }

    /// Cannot resolve table name
    pub fn table(name: &str) -> Self {
        Self::Table(name.to_string())
    }

    /// Cannot resolve field name within table
    pub fn field(table: &str, field: &str) -> Self {
        Self::Field(table.to_string(), field.to_string())
    }
}

#[derive(Error, Debug)]
pub enum Unsupported {
    #[error("Subscriptions must return a single table type")]
    SubReturnType,
    #[error("Unsupported expression in projection")]
    ProjectExpr,
    #[error("Unqualified column projections are not supported")]
    UnqualifiedProjectExpr,
    #[error("ORDER BY is not supported")]
    OrderBy,
    #[error("LIMIT is not supported")]
    Limit,
}

// TODO: It might be better to return the missing/extra fields
#[derive(Error, Debug)]
#[error("Inserting a row with {values} values into `{table}` which has {fields} fields")]
pub struct InsertError {
    pub table: String,
    pub values: usize,
    pub fields: usize,
}

#[derive(Error, Debug)]
pub enum TypingError {
    #[error(transparent)]
    Unsupported(#[from] Unsupported),
    #[error(transparent)]
    Constraint(#[from] ConstraintViolation),
    #[error(transparent)]
    Unresolved(#[from] Unresolved),
    #[error(transparent)]
    InvalidTyId(#[from] InvalidTyId),
    #[error(transparent)]
    InvalidVar(#[from] InvalidVar),
    #[error(transparent)]
    Insert(#[from] InsertError),
    #[error(transparent)]
    ParseError(#[from] SqlParseError),
}
