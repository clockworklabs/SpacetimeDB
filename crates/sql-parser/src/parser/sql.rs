//! The SpacetimeDB SQL grammar
//!
//! ```ebnf
//! statement
//!     = select
//!     | insert
//!     | delete
//!     | update
//!     | set
//!     | show
//!     ;
//!
//! insert
//!     = INSERT INTO table [ '(' column { ',' column } ')' ] VALUES '(' literal { ',' literal } ')'
//!     ;
//!
//! delete
//!     = DELETE FROM table [ WHERE predicate ]
//!     ;
//!
//! update
//!     = UPDATE table SET [ '(' assignment { ',' assignment } ')' ] [ WHERE predicate ]
//!     ;
//!
//! assignment
//!     = column '=' expr
//!     ;
//!
//! set
//!     = SET var ( TO | '=' ) literal
//!     ;
//!
//! show
//!     = SHOW var
//!     ;
//!
//! var
//!     = ident
//!     ;
//!
//! select
//!     = SELECT [ DISTINCT ] projection FROM relation [ [ WHERE predicate ] [ ORDER BY order ] [ LIMIT limit ] ]
//!     ;
//!
//! projection
//!     = listExpr
//!     | projExpr { ',' projExpr }
//!     | aggrExpr { ',' aggrExpr }
//!     ;
//!
//! listExpr
//!     = STAR
//!     | ident '.' STAR
//!     ;
//!
//! projExpr
//!     = columnExpr [ [ AS ] ident ]
//!     ;
//!
//! columnExpr
//!     = column
//!     | field
//!     ;
//!
//! aggrExpr
//!     = COUNT '(' STAR ')' AS ident
//!     | COUNT '(' DISTINCT columnExpr ')' AS ident
//!     | SUM   '(' columnExpr ')' AS ident
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
//! order
//!     = columnExpr [ ASC | DESC ] { ',' columnExpr [ ASC | DESC ] }
//!     ;
//!
//! limit
//!     = INTEGER
//!     ;
//!
//! table
//!     = ident
//!     ;
//!
//! column
//!     = ident
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
    ast::{
        Assignment, Distinct, Expr, GroupByExpr, ObjectName, OrderByExpr, Query, Select, SetExpr, SetOperator,
        SetQuantifier, Statement, TableFactor, TableWithJoins, Values,
    },
    dialect::PostgreSqlDialect,
    parser::Parser,
};

use crate::ast::{
    sql::{
        OrderByElem, QueryAst, SqlAst, SqlDelete, SqlInsert, SqlSelect, SqlSet, SqlSetOp, SqlShow, SqlUpdate, SqlValues,
    },
    SqlIdent, SqlLiteral,
};

use super::{
    errors::SqlUnsupported, parse_expr, parse_expr_opt, parse_ident, parse_literal, parse_parts, parse_projection,
    RelParser, SqlParseResult,
};

/// Parse a SQL string
pub fn parse_sql(sql: &str) -> SqlParseResult<SqlAst> {
    let mut stmts = Parser::parse_sql(&PostgreSqlDialect {}, sql)?;
    if stmts.len() > 1 {
        return Err(SqlUnsupported::MultiStatement.into());
    }
    parse_statement(stmts.swap_remove(0))
}

/// Parse a SQL statement
fn parse_statement(stmt: Statement) -> SqlParseResult<SqlAst> {
    match stmt {
        Statement::Query(query) => Ok(SqlAst::Query(SqlParser::parse_query(*query)?)),
        Statement::Insert {
            or: None,
            table_name,
            columns,
            overwrite: false,
            source,
            partitioned: None,
            after_columns,
            table: false,
            on: None,
            returning: None,
            ..
        } if after_columns.is_empty() => Ok(SqlAst::Insert(SqlInsert {
            table: parse_ident(table_name)?,
            fields: columns.into_iter().map(SqlIdent::from).collect(),
            values: parse_values(*source)?,
        })),
        Statement::Update {
            table:
                TableWithJoins {
                    relation:
                        TableFactor::Table {
                            name,
                            alias: None,
                            args: None,
                            with_hints,
                            version: None,
                            partitions,
                        },
                    joins,
                },
            assignments,
            from: None,
            selection,
            returning: None,
        } if joins.is_empty() && with_hints.is_empty() && partitions.is_empty() => Ok(SqlAst::Update(SqlUpdate {
            table: parse_ident(name)?,
            assignments: parse_assignments(assignments)?,
            filter: parse_expr_opt(selection)?,
        })),
        Statement::Delete {
            tables,
            from,
            using: None,
            selection,
            returning: None,
        } if tables.is_empty() => Ok(SqlAst::Delete(parse_delete(from, selection)?)),
        Statement::SetVariable {
            local: false,
            hivevar: false,
            variable,
            value,
        } => Ok(SqlAst::Set(parse_set_var(variable, value)?)),
        Statement::ShowVariable { variable } => Ok(SqlAst::Show(SqlShow(parse_parts(variable)?))),
        _ => Err(SqlUnsupported::feature(stmt).into()),
    }
}

