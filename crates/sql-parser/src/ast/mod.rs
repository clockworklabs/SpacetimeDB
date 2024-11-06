use std::fmt::{Display, Formatter};

use sqlparser::ast::Ident;

pub mod sql;
pub mod sub;

/// The FROM clause is either a [RelExpr] or a JOIN
#[derive(Debug)]
pub enum SqlFrom<Ast> {
    Expr(RelExpr<Ast>, Option<SqlIdent>),
    Join(RelExpr<Ast>, SqlIdent, Vec<SqlJoin<Ast>>),
}

/// A RelExpr is an expression that produces a relation
#[derive(Debug)]
pub enum RelExpr<Ast> {
    Var(SqlIdent),
    Ast(Box<Ast>),
}

/// An inner join in a FROM clause
#[derive(Debug)]
pub struct SqlJoin<Ast> {
    pub expr: RelExpr<Ast>,
    pub alias: SqlIdent,
    pub on: Option<SqlExpr>,
}

/// A projection expression in a SELECT clause
#[derive(Debug)]
pub struct ProjectElem(pub ProjectExpr, pub Option<SqlIdent>);

/// A column projection in a SELECT clause
#[derive(Debug)]
pub enum ProjectExpr {
    Var(SqlIdent),
    Field(SqlIdent, SqlIdent),
}

/// A SQL SELECT clause
#[derive(Debug)]
pub enum Project {
    /// SELECT *
    /// SELECT a.*
    Star(Option<SqlIdent>),
    /// SELECT a, b
    Exprs(Vec<ProjectElem>),
}

/// A scalar SQL expression
#[derive(Debug)]
pub enum SqlExpr {
    /// A constant expression
    Lit(SqlLiteral),
    /// Unqualified column ref
    Var(SqlIdent),
    /// Qualified column ref
    Field(SqlIdent, SqlIdent),
    /// A binary infix expression
    Bin(Box<SqlExpr>, Box<SqlExpr>, BinOp),
}

/// A SQL identifier or named reference.
/// Currently case sensitive.
#[derive(Debug, Clone)]
pub struct SqlIdent(pub Box<str>);

/// Case insensitivity should be implemented here if at all
impl From<Ident> for SqlIdent {
    fn from(Ident { value, .. }: Ident) -> Self {
        SqlIdent(value.into_boxed_str())
    }
}

/// A SQL constant expression
#[derive(Debug)]
pub enum SqlLiteral {
    /// A boolean constant
    Bool(bool),
    /// A hex value like 0xFF or x'FF'
    Hex(Box<str>),
    /// An integer or float value
    Num(Box<str>),
    /// A string value
    Str(Box<str>),
}

/// Binary infix operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
}

impl Display for BinOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eq => write!(f, "="),
            Self::Ne => write!(f, "<>"),
            Self::Lt => write!(f, "<"),
            Self::Gt => write!(f, ">"),
            Self::Lte => write!(f, "<="),
            Self::Gte => write!(f, ">="),
            Self::And => write!(f, "AND"),
            Self::Or => write!(f, "OR"),
        }
    }
}
