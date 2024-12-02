use super::{Project, SqlExpr, SqlFrom, SqlIdent, SqlLiteral};

/// The AST for the SQL DML and query language
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

/// A SELECT statement in the SQL query language
pub struct SqlSelect {
    pub project: Project,
    pub from: SqlFrom,
    pub filter: Option<SqlExpr>,
}

/// INSERT INTO table cols VALUES literals
pub struct SqlInsert {
    pub table: SqlIdent,
    pub fields: Vec<SqlIdent>,
    pub values: SqlValues,
}

/// VALUES literals
pub struct SqlValues(pub Vec<Vec<SqlLiteral>>);

/// UPDATE table SET cols [ WHERE predicate ]
pub struct SqlUpdate {
    pub table: SqlIdent,
    pub assignments: Vec<SqlSet>,
    pub filter: Option<SqlExpr>,
}

/// DELETE FROM table [ WHERE predicate ]
pub struct SqlDelete {
    pub table: SqlIdent,
    pub filter: Option<SqlExpr>,
}

/// SET var '=' literal
pub struct SqlSet(pub SqlIdent, pub SqlLiteral);

/// SHOW var
pub struct SqlShow(pub SqlIdent);