/// Parse a VALUES expression
fn parse_values(values: Query) -> SqlParseResult<SqlValues> {
    match values {
        Query {
            with: None,
            body,
            order_by,
            limit: None,
            offset: None,
            fetch: None,
            locks,
        } if order_by.is_empty() && locks.is_empty() => match *body {
            SetExpr::Values(Values {
                explicit_row: false,
                rows,
            }) => {
                let mut row_literals = Vec::new();
                for row in rows {
                    let mut literals = Vec::new();
                    for expr in row {
                        if let Expr::Value(value) = expr {
                            literals.push(parse_literal(value)?);
                        } else {
                            return Err(SqlUnsupported::InsertValue(expr).into());
                        }
                    }
                    row_literals.push(literals);
                }
                Ok(SqlValues(row_literals))
            }
            _ => Err(SqlUnsupported::Insert(Query {
                with: None,
                body,
                order_by,
                limit: None,
                offset: None,
                fetch: None,
                locks,
            })
            .into()),
        },
        _ => Err(SqlUnsupported::Insert(values).into()),
    }
}

/// Parse column/variable assignments in an UPDATE or SET statement
fn parse_assignments(assignments: Vec<Assignment>) -> SqlParseResult<Vec<SqlSet>> {
    assignments.into_iter().map(parse_assignment).collect()
}

/// Parse a column/variable assignment in an UPDATE or SET statement
fn parse_assignment(Assignment { id, value }: Assignment) -> SqlParseResult<SqlSet> {
    match value {
        Expr::Value(value) => Ok(SqlSet(parse_parts(id)?, parse_literal(value)?)),
        _ => Err(SqlUnsupported::Assignment(value).into()),
    }
}

/// Parse a DELETE statement
fn parse_delete(mut from: Vec<TableWithJoins>, selection: Option<Expr>) -> SqlParseResult<SqlDelete> {
    if from.len() == 1 {
        match from.swap_remove(0) {
            TableWithJoins {
                relation:
                    TableFactor::Table {
                        name,
                        alias: None,
                        args: None,
                        with_hints,
                        version: None,
                        partitions,
                    },
                joins,
            } if joins.is_empty() && with_hints.is_empty() && partitions.is_empty() => {
                Ok(SqlDelete(parse_ident(name)?, parse_expr_opt(selection)?))
            }
            t => Err(SqlUnsupported::DeleteTable(t).into()),
        }
    } else {
        Err(SqlUnsupported::MultiTableDelete.into())
    }
}

/// Parse a SET variable statement
fn parse_set_var(variable: ObjectName, mut value: Vec<Expr>) -> SqlParseResult<SqlSet> {
    if value.len() == 1 {
        Ok(SqlSet(
            parse_ident(variable)?,
            match value.swap_remove(0) {
                Expr::Value(value) => parse_literal(value)?,
                expr => {
                    return Err(SqlUnsupported::Assignment(expr).into());
                }
            },
        ))
    } else {
        Err(SqlUnsupported::feature(Statement::SetVariable {
            local: false,
            hivevar: false,
            variable,
            value,
        })
        .into())
    }
}

struct SqlParser;

impl RelParser for SqlParser {
    type Ast = QueryAst;

    fn parse_query(query: Query) -> SqlParseResult<Self::Ast> {
        match query {
            Query {
                with: None,
                body,
                order_by,
                limit,
                offset: None,
                fetch: None,
                locks,
            } if locks.is_empty() => Ok(QueryAst {
                query: parse_set_op(*body)?,
                order: parse_order_by(order_by)?,
                limit: parse_limit(limit)?,
            }),
            _ => Err(SqlUnsupported::feature(query).into()),
        }
    }
}

