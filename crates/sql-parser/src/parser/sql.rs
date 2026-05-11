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
        Assignment, Expr, GroupByExpr, ObjectName, Query, Select, SetExpr, Statement, TableFactor, TableWithJoins,
        Value, Values,
    },
    dialect::PostgreSqlDialect,
    parser::Parser,
};

use crate::ast::{
    sql::{CypherQuery, SqlAst, SqlDelete, SqlInsert, SqlSelect, SqlSet, SqlShow, SqlUpdate, SqlValues},
    SqlIdent,
};

use super::{
    errors::SqlUnsupported, parse_expr_opt, parse_ident, parse_literal_expr, parse_parts, parse_projection, RelParser,
    SqlParseResult,
};

/// Parse a SQL string.
///
/// Supports standard SQL DML as well as two Cypher entry points:
/// - Bare `MATCH … RETURN …` (direct openCypher syntax)
/// - `SELECT * FROM cypher('MATCH … RETURN …')` (AGE-style SQL wrapper)
pub fn parse_sql(sql: &str) -> SqlParseResult<SqlAst> {
    let trimmed = sql.trim();

    if trimmed.len() >= 5
        && trimmed[..5].eq_ignore_ascii_case("MATCH")
        && trimmed.as_bytes().get(5).map_or(true, |b| b.is_ascii_whitespace() || *b == b'(')
    {
        return Ok(SqlAst::Cypher(spacetimedb_cypher_parser::parse_cypher(trimmed)?));
    }

    let mut stmts = Parser::parse_sql(&PostgreSqlDialect {}, sql)?;
    if stmts.len() > 1 {
        return Err(SqlUnsupported::MultiStatement.into());
    }
    if stmts.is_empty() {
        return Err(SqlUnsupported::Empty.into());
    }

    let stmt = stmts.swap_remove(0);

    if let Some(cypher_ast) = try_parse_cypher_function(&stmt)? {
        return Ok(SqlAst::Cypher(cypher_ast));
    }

    parse_statement(stmt)
        .map(|ast| ast.qualify_vars())
        .and_then(|ast| ast.find_unqualified_vars())
}

/// Detect `SELECT * FROM cypher('...')` and parse the inner Cypher string.
fn try_parse_cypher_function(stmt: &Statement) -> SqlParseResult<Option<CypherQuery>> {
    let Statement::Query(query) = stmt else {
        return Ok(None);
    };
    let Query {
        with: None,
        body,
        order_by,
        limit: None,
        offset: None,
        fetch: None,
        locks,
        ..
    } = query.as_ref()
    else {
        return Ok(None);
    };
    if !order_by.is_empty() || !locks.is_empty() {
        return Ok(None);
    }
    let SetExpr::Select(select) = body.as_ref() else {
        return Ok(None);
    };
    use sqlparser::ast::SelectItem;
    if select.projection.len() != 1 || !matches!(select.projection[0], SelectItem::Wildcard(_)) {
        return Ok(None);
    }
    if select.from.len() != 1 {
        return Ok(None);
    }
    let TableWithJoins { relation, joins } = &select.from[0];
    if !joins.is_empty() {
        return Ok(None);
    }
    let TableFactor::Table {
        name,
        args: Some(args),
        ..
    } = relation
    else {
        return Ok(None);
    };
    if name.0.len() != 1 || !name.0[0].value.eq_ignore_ascii_case("cypher") {
        return Ok(None);
    }
    use sqlparser::ast::{FunctionArg, FunctionArgExpr};
    if args.len() != 1 {
        return Ok(None);
    }
    let cypher_str = match &args[0] {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(Value::SingleQuotedString(s)))) => s,
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(Value::DollarQuotedString(s)))) => &s.value,
        _ => return Ok(None),
    };
    let ast = spacetimedb_cypher_parser::parse_cypher(cypher_str)?;
    Ok(Some(ast))
}

