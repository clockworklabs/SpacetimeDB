use spacetimedb_lib::Identity;

use crate::parser::{errors::SqlUnsupported, SqlParseResult};

use super::{Project, SqlExpr, SqlFrom};

/// A SELECT statement in the SQL subscription language
#[derive(Debug)]
pub struct SqlSelect {
    pub project: Project,
    pub from: SqlFrom,
    pub filter: Option<SqlExpr>,
}

impl SqlSelect {
    pub fn qualify_vars(self) -> Self {
        match &self.from {
            SqlFrom::Expr(_, alias) => Self {
                project: self.project.qualify_vars(alias.clone()),
                filter: self.filter.map(|expr| expr.qualify_vars(alias.clone())),
                from: self.from,
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

    /// Is this AST parameterized?
    /// We need to know in order to hash subscription queries correctly.
    pub fn has_parameter(&self) -> bool {
        self.filter.as_ref().is_some_and(|expr| expr.has_parameter())
    }

    /// Replace the `:sender` parameter with the [Identity] it represents
    pub fn resolve_sender(self, sender_identity: Identity) -> Self {
        Self {
            filter: self.filter.map(|expr| expr.resolve_sender(sender_identity)),
            ..self
        }
    }
}
