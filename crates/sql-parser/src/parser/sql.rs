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

use crate::ast::{
    sql::{SqlAst, SqlDelete, SqlInsert, SqlSelect, SqlSet, SqlShow, SqlUpdate, SqlValues},
    SqlIdent,
};
use sqlparser::ast::{
    AssignmentTarget, Delete, FromTable, Insert, LimitClause, SelectFlavor, Set, TableObject, ValueWithSpan,
};
use sqlparser::{
    ast::{
        Assignment, Expr, GroupByExpr, ObjectName, Query, Select, SetExpr, Statement, TableFactor, TableWithJoins,
        Value, Values,
    },
    dialect::PostgreSqlDialect,
    parser::Parser,
};

use super::{
    errors::SqlUnsupported, parse_expr_opt, parse_ident, parse_literal, parse_parts, parse_projection, RelParser,
    SqlParseResult,
};

/// Parse a SQL string
pub fn parse_sql(sql: &str) -> SqlParseResult<SqlAst> {
    let mut stmts = Parser::parse_sql(&PostgreSqlDialect {}, sql)?;
    if stmts.len() > 1 {
        return Err(SqlUnsupported::MultiStatement.into());
    }
    if stmts.is_empty() {
        return Err(SqlUnsupported::Empty.into());
    }
    parse_statement(stmts.swap_remove(0))
        .map(|ast| ast.qualify_vars())
        .and_then(|ast| ast.find_unqualified_vars())
}

