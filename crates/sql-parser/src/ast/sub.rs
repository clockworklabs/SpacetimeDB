use super::{Project, SqlExpr, SqlFrom};

/// A SELECT statement in the SQL subscription language
pub struct SqlSelect {
    pub project: Project,
    pub from: SqlFrom,
    pub filter: Option<SqlExpr>,
}
