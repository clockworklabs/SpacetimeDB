use std::fmt::{Display, Formatter};

use spacetimedb_lib::Identity;
use sqlparser::ast::Ident;

pub mod sql;
pub mod sub;

/// The FROM clause is either a relvar or a JOIN
#[derive(Debug)]
pub enum SqlFrom {
    Expr(SqlIdent, SqlIdent),
    Join(SqlIdent, SqlIdent, Vec<SqlJoin>),
}

impl SqlFrom {
    pub fn has_unqualified_vars(&self) -> bool {
        match self {
            Self::Join(_, _, joins) => joins.iter().any(|join| join.has_unqualified_vars()),
            _ => false,
        }
    }
}

/// An inner join in a FROM clause
#[derive(Debug)]
pub struct SqlJoin {
    pub var: SqlIdent,
    pub alias: SqlIdent,
    pub on: Option<SqlExpr>,
}

impl SqlJoin {
    pub fn has_unqualified_vars(&self) -> bool {
        self.on.as_ref().is_some_and(|expr| expr.has_unqualified_vars())
    }
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
    /// SELECT COUNT(*)
    Count(SqlIdent),
}

impl Project {
    pub fn qualify_vars(self, with: SqlIdent) -> Self {
        match self {
            Self::Star(..) | Self::Count(..) => self,
            Self::Exprs(elems) => Self::Exprs(elems.into_iter().map(|elem| elem.qualify_vars(with.clone())).collect()),
        }
    }

    pub fn has_unqualified_vars(&self) -> bool {
        match self {
            Self::Exprs(exprs) => exprs
                .iter()
                .any(|ProjectElem(expr, _)| matches!(expr, ProjectExpr::Var(_))),
            _ => false,
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
    /// A parameter prefixed with `:`
    Param(Parameter),
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
            Self::Lit(..) | Self::Field(..) | Self::Param(..) => self,
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

    pub fn has_unqualified_vars(&self) -> bool {
        match self {
            Self::Var(_) => true,
            Self::Bin(a, b, _) | Self::Log(a, b, _) => a.has_unqualified_vars() || b.has_unqualified_vars(),
            _ => false,
        }
    }

    /// Is this AST parameterized?
    /// We need to know in order to hash subscription queries correctly.
    pub fn has_parameter(&self) -> bool {
        match self {
            Self::Lit(_) | Self::Var(_) | Self::Field(..) => false,
            Self::Param(Parameter::Sender) => true,
            Self::Bin(a, b, _) | Self::Log(a, b, _) => a.has_parameter() || b.has_parameter(),
        }
    }

    /// Replace the `:sender` parameter with the [Identity] it represents
    pub fn resolve_sender(self, sender_identity: Identity) -> Self {
        match self {
            Self::Lit(_) | Self::Var(_) | Self::Field(..) => self,
            Self::Param(Parameter::Sender) => {
                Self::Lit(SqlLiteral::Hex(String::from(sender_identity.to_hex()).into_boxed_str()))
            }

            Self::Bin(a, b, op) => Self::Bin(
                Box::new(a.resolve_sender(sender_identity)),
                Box::new(b.resolve_sender(sender_identity)),
                op,
            ),
            Self::Log(a, b, op) => Self::Log(
                Box::new(a.resolve_sender(sender_identity)),
                Box::new(b.resolve_sender(sender_identity)),
                op,
            ),
        }
    }
}

/// A named parameter prefixed with `:`
#[derive(Debug)]
pub enum Parameter {
    /// :sender
    Sender,
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
