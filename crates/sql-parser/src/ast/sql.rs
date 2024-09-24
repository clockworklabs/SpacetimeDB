use super::{Project, SqlExpr, SqlFrom, SqlIdent, SqlLiteral};

/// The AST for the SQL DML and query language
pub enum SqlAst {
    /// SELECT ...
    Query(QueryAst),
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

/// The AST for the SQL query language
pub struct QueryAst {
    pub query: SqlSetOp,
    pub order: Vec<OrderByElem>,
    pub limit: Option<SqlLiteral>,
}

/// Set operations in the SQL query language
pub enum SqlSetOp {
    /// SELECT
    Select(SqlSelect),
    /// ORDER/LIMIT
    Query(Box<QueryAst>),
    /// UNION
    Union(Box<SqlSetOp>, Box<SqlSetOp>, bool),
    /// EXCEPT
    Minus(Box<SqlSetOp>, Box<SqlSetOp>, bool),
}

/// A SELECT statement in the SQL query language
pub struct SqlSelect {
    pub project: Project,
    pub distinct: bool,
    pub from: SqlFrom<QueryAst>,
    pub filter: Option<SqlExpr>,
}

/// ORDER BY cols [ ASC | DESC ]
pub struct OrderByElem(pub SqlExpr, pub bool);

/// INSERT INTO table cols VALUES literals
pub struct SqlInsert {
    pub table: SqlIdent,
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
