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
    ast::{GroupByExpr, Query, Select, SetExpr, SetOperator, SetQuantifier, Statement},
    dialect::PostgreSqlDialect,
    parser::Parser,
};

use crate::ast::sub::{SqlAst, SqlSelect};

use super::{
    errors::{SqlUnsupported, SubscriptionUnsupported},
    parse_expr_opt, parse_projection, RelParser, SqlParseResult,
};

/// Parse a SQL string
pub fn parse_subscription(sql: &str) -> SqlParseResult<SqlAst> {
    let mut stmts = Parser::parse_sql(&PostgreSqlDialect {}, sql)?;
    if stmts.len() > 1 {
        return Err(SqlUnsupported::MultiStatement.into());
    }
    parse_statement(stmts.swap_remove(0))
}

/// Parse a SQL query
fn parse_statement(stmt: Statement) -> SqlParseResult<SqlAst> {
    match stmt {
        Statement::Query(query) => SubParser::parse_query(*query),
        _ => Err(SubscriptionUnsupported::Dml.into()),
    }
}

struct SubParser;

impl RelParser for SubParser {
    type Ast = SqlAst;

    fn parse_query(query: Query) -> SqlParseResult<Self::Ast> {
        match query {
            Query {
                with: None,
                body,
                order_by: None,
                limit: None,
                limit_by,
                offset: None,
                fetch: None,
                locks,
                for_clause: None,
                settings: None,
                format_clause: None,
            } if locks.is_empty() && limit_by.is_empty() => parse_set_op(*body),
            _ => Err(SubscriptionUnsupported::feature(query).into()),
        }
    }
}

/// Parse a set operation
fn parse_set_op(expr: SetExpr) -> SqlParseResult<SqlAst> {
    match expr {
        SetExpr::Query(query) => SubParser::parse_query(*query),
        SetExpr::Select(select) => Ok(SqlAst::Select(parse_select(*select)?)),
        SetExpr::SetOperation {
            op: SetOperator::Union,
            set_quantifier: SetQuantifier::All,
            left,
            right,
        } => Ok(SqlAst::Union(
            Box::new(parse_set_op(*left)?),
            Box::new(parse_set_op(*right)?),
        )),
        SetExpr::SetOperation {
            op: SetOperator::Except,
            set_quantifier: SetQuantifier::All,
            left,
            right,
        } => Ok(SqlAst::Minus(
            Box::new(parse_set_op(*left)?),
            Box::new(parse_set_op(*right)?),
        )),
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
            prewhere: None,
            selection,
            group_by: GroupByExpr::Expressions(exprs, modifiers),
            cluster_by,
            distribute_by,
            sort_by,
            having: None,
            named_window,
            qualify: None,
            window_before_qualify: false,
            value_table_mode: None,
            connect_by: None,
        } if lateral_views.is_empty()
            && exprs.is_empty()
            && cluster_by.is_empty()
            && distribute_by.is_empty()
            && sort_by.is_empty()
            && named_window.is_empty()
            && modifiers.is_empty() =>
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
            "select distinct a from t",
            "select * from (select * from t) join (select * from s) on a = b",
        ] {
            assert!(parse_subscription(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn supported() {
        for sql in [
            "select * from t",
            "select * from t where a = 1",
            "select * from t where a <> 1",
            "select * from t where a = 1 or a = 2",
            "select * from t where a = 1 union all select * from t where a = 2",
            "select * from (select * from t)",
            "select * from (select t.* from t join s)",
            "select * from (select t.* from t join s on t.c = s.d)",
            "select * from (select a.* from t as a join s as b on a.c = b.d)",
            "select * from (select t.* from (select * from t) t join (select * from s) s on s.id = t.id)",
        ] {
            assert!(parse_subscription(sql).is_ok(), "{sql}");
        }
    }
}