/// Parse a SQL statement
fn parse_statement(stmt: Statement) -> SqlParseResult<SqlAst> {
    match stmt {
        Statement::Query(query) => Ok(SqlAst::Select(SqlParser::parse_query(*query)?)),
        Statement::Insert(Insert {
            or: None,
            table: TableObject::TableName(table_name),
            columns,
            overwrite: false,
            source: Some(source),
            partitioned: None,
            after_columns,
            on: None,
            returning: None,
            ..
        }) if after_columns.is_empty() => Ok(SqlAst::Insert(SqlInsert {
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
                            with_ordinality: false,
                            partitions,
                            json_path: None,
                            sample: None,
                            index_hints,
                        },
                    joins,
                },
            assignments,
            from: None,
            selection,
            returning: None,
            or: None,
        } if joins.is_empty() && with_hints.is_empty() && partitions.is_empty() && index_hints.is_empty() => {
            Ok(SqlAst::Update(SqlUpdate {
                table: parse_ident(name)?,
                assignments: parse_assignments(assignments)?,
                filter: parse_expr_opt(selection)?,
            }))
        }
        Statement::Delete(Delete {
            tables,
            from: FromTable::WithFromKeyword(from),
            using: None,
            selection,
            returning: None,
            order_by,
            limit: None,
        }) if tables.is_empty() && order_by.is_empty() => Ok(SqlAst::Delete(parse_delete(from, selection)?)),
        Statement::Set(Set::SingleAssignment {
            scope: None,
            hivevar: false,
            variable,
            values,
        }) => Ok(SqlAst::Set(parse_set_var(variable, values)?)),
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
            order_by: None,
            limit_clause: None,
            fetch: None,
            locks,
            for_clause: None,
            settings: None,
            format_clause: None,
            pipe_operators,
        } if locks.is_empty() && pipe_operators.is_empty() => match *body {
            SetExpr::Values(Values {
                explicit_row: false,
                rows,
            }) => {
                let mut row_literals = Vec::new();
                for row in rows {
                    let mut literals = Vec::new();
                    for expr in row {
                        if let Expr::Value(value) = expr {
                            literals.push(parse_literal(value.into())?);
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
                order_by: None,
                limit_clause: None,
                fetch: None,
                locks,
                for_clause: None,
                settings: None,
                format_clause: None,
                pipe_operators,
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
fn parse_assignment(Assignment { target, value }: Assignment) -> SqlParseResult<SqlSet> {
    match (target, &value) {
        (AssignmentTarget::ColumnName(target), Expr::Value(value)) => {
            Ok(SqlSet(parse_ident(target)?, parse_literal(value.clone().into())?))
        }
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
                        with_ordinality: false,
                        partitions,
                        json_path: None,
                        sample: None,
                        index_hints,
                    },
                joins,
            } if joins.is_empty() && with_hints.is_empty() && partitions.is_empty() && index_hints.is_empty() => {
                Ok(SqlDelete {
                    table: parse_ident(name)?,
                    filter: parse_expr_opt(selection)?,
                })
            }
            t => Err(SqlUnsupported::DeleteTable(t).into()),
        }
    } else {
        Err(SqlUnsupported::MultiTableDelete.into())
    }
}

/// Parse a SET variable statement
fn parse_set_var(variable: ObjectName, mut values: Vec<Expr>) -> SqlParseResult<SqlSet> {
    if values.len() == 1 {
        Ok(SqlSet(
            parse_ident(variable)?,
            match values.swap_remove(0) {
                Expr::Value(value) => parse_literal(value.into())?,
                expr => {
                    return Err(SqlUnsupported::Assignment(expr).into());
                }
            },
        ))
    } else {
        Err(SqlUnsupported::feature(Statement::Set(Set::SingleAssignment {
            scope: None,
            hivevar: false,
            variable,
            values,
        }))
        .into())
    }
}

struct SqlParser;

impl RelParser for SqlParser {
    type Ast = SqlSelect;

    fn parse_query(query: Query) -> SqlParseResult<Self::Ast> {
        match query {
            Query {
                with: None,
                body,
                order_by: None,
                limit_clause,
                fetch: None,
                locks,
                for_clause: None,
                settings: None,
                format_clause: None,
                pipe_operators,
            } if locks.is_empty() && pipe_operators.is_empty() => match limit_clause {
                Some(LimitClause::LimitOffset {
                    limit:
                        Some(Expr::Value(ValueWithSpan {
                            value: Value::Number(n, _),
                            ..
                        })),
                    offset: None,
                    limit_by,
                }) if limit_by.is_empty() => parse_set_op(*body, Some(n.into_boxed_str())),
                None => parse_set_op(*body, None),
                Some(x) => Err(SqlUnsupported::feature(x).into()),
            },
            _ => Err(SqlUnsupported::feature(query).into()),
        }
    }
}

/// Parse a set operation
fn parse_set_op(expr: SetExpr, limit: Option<Box<str>>) -> SqlParseResult<SqlSelect> {
    match expr {
        SetExpr::Select(select) => parse_select(*select, limit).map(SqlSelect::qualify_vars),
        _ => Err(SqlUnsupported::feature(expr).into()),
    }
}

/// Parse a SELECT statement
fn parse_select(select: Select, limit: Option<Box<str>>) -> SqlParseResult<SqlSelect> {
    match select {
        Select {
            select_token: _,
            distinct: None,
            top: None,
            top_before_distinct: _,
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
            flavor: SelectFlavor::Standard,
        } if lateral_views.is_empty()
            && exprs.is_empty()
            && cluster_by.is_empty()
            && distribute_by.is_empty()
            && sort_by.is_empty()
            && named_window.is_empty()
            && modifiers.is_empty()
            && !projection.is_empty() =>
        {
            Ok(SqlSelect {
                project: parse_projection(projection)?,
                from: SqlParser::parse_from(from)?,
                filter: parse_expr_opt(selection)?,
                limit,
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
            // Joins require qualified vars
            "select t.* from t join s on int = u32",
        ] {
            assert!(parse_sql(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn supported() {
        for sql in [
            "select a from t",
            "select a from t where x = :sender",
            "select count(*) as n from t",
            "select count(*) as n from t join s on t.id = s.id where s.x = 1",
            "insert into t values (1, 2)",
            "delete from t",
            "delete from t where a = 1",
            "delete from t where x = :sender",
            "update t set a = 1, b = 2",
            "update t set a = 1, b = 2 where c = 3",
            "update t set a = 1, b = 2 where x = :sender",
        ] {
            assert!(parse_sql(sql).is_ok(), "{sql}");
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
            // Aggregate without alias
            "select count(*) from t",
            // Empty statement
            "",
            " ",
        ] {
            assert!(parse_sql(sql).is_err(), "{sql}");
        }
    }
}
