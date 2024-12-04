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
}
