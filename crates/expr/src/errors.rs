use super::statement::InvalidVar;
use spacetimedb_lib::AlgebraicType;
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sql_parser::ast::BinOp;
use spacetimedb_sql_parser::parser::errors::SqlParseError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Unresolved {
    #[error("`{0}` is not in scope")]
    Var(String),
    #[error("`{0}` is not a valid table")]
    Table(String),
    #[error("`{0}` does not have a field `{1}`")]
    Field(String, String),
    #[error("Cannot resolve type for literal expression")]
    Literal,
}

impl Unresolved {
    /// Cannot resolve name
    pub fn var(name: &str) -> Self {
        Self::Var(name.to_owned())
    }

    /// Cannot resolve table name
    pub fn table(name: &str) -> Self {
        Self::Table(name.to_owned())
    }

    /// Cannot resolve field name within table
    pub fn field(table: &str, field: &str) -> Self {
        Self::Field(table.to_owned(), field.to_owned())
    }
}

#[derive(Error, Debug)]
pub enum InvalidWildcard {
    #[error("SELECT * is not supported for joins")]
    Join,
}

#[derive(Error, Debug)]
pub enum Unsupported {
    #[error("Column projections are not supported in subscriptions; Subscriptions must return a table type")]
    ReturnType,
    #[error("Distinct projections are not supported in subscriptions")]
    Dedup,
    #[error("Unsupported expression in projection")]
    ProjectExpr,
}

// TODO: It might be better to return the missing/extra fields
#[derive(Error, Debug)]
#[error("Inserting a row with {values} values into `{table}` which has {fields} fields")]
pub struct InsertValuesError {
    pub table: String,
    pub values: usize,
    pub fields: usize,
}

// TODO: It might be better to return the missing/extra fields
#[derive(Error, Debug)]
#[error("The number of fields ({nfields}) in the INSERT does not match the number of columns ({ncols}) of the table `{table}`")]
pub struct InsertFieldsError {
    pub table: String,
    pub ncols: usize,
    pub nfields: usize,
}

#[derive(Debug, Error)]
#[error("Invalid binary operator `{op}` for type `{ty}`")]
pub struct InvalidOp {
    op: BinOp,
    ty: String,
}

impl InvalidOp {
    pub fn new(op: BinOp, ty: &AlgebraicType) -> Self {
        Self {
            op,
            ty: fmt_algebraic_type(ty).to_string(),
        }
    }
}

#[derive(Error, Debug)]
#[error("The literal expression `{literal}` cannot be parsed as type `{ty}`")]
pub struct InvalidLiteral {
    literal: String,
    ty: String,
}

impl InvalidLiteral {
    pub fn new(literal: String, expected: &AlgebraicType) -> Self {
        Self {
            literal,
            ty: fmt_algebraic_type(expected).to_string(),
        }
    }
}

#[derive(Debug, Error)]
#[error("Unexpected type: (expected) {expected} != {inferred} (inferred)")]
pub struct UnexpectedType {
    expected: String,
    inferred: String,
}

impl UnexpectedType {
    pub fn new(expected: &AlgebraicType, inferred: &AlgebraicType) -> Self {
        Self {
            expected: fmt_algebraic_type(expected).to_string(),
            inferred: fmt_algebraic_type(inferred).to_string(),
        }
    }
}

#[derive(Debug, Error)]
#[error("Duplicate name `{0}`")]
pub struct DuplicateName(pub String);

#[derive(Debug, Error)]
#[error("`filter!` does not support column projections; Must return table rows")]
pub struct FilterReturnType;

#[derive(Error, Debug)]
pub enum TypingError {
    #[error(transparent)]
    Unsupported(#[from] Unsupported),
    #[error(transparent)]
    Unresolved(#[from] Unresolved),
    #[error(transparent)]
    InvalidVar(#[from] InvalidVar),
    #[error(transparent)]
    InsertValues(#[from] InsertValuesError),
    #[error(transparent)]
    InsertFields(#[from] InsertFieldsError),
    #[error(transparent)]
    ParseError(#[from] SqlParseError),

    #[error(transparent)]
    InvalidOp(#[from] InvalidOp),
    #[error(transparent)]
    Literal(#[from] InvalidLiteral),
    #[error(transparent)]
    Unexpected(#[from] UnexpectedType),
    #[error(transparent)]
    Wildcard(#[from] InvalidWildcard),
    #[error(transparent)]
    DuplicateName(#[from] DuplicateName),
    #[error(transparent)]
    FilterReturnType(#[from] FilterReturnType),
}
