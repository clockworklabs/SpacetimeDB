//! The SpacetimeDB SQL subscription grammar
//!
//! ```ebnf
//! query
//!     = SELECT projection FROM relation [ WHERE predicate ]
//!     ;
//!
//! projection
//!     = STAR
//!     | ident '.' STAR
//!     ;
//!
//! relation
//!     = table
//!     | '(' query ')'
//!     | relation [ [AS] ident ] { [INNER] JOIN relation [ [AS] ident ] ON predicate }
//!     ;
//!
//! predicate
//!     = expr
//!     | predicate AND predicate
//!     | predicate OR  predicate
//!     ;
//!
//! expr
//!     = literal
//!     | ident
//!     | field
//!     | expr op expr
//!     ;
//!
//! field
//!     = ident '.' ident
//!     ;
//!
//! op
//!     = '='
//!     | '<'
//!     | '>'
//!     | '<' '='
//!     | '>' '='
//!     | '!' '='
//!     | '<' '>'
//!     ;
//!
//! literal
//!     = INTEGER
//!     | FLOAT
//!     | STRING
//!     | HEX
//!     | TRUE
//!     | FALSE
//!     ;
//! ```

use sqlparser::{
    ast::{GroupByExpr, Query, Select, SetExpr, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};

use crate::ast::sub::SqlSelect;

use super::{
    errors::{SqlUnsupported, SubscriptionUnsupported},
    parse_expr_opt, parse_projection, RelParser, SqlParseResult,
};

/// Parse a SQL string
pub fn parse_subscription(sql: &str) -> SqlParseResult<SqlSelect> {
    let mut stmts = Parser::parse_sql(&PostgreSqlDialect {}, sql)?;
    match stmts.len() {
        0 => Err(SqlUnsupported::Empty.into()),
        1 => parse_statement(stmts.swap_remove(0))
            .map(|ast| ast.qualify_vars())
            .and_then(|ast| ast.find_unqualified_vars()),
        _ => Err(SqlUnsupported::MultiStatement.into()),
    }
}

/// Parse a SQL query
fn parse_statement(stmt: Statement) -> SqlParseResult<SqlSelect> {
    match stmt {
        Statement::Query(query) => SubParser::parse_query(*query),
        _ => Err(SubscriptionUnsupported::Dml.into()),
    }
}

struct SubParser;

impl RelParser for SubParser {
    type Ast = SqlSelect;

    fn parse_query(query: Query) -> SqlParseResult<Self::Ast> {
        match query {
            Query {
                with: None,
                body,
                order_by,
                limit: None,
                offset: None,
                fetch: None,
                locks,
            } if order_by.is_empty() && locks.is_empty() => parse_set_op(*body),
            _ => Err(SubscriptionUnsupported::feature(query).into()),
        }
    }
}

/// Parse a set operation
fn parse_set_op(expr: SetExpr) -> SqlParseResult<SqlSelect> {
    match expr {
        SetExpr::Select(select) => parse_select(*select).map(SqlSelect::qualify_vars),
        _ => Err(SqlUnsupported::SetOp(expr).into()),
    }
}

// Parse a SELECT statement
fn parse_select(select: Select) -> SqlParseResult<SqlSelect> {
    match select {
        Select {
            distinct: None,
            top: None,
            projection,
            into: None,
            from,
            lateral_views,
            selection,
            group_by: GroupByExpr::Expressions(exprs),
            cluster_by,
            distribute_by,
            sort_by,
            having: None,
            named_window,
            qualify: None,
        } if lateral_views.is_empty()
            && exprs.is_empty()
            && cluster_by.is_empty()
            && distribute_by.is_empty()
            && sort_by.is_empty()
            && named_window.is_empty() =>
        {
            Ok(SqlSelect {
                from: SubParser::parse_from(from)?,
                filter: parse_expr_opt(selection)?,
                project: parse_projection(projection)?,
            })
        }
        _ => Err(SubscriptionUnsupported::Select(select).into()),
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::sub::parse_subscription;

    #[test]
    fn unsupported() {
        for sql in [
            "delete from t",
            " ",
            "",
            "select distinct a from t",
            "select * from (select * from t) join (select * from s) on a = b",
        ] {
            assert!(parse_subscription(sql).is_err());
        }
    }

    #[test]
    fn supported() {
        for sql in [
            "select * from t",
            "select * from t where a = 1",
            "select * from t where a <> 1",
            "select * from t where a = 1 or a = 2",
            "select t.* from t join s",
            "select t.* from t join s on t.c = s.d",
            "select a.* from t as a join s as b on a.c = b.d",
            "select * from t where x = :sender",
        ] {
            assert!(parse_subscription(sql).is_ok());
        }
    }
}