/// Parse a SQL statement
fn parse_statement(stmt: Statement) -> SqlParseResult<SqlAst> {
    match stmt {
        Statement::Query(query) => Ok(SqlAst::Select(SqlParser::parse_query(*query)?)),
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
                        literals.push(parse_literal_expr(expr, SqlUnsupported::InsertValue)?);
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
    Ok(SqlSet(
        parse_parts(id)?,
        parse_literal_expr(value, SqlUnsupported::Assignment)?,
    ))
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
            } if joins.is_empty() && with_hints.is_empty() && partitions.is_empty() => Ok(SqlDelete {
                table: parse_ident(name)?,
                filter: parse_expr_opt(selection)?,
            }),
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
            parse_literal_expr(value.swap_remove(0), SqlUnsupported::Assignment)?,
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
            } if order_by.is_empty() && locks.is_empty() => parse_set_op(*body, None),
            Query {
                with: None,
                body,
                order_by,
                limit: Some(Expr::Value(Value::Number(n, _))),
                offset: None,
                fetch: None,
                locks,
            } if order_by.is_empty() && locks.is_empty() => parse_set_op(*body, Some(n.into_boxed_str())),
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
            assert!(parse_sql(sql).is_err());
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
            assert!(parse_sql(sql).is_ok());
        }
    }

    #[test]
    fn signed_numeric_literals_are_supported_across_sql_api() {
        for sql in [
            "select a from t where b = -1",
            "delete from t where a = +1",
            "insert into t values (-1, +2.5)",
            "update t set a = -1, b = +2 where c = -3",
            "set x = -1",
            "set y to +2.5",
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
            // Aggregate without alias
            "select count(*) from t",
            // Empty statement
            "",
            " ",
        ] {
            assert!(parse_sql(sql).is_err());
        }
    }

    // ── Cypher entry point tests ────────────────────────────────────────

    #[test]
    fn cypher_bare_match() {
        let ast = parse_sql("MATCH (n:Person) RETURN n").unwrap();
        assert!(matches!(ast, super::SqlAst::Cypher(_)));
    }

    #[test]
    fn cypher_bare_match_case_insensitive() {
        let ast = parse_sql("match (n) WHERE n.x = 1 return n").unwrap();
        assert!(matches!(ast, super::SqlAst::Cypher(_)));
    }

    #[test]
    fn cypher_bare_match_with_leading_whitespace() {
        let ast = parse_sql("  MATCH (a)-[:KNOWS]->(b) RETURN a, b").unwrap();
        assert!(matches!(ast, super::SqlAst::Cypher(_)));
    }

    #[test]
    fn cypher_function_single_quoted() {
        let ast = parse_sql("SELECT * FROM cypher('MATCH (n) RETURN n')").unwrap();
        assert!(matches!(ast, super::SqlAst::Cypher(_)));
    }

    #[test]
    fn cypher_function_case_insensitive_name() {
        let ast = parse_sql("SELECT * FROM CYPHER('MATCH (n) RETURN n')").unwrap();
        assert!(matches!(ast, super::SqlAst::Cypher(_)));
    }

    #[test]
    fn cypher_function_with_complex_query() {
        let ast =
            parse_sql("SELECT * FROM cypher('MATCH (a:Person)-[r:KNOWS]->(b) WHERE a.age > 25 RETURN a, b')")
                .unwrap();
        assert!(matches!(ast, super::SqlAst::Cypher(_)));
    }

    #[test]
    fn cypher_function_dollar_quoted() {
        let ast = parse_sql("SELECT * FROM cypher($$MATCH (n) RETURN n$$)").unwrap();
        assert!(matches!(ast, super::SqlAst::Cypher(_)));
    }

    #[test]
    fn cypher_bare_match_produces_correct_ast() {
        use spacetimedb_cypher_parser::ast::*;
        let ast = parse_sql("MATCH (p:Person) WHERE p.age > 30 RETURN p.name").unwrap();
        let super::SqlAst::Cypher(query) = ast else {
            panic!("expected Cypher variant");
        };
        assert_eq!(query.match_clause.patterns.len(), 1);
        assert_eq!(
            query.match_clause.patterns[0].nodes[0].label.as_deref(),
            Some("Person")
        );
        assert!(query.where_clause.is_some());
        assert!(matches!(query.return_clause, ReturnClause::Items(ref items) if items.len() == 1));
    }

    #[test]
    fn cypher_error_invalid_match_syntax() {
        let err = parse_sql("MATCH (n) WHERE");
        assert!(err.is_err());
    }

    #[test]
    fn cypher_error_function_with_invalid_inner() {
        let err = parse_sql("SELECT * FROM cypher('MATCH (n) WHERE')");
        assert!(err.is_err());
    }

    #[test]
    fn cypher_function_does_not_hijack_normal_from() {
        let ast = parse_sql("select a from t").unwrap();
        assert!(matches!(ast, super::SqlAst::Select(_)));
    }

    #[test]
    fn cypher_bare_match_prefix_does_not_trigger_cypher() {
        let err = parse_sql("MATCHING (n:Person) RETURN n");
        assert!(err.is_err(), "MATCHING should not be treated as a MATCH keyword");
    }

    #[test]
    fn cypher_function_non_wildcard_projection_falls_through() {
        let err = parse_sql("SELECT a.name FROM cypher('MATCH (n) RETURN n')");
        assert!(err.is_err(), "non-wildcard projection should not be intercepted as cypher");
    }
}
