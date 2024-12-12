use std::fmt::{Display, Formatter};

use sqlparser::ast::Ident;

pub mod sql;
pub mod sub;

/// The FROM clause is either a relvar or a JOIN
#[derive(Debug)]
pub enum SqlFrom {
    Expr(SqlIdent, SqlIdent),
    Join(SqlIdent, SqlIdent, Vec<SqlJoin>),
}

/// An inner join in a FROM clause
#[derive(Debug)]
pub struct SqlJoin {
    pub var: SqlIdent,
    pub alias: SqlIdent,
    pub on: Option<SqlExpr>,
}

/// A projection expression in a SELECT clause
#[derive(Debug)]
pub struct ProjectElem(pub ProjectExpr, pub SqlIdent);

impl ProjectElem {
    pub fn qualify_vars(self, with: SqlIdent) -> Self {
        let Self(expr, alias) = self;
        Self(expr.qualify_vars(with), alias)
    }
}

/// A column projection in a SELECT clause
#[derive(Debug)]
pub enum ProjectExpr {
    Var(SqlIdent),
    Field(SqlIdent, SqlIdent),
}

impl From<ProjectExpr> for SqlExpr {
    fn from(value: ProjectExpr) -> Self {
        match value {
            ProjectExpr::Var(name) => Self::Var(name),
            ProjectExpr::Field(table, field) => Self::Field(table, field),
        }
    }
}

impl ProjectExpr {
    pub fn qualify_vars(self, with: SqlIdent) -> Self {
        match self {
            Self::Var(name) => Self::Field(with, name),
            Self::Field(_, _) => self,
        }
    }
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

impl Project {
    pub fn qualify_vars(self, with: SqlIdent) -> Self {
        match self {
            Self::Star(..) => self,
            Self::Exprs(elems) => Self::Exprs(elems.into_iter().map(|elem| elem.qualify_vars(with.clone())).collect()),
        }
    }
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
    /// A binary logic expression
    Log(Box<SqlExpr>, Box<SqlExpr>, LogOp),
}

impl SqlExpr {
    pub fn qualify_vars(self, with: SqlIdent) -> Self {
        match self {
            Self::Var(name) => Self::Field(with, name),
            Self::Lit(..) | Self::Field(..) => self,
            Self::Bin(a, b, op) => Self::Bin(
                Box::new(a.qualify_vars(with.clone())),
                Box::new(b.qualify_vars(with)),
                op,
            ),
            Self::Log(a, b, op) => Self::Log(
                Box::new(a.qualify_vars(with.clone())),
                Box::new(b.qualify_vars(with)),
                op,
            ),
        }
    }
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogOp {
    And,
    Or,
}

impl Display for LogOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::And => write!(f, "AND"),
            Self::Or => write!(f, "OR"),
        }
    }
}