/// Parse ORDER BY
fn parse_order_by(items: Vec<OrderByExpr>) -> SqlParseResult<Vec<OrderByElem>> {
    let mut elems = Vec::new();
    for item in items {
        elems.push(OrderByElem(
            parse_expr(item.expr)?,
            matches!(item.asc, Some(true)) || item.asc.is_none(),
        ));
    }
    Ok(elems)
}

/// Parse LIMIT
fn parse_limit(limit: Option<Expr>) -> SqlParseResult<Option<SqlLiteral>> {
    limit
        .map(|expr| {
            if let Expr::Value(v) = expr {
                parse_literal(v)
            } else {
                Err(SqlUnsupported::Limit(expr).into())
            }
        })
        .transpose()
}

/// Parse a set operation
fn parse_set_op(expr: SetExpr) -> SqlParseResult<SqlSetOp> {
    match expr {
        SetExpr::Query(query) => Ok(SqlSetOp::Query(Box::new(SqlParser::parse_query(*query)?))),
        SetExpr::Select(select) => Ok(SqlSetOp::Select(parse_select(*select)?)),
        SetExpr::SetOperation {
            op: SetOperator::Union,
            set_quantifier: SetQuantifier::All,
            left,
            right,
        } => Ok(SqlSetOp::Union(
            Box::new(parse_set_op(*left)?),
            Box::new(parse_set_op(*right)?),
            true,
        )),
        SetExpr::SetOperation {
            op: SetOperator::Union,
            set_quantifier: SetQuantifier::None,
            left,
            right,
        } => Ok(SqlSetOp::Union(
            Box::new(parse_set_op(*left)?),
            Box::new(parse_set_op(*right)?),
            false,
        )),
        SetExpr::SetOperation {
            op: SetOperator::Except,
            set_quantifier: SetQuantifier::All,
            left,
            right,
        } => Ok(SqlSetOp::Minus(
            Box::new(parse_set_op(*left)?),
            Box::new(parse_set_op(*right)?),
            true,
        )),
        SetExpr::SetOperation {
            op: SetOperator::Except,
            set_quantifier: SetQuantifier::None,
            left,
            right,
        } => Ok(SqlSetOp::Minus(
            Box::new(parse_set_op(*left)?),
            Box::new(parse_set_op(*right)?),
            false,
        )),
        _ => Err(SqlUnsupported::feature(expr).into()),
    }
}

/// Parse a SELECT statement
fn parse_select(select: Select) -> SqlParseResult<SqlSelect> {
    match select {
        Select {
            distinct,
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
                project: parse_projection(projection)?,
                distinct: matches!(distinct, Some(Distinct::Distinct)),
                from: SqlParser::parse_from(from)?,
                filter: parse_expr_opt(selection)?,
            })
        }
        _ => Err(SqlUnsupported::feature(select).into()),
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::sql::parse_sql;

    #[test]
    fn unsupported() {
        for sql in [
            // FROM is required
            "select 1",
            // Multi-part table names
            "select a from s.t",
            // Bit-string literals
            "select * from t where a = B'1010'",
            // Wildcard with non-wildcard projections
            "select a.*, b, c from t",
            // Limit expression
            "select * from t order by a limit b",
            // GROUP BY
            "select a, count(*) from t group by a",
            // Join updates
            "update t as a join s as b on a.id = b.id set c = 1",
            // Join updates
            "update t set a = 1 from s where t.id = s.id and s.b = 2",
            // Implicit joins
            "select a.* from t as a, s as b where a.id = b.id and b.c = 1",
        ] {
            assert!(parse_sql(sql).is_err());
        }
    }

    #[test]
    fn supported() {
        for sql in [
            "select a from t",
            "select distinct a from t",
            "select * from t order by a limit 5",
            "select * from t where a = 1 union select * from t where a = 2",
            "insert into t values (1, 2)",
            "insert into t (a, b) values (1, 2)",
            "delete from t",
            "delete from t where a = 1",
            "update t set a = 1, b = 2",
            "update t set a = 1, b = 2 where c = 3",
        ] {
            assert!(parse_sql(sql).is_ok());
        }
    }

    #[test]
    fn invalid() {
        for sql in [
            // Empty SELECT
            "select from t",
            // Empty FROM
            "select a from where b = 1",
            // Empty WHERE
            "select a from t where",
            // Empty GROUP BY
            "select a, count(*) from t group by",
        ] {
            assert!(parse_sql(sql).is_err());
        }
    }
}
