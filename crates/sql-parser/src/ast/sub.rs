use super::{Project, SqlExpr, SqlFrom};

/// The AST for the SQL subscription language
pub enum SqlAst {
    Select(SqlSelect),
    /// UNION ALL
    Union(Box<SqlAst>, Box<SqlAst>),
    /// EXCEPT ALL
    Minus(Box<SqlAst>, Box<SqlAst>),
}

/// A SELECT statement in the SQL subscription language
pub struct SqlSelect {
    pub project: Project,
    pub from: SqlFrom<SqlAst>,
    pub filter: Option<SqlExpr>,
}
