use crate::parser::{errors::SqlUnsupported, SqlParseResult};

use super::{Project, SqlExpr, SqlFrom};

/// A SELECT statement in the SQL subscription language
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
}
