use spacetimedb_lib::Identity;

use crate::parser::{errors::SqlUnsupported, SqlParseResult};

use super::{Project, SqlExpr, SqlFrom, SqlIdent, SqlLiteral};

/// The AST for the SQL DML and query language
#[derive(Debug)]
pub enum SqlAst {
    /// SELECT ...
    Select(SqlSelect),
    /// INSERT INTO ...
    Insert(SqlInsert),
    /// UPDATE ...
    Update(SqlUpdate),
    /// DELETE FROM ...
    Delete(SqlDelete),
    /// SET var TO ...
    Set(SqlSet),
    /// SHOW var
    Show(SqlShow),
}

impl SqlAst {
    pub fn qualify_vars(self) -> Self {
        match self {
            Self::Select(select) => Self::Select(select.qualify_vars()),
            Self::Update(SqlUpdate {
                table: with,
                assignments,
                filter,
            }) => Self::Update(SqlUpdate {
                table: with.clone(),
                filter: filter.map(|expr| expr.qualify_vars(with)),
                assignments,
            }),
            Self::Delete(SqlDelete { table: with, filter }) => Self::Delete(SqlDelete {
                table: with.clone(),
                filter: filter.map(|expr| expr.qualify_vars(with)),
            }),
            _ => self,
        }
    }

    pub fn find_unqualified_vars(self) -> SqlParseResult<Self> {
        match self {
            Self::Select(select) => select.find_unqualified_vars().map(Self::Select),
            _ => Ok(self),
        }
    }

    /// Replace the `:sender` parameter with the [Identity] it represents
    pub fn resolve_sender(self, sender_identity: Identity) -> Self {
        match self {
            Self::Select(select) => Self::Select(select.resolve_sender(sender_identity)),
            Self::Update(update) => Self::Update(update.resolve_sender(sender_identity)),
            Self::Delete(delete) => Self::Delete(delete.resolve_sender(sender_identity)),
            _ => self,
        }
    }
}

/// A SELECT statement in the SQL query language
#[derive(Debug)]
pub struct SqlSelect {
    pub project: Project,
    pub from: SqlFrom,
    pub filter: Option<SqlExpr>,
    pub limit: Option<Box<str>>,
}

impl SqlSelect {
    pub fn qualify_vars(self) -> Self {
        match &self.from {
            SqlFrom::Expr(_, alias) => Self {
                project: self.project.qualify_vars(alias.clone()),
                filter: self.filter.map(|expr| expr.qualify_vars(alias.clone())),
                ..self
            },
            SqlFrom::Join(..) => self,
        }
    }

    pub fn find_unqualified_vars(self) -> SqlParseResult<Self> {
        if self.from.has_unqualified_vars() {
            return Err(SqlUnsupported::UnqualifiedNames.into());
        }
        if self.project.has_unqualified_vars() {
            return Err(SqlUnsupported::UnqualifiedNames.into());
        }
        Ok(self)
    }

    /// Replace the `:sender` parameter with the [Identity] it represents
    pub fn resolve_sender(self, sender_identity: Identity) -> Self {
        Self {
            filter: self.filter.map(|expr| expr.resolve_sender(sender_identity)),
            ..self
        }
    }
}

/// INSERT INTO table cols VALUES literals
#[derive(Debug)]
pub struct SqlInsert {
    pub table: SqlIdent,
    pub fields: Vec<SqlIdent>,
    pub values: SqlValues,
}

/// VALUES literals
#[derive(Debug)]
pub struct SqlValues(pub Vec<Vec<SqlLiteral>>);

/// UPDATE table SET cols [ WHERE predicate ]
#[derive(Debug)]
pub struct SqlUpdate {
    pub table: SqlIdent,
    pub assignments: Vec<SqlSet>,
    pub filter: Option<SqlExpr>,
}

impl SqlUpdate {
    /// Replace the `:sender` parameter with the [Identity] it represents
    fn resolve_sender(self, sender_identity: Identity) -> Self {
        Self {
            filter: self.filter.map(|expr| expr.resolve_sender(sender_identity)),
            ..self
        }
    }
}

/// DELETE FROM table [ WHERE predicate ]
#[derive(Debug)]
pub struct SqlDelete {
    pub table: SqlIdent,
    pub filter: Option<SqlExpr>,
}

impl SqlDelete {
    /// Replace the `:sender` parameter with the [Identity] it represents
    fn resolve_sender(self, sender_identity: Identity) -> Self {
        Self {
            filter: self.filter.map(|expr| expr.resolve_sender(sender_identity)),
            ..self
        }
    }
}

/// SET var '=' literal
#[derive(Debug)]
pub struct SqlSet(pub SqlIdent, pub SqlLiteral);

/// SHOW var
#[derive(Debug)]
pub struct SqlShow(pub SqlIdent);
