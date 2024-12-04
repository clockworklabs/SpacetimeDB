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
}

/// A SELECT statement in the SQL query language
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
