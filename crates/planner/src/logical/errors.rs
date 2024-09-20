use spacetimedb_sql_parser::{ast::BinOp, parser::errors::SqlParseError};
use thiserror::Error;

use super::expr::Type;

#[derive(Error, Debug)]
pub enum ConstraintViolation {
    #[error("(expected) {expected} != (actual) {actual}")]
    Eq { expected: Type, actual: Type },
    #[error("{0} is not a numeric type")]
    Num(Type),
    #[error("{0} cannot be interpreted as a byte array")]
    Hex(Type),
    #[error("{0} cannot be parsed as {1}")]
    Lit(String, Type),
    #[error("{1} is not supported by the binary operator {0}")]
    Op(BinOp, Type),
}

impl ConstraintViolation {
    // Types are not equal
    pub fn eq(expected: &Type, actual: &Type) -> Self {
        let expected = expected.clone();
        let actual = actual.clone();
        Self::Eq { expected, actual }
    }

    // Not a numeric type
    pub fn num(t: &Type) -> Self {
        Self::Num(t.clone())
    }

    // Not a type that can be compared to a hex value
    pub fn hex(t: &Type) -> Self {
        Self::Hex(t.clone())
    }

    // This literal expression cannot be parsed as this type
    pub fn lit(v: &str, ty: &Type) -> Self {
        Self::Lit(v.to_string(), ty.clone())
    }

    // This type is not supported by this operator
    pub fn op(op: BinOp, ty: &Type) -> Self {
        Self::Op(op, ty.clone())
    }
}

#[derive(Error, Debug)]
pub enum ResolutionError {
    #[error("Cannot resolve {0}")]
    Var(String),
    #[error("Cannot resolve table {0}")]
    Table(String),
    #[error("Cannot resolve field {1} in {0}")]
    Field(String, String),
    #[error("Cannot resolve type for literal expression")]
    UntypedLiteral,
}

impl ResolutionError {
    /// Cannot resolve name
    pub fn unresolved_var(name: &str) -> Self {
        Self::Var(name.to_string())
    }

    /// Cannot resolve table name
    pub fn unresolved_table(name: &str) -> Self {
        Self::Table(name.to_string())
    }

    /// Cannot resolve field name within table
    pub fn unresolved_field(table: &str, field: &str) -> Self {
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
}

#[derive(Error, Debug)]
pub enum TypingError {
    #[error(transparent)]
    Unsupported(#[from] Unsupported),
    #[error(transparent)]
    Constraint(#[from] ConstraintViolation),
    #[error(transparent)]
    ResolutionError(#[from] ResolutionError),
    #[error(transparent)]
    ParseError(#[from] SqlParseError),
}
