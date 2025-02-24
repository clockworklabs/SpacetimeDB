use std::fmt::Display;

use sqlparser::{
    ast::{
        BinaryOperator, Expr, Function, ObjectName, Query, Select, SelectItem, SetExpr, TableFactor, TableWithJoins,
        Value,
    },
    parser::ParserError,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SubscriptionUnsupported {
    #[error("Unsupported SELECT: {0}")]
    Select(Select),
    #[error("Unsupported: {0}")]
    Feature(String),
    #[error("Unsupported: Non-SELECT queries")]
    Dml,
}

impl SubscriptionUnsupported {
    pub(crate) fn feature(expr: impl Display) -> Self {
        Self::Feature(format!("{expr}"))
    }
}

#[derive(Error, Debug)]
pub enum SqlUnsupported {
    #[error("Unsupported literal expression: {0}")]
    Literal(Value),
    #[error("Unsupported LIMIT expression: {0}")]
    Limit(Expr),
    #[error("Unsupported expression: {0}")]
    Expr(Expr),
    #[error("Unsupported binary operator: {0}")]
    BinOp(BinaryOperator),
    #[error("Unsupported projection: {0}")]
    Projection(SelectItem),
    #[error("Unsupported projection expression: {0}")]
    ProjectionExpr(Expr),
    #[error("Unsupported aggregate function: {0}")]
    Aggregate(Function),
    #[error("Aggregate expressions must have column aliases")]
    AggregateWithoutAlias,
    #[error("Unsupported FROM expression: {0}")]
    From(TableFactor),
    #[error("Unsupported set operation: {0}")]
    SetOp(SetExpr),
    #[error("Unsupported INSERT expression: {0}")]
    Insert(Query),
    #[error("Unsupported INSERT value: {0}")]
    InsertValue(Expr),
    #[error("Unsupported table expression in DELETE: {0}")]
    DeleteTable(TableWithJoins),
    #[error("Unsupported column/variable assignment expression: {0}")]
    Assignment(Expr),
    #[error("Multi-part names are not supported: {0}")]
    MultiPartName(ObjectName),
    #[error("Unsupported: {0}")]
    Feature(String),
    #[error("Non-inner joins are not supported")]
    JoinType,
    #[error("Implicit joins are not supported")]
    ImplicitJoins,
    #[error("Mixed wildcard projections are not supported")]
    MixedWildcardProject,
    #[error("Multiple SQL statements are not supported")]
    MultiStatement,
    #[error("Multi-table DELETE is not supported")]
    MultiTableDelete,
    #[error("Empty SQL query")]
    Empty,
    #[error("Names must be qualified when using joins")]
    UnqualifiedNames,
}

impl SqlUnsupported {
    pub(crate) fn feature(expr: impl Display) -> Self {
        Self::Feature(format!("{expr}"))
    }
}

#[derive(Error, Debug)]
pub enum SqlRequired {
    #[error("A FROM clause is required")]
    From,
    #[error("Aliases are required for JOIN")]
    JoinAlias,
}

#[derive(Error, Debug)]
pub enum SqlParseError {
    #[error(transparent)]
    SqlUnsupported(#[from] SqlUnsupported),
    #[error(transparent)]
    SubscriptionUnsupported(#[from] SubscriptionUnsupported),
    #[error(transparent)]
    SqlRequired(#[from] SqlRequired),
    #[error(transparent)]
    ParserError(#[from] ParserError),
}
