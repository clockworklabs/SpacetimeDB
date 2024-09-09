use crate::ast::sql::OrderByElem;
use crate::ast::{BinOp, Project, ProjectElem, SqlExpr, SqlIdent, SqlLiteral};
use crate::parser::dialect::SpacetimeSqlDialect;
use sqlparser::ast::*;
use sqlparser::keywords::Keyword;
use sqlparser::parser::*;
use sqlparser::tokenizer::{Token, TokenWithLocation, Tokenizer, TokenizerError};
use std::collections::VecDeque;
use std::ops::Range;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub start: TokenWithLocation,
    pub end: TokenWithLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T> {
    pub value: T,
    pub location: Location,
}

impl<T> Spanned<T> {
    pub fn new(value: T, start: TokenWithLocation, end: TokenWithLocation) -> Self {
        Self {
            value,
            location: Location { start, end },
        }
    }

    pub fn location(value: T, location: Location) -> Self {
        Self { value, location }
    }
}

#[derive(Error, Debug)]
pub enum SqlError {
    #[error("Tokenizer error: {0}")]
    Tokenizer(#[from] Box<TokenizerError>),
    #[error("Expected `{expected}`, found `{0:?}`", found.token)]
    Expect {
        expected: String,
        found: Box<TokenWithLocation>,
    },
    #[error("{0}")]
    Parse(#[from] Box<ParserError>),
    #[error("Unsupported projection: {0}")]
    MultiPartName(ObjectName),
    #[error("Unsupported projection: {0:?}")]
    Projection(Spanned<SelectItem>),
    #[error("Unsupported literal expression: {0}")]
    BinOp(BinaryOperator),
    #[error("Unsupported expression: `{expr}`")]
    Literal { expr: Value, found: Box<Location> },
    #[error("Unsupported expression: `{expr}`")]
    Expr { expr: Expr, found: Box<Location> },
    #[error("Unsupported projection")]
    MixedWildcardProject,
}

pub enum LineColumn<'a> {
    Expect { found: &'a str, line: u64, column: u64 },
    At { line: u64, column: u64 },
}

fn extract_line_column(input: &str) -> Option<LineColumn> {
    // Find the positions of "Line:" and "Column:"
    let line_pos = input.find("Line:")?;
    let column_pos = input.find("Column:")?;

    let line_str = input.get(line_pos + 6..column_pos - 2)?.trim();
    let column_str = input.get(column_pos + 8..)?.trim();

    let line = line_str.parse().ok()?;
    let column = column_str.parse().ok()?;

    if let Some(found) = input.find("found:") {
        let line_found = input.get(found + 6..line_pos - 3)?.trim();
        Some(LineColumn::Expect {
            found: line_found,
            line,
            column,
        })
    } else {
        Some(LineColumn::At { line, column })
    }
}

fn token_len(of: &TokenWithLocation) -> usize {
    match &of.token {
        Token::EOF => 3,
        Token::Word(s) => s.value.len(),
        Token::Number(s, _) => s.len(),
        Token::Char(s) => s.len_utf8(),
        Token::SingleQuotedString(s) => s.len(),
        Token::DoubleQuotedString(s) => s.len(),
        Token::TripleSingleQuotedString(s) => s.len(),
        Token::TripleDoubleQuotedString(s) => s.len(),
        Token::DollarQuotedString(s) => s.value.len(),
        Token::SingleQuotedByteStringLiteral(s) => s.len(),
        Token::DoubleQuotedByteStringLiteral(s) => s.len(),
        Token::TripleSingleQuotedByteStringLiteral(s) => s.len(),
        Token::TripleDoubleQuotedByteStringLiteral(s) => s.len(),
        Token::SingleQuotedRawStringLiteral(s) => s.len(),
        Token::DoubleQuotedRawStringLiteral(s) => s.len(),
        Token::TripleSingleQuotedRawStringLiteral(s) => s.len(),
        Token::TripleDoubleQuotedRawStringLiteral(s) => s.len(),
        Token::NationalStringLiteral(s) => s.len(),
        Token::EscapedStringLiteral(s) => s.len(),
        Token::UnicodeStringLiteral(s) => s.len(),
        Token::HexStringLiteral(s) => s.len(),
        x => x.to_string().len(),
    }
}

#[derive(Debug)]
pub struct SqlErrorWithLocation<'a> {
    pub(crate) error: SqlError,
    pub(crate) sql: &'a str,
    pub(crate) span: Range<usize>,
    pub(crate) label: String,
}

impl<'a> SqlErrorWithLocation<'a> {
    pub fn new(error: SqlError, sql: &'a str) -> Self {
        let lines: Vec<_> = sql
            .split('\n')
            .enumerate()
            .map(|(i, line)| {
                let start = i;
                let end = start + line.len();
                (line, start..(end + 1))
            })
            .collect();

        let (label, span) = match &error {
            SqlError::Tokenizer(err) => {
                let found = err.location;
                ("Token", Self::span(&lines, found.line, found.column))
            }
            SqlError::Expect { found, .. } => ("Expect", Self::span_token(&lines, found)),
            SqlError::Parse(err) => match &**err {
                ParserError::TokenizerError(_) => ("Error", 0..sql.len()),
                ParserError::ParserError(msg) => {
                    if let Some(err) = extract_line_column(msg) {
                        match err {
                            LineColumn::Expect { found, line, column } => {
                                ("Error here", Self::span_expect(&lines, found, line, column))
                            }
                            LineColumn::At { line, column } => ("Error here", Self::span(&lines, line, column)),
                        }
                    } else {
                        ("Error", 0..sql.len())
                    }
                }
                ParserError::ParserErrorWithToken { token, .. } => ("Error here", Self::span_token(&lines, token)),
                ParserError::RecursionLimitExceeded => ("RecursionLimitExceeded", 0..sql.len()),
            },
            SqlError::MultiPartName(name) => ("Unsupported name", Self::span_text(sql, &name.to_string())),
            SqlError::Expr { found, .. } => {
                let (start, end) = (&found.start, &found.end);
                let range = Self::span_range(&lines, start, end);
                ("Unsupported expression", range)
            }
            SqlError::Literal { found, .. } => {
                let (start, end) = (&found.start, &found.end);
                let range = Self::span_range(&lines, start, end);
                ("Unsupported literal", range)
            }
            x => todo!("{:?}", x),
        };
        Self {
            label: label.to_string(),
            error,
            sql,
            span,
        }
    }

    pub fn span_token(lines: &[(&'a str, Range<usize>)], token_with_location: &TokenWithLocation) -> Range<usize> {
        let (line, col) = (token_with_location.location.line, token_with_location.location.column);
        let len = token_len(token_with_location);
        let (_, range) = lines.get((line - 1) as usize).unwrap();
        let start = range.start + (col as usize) - 1;
        let end = start + len;
        start..end
    }

    pub fn span_text(source: &str, of: &str) -> Range<usize> {
        if let Some(start) = source.find(of) {
            start..(start + of.len())
        } else {
            0..source.len()
        }
    }
    pub fn span_expect(lines: &[(&'a str, Range<usize>)], found: &str, line: u64, column: u64) -> Range<usize> {
        let (_, range) = lines.get((line - 1) as usize).unwrap();
        let start = range.start + (column as usize) - 1;
        let end = start + found.len();
        start..end
    }

    pub fn span(lines: &[(&'a str, Range<usize>)], line: u64, col: u64) -> Range<usize> {
        //dbg!(lines, line, col);
        let (_, range) = lines.get((line - 1) as usize).unwrap();
        let end = range.start + (col as usize);
        range.start..end
    }

    pub fn span_range(
        lines: &[(&'a str, Range<usize>)],
        start: &TokenWithLocation,
        end: &TokenWithLocation,
    ) -> Range<usize> {
        let start = Self::span(lines, start.location.line, start.location.column);
        let end = Self::span(lines, end.location.line, end.location.column);
        start.start..end.end
    }
}

#[derive(Debug, Clone)]
pub struct SqlSelect {
    pub distinct: Option<Spanned<()>>,
    pub projection: Spanned<Vec<Spanned<Project>>>,
    pub from: Vec<Spanned<TableWithJoins>>,
    pub selection: Option<Spanned<Expr>>,
    pub sort_by: Vec<Spanned<OrderByElem>>,
}

/// SQL Statement.
#[derive(Debug, Clone)]
pub enum SqlStatement {
    Select(Box<SqlSelect>),
}

/// Parse an identifier
pub(crate) fn parse_ident(ObjectName(parts): ObjectName) -> Result<SqlIdent, SqlError> {
    parse_parts(parts)
}

/// Parse an identifier
pub(crate) fn parse_parts(mut parts: Vec<Ident>) -> Result<SqlIdent, SqlError> {
    if parts.len() == 1 {
        return Ok(parts.swap_remove(0).into());
    }
    Err(SqlError::MultiPartName(ObjectName(parts)))
}

/// Parse a SELECT clause with only a single item
pub(crate) fn parse_project(item: Spanned<SelectItem>) -> Result<Spanned<Project>, SqlError> {
    let project = match item.value {
        SelectItem::Wildcard(WildcardAdditionalOptions {
            opt_ilike: None,
            opt_exclude: None,
            opt_except: None,
            opt_rename: None,
            opt_replace: None,
        }) => Project::Star(None),
        SelectItem::QualifiedWildcard(
            table_name,
            WildcardAdditionalOptions {
                opt_ilike: None,
                opt_exclude: None,
                opt_except: None,
                opt_rename: None,
                opt_replace: None,
            },
        ) => Project::Star(Some(parse_ident(table_name)?)),
        SelectItem::UnnamedExpr(_) | SelectItem::ExprWithAlias { .. } => {
            Project::Exprs(vec![parse_project_elem(item.clone())?])
        }
        _ => return Err(SqlError::Projection(item)),
    };
    Ok(Spanned::location(project, item.location))
}

/// Parse a literal expression
pub(crate) fn parse_literal(value: Value, location: Location) -> Result<SqlLiteral, SqlError> {
    match value {
        Value::Boolean(v) => Ok(SqlLiteral::Bool(v)),
        Value::Number(v, _) => Ok(SqlLiteral::Num(v)),
        Value::SingleQuotedString(s) => Ok(SqlLiteral::Str(s)),
        Value::HexStringLiteral(s) => Ok(SqlLiteral::Hex(s)),
        _ => Err(SqlError::Literal {
            expr: value,
            found: Box::new(location),
        }),
    }
}

/// Parse a scalar binary operator
pub(crate) fn parse_binop(op: BinaryOperator) -> Result<BinOp, SqlError> {
    match op {
        BinaryOperator::Eq => Ok(BinOp::Eq),
        BinaryOperator::NotEq => Ok(BinOp::Ne),
        BinaryOperator::Lt => Ok(BinOp::Lt),
        BinaryOperator::LtEq => Ok(BinOp::Lte),
        BinaryOperator::Gt => Ok(BinOp::Gt),
        BinaryOperator::GtEq => Ok(BinOp::Gte),
        BinaryOperator::And => Ok(BinOp::And),
        BinaryOperator::Or => Ok(BinOp::Or),
        _ => Err(SqlError::BinOp(op)),
    }
}

/// Parse a scalar expression
pub(crate) fn parse_expr(expr: Expr, location: Location) -> Result<SqlExpr, SqlError> {
    match expr {
        Expr::Value(v) => Ok(SqlExpr::Lit(parse_literal(v, location)?)),
        Expr::Identifier(ident) => Ok(SqlExpr::Var(ident.into())),
        Expr::CompoundIdentifier(mut idents) if idents.len() == 2 => {
            let table = idents.swap_remove(0).into();
            let field = idents.swap_remove(0).into();
            Ok(SqlExpr::Field(table, field))
        }
        Expr::BinaryOp { left, op, right } => {
            let l = parse_expr(*left, location.clone())?;
            let r = parse_expr(*right, location)?;
            Ok(SqlExpr::Bin(Box::new(l), Box::new(r), parse_binop(op)?))
        }
        _ => Err(SqlError::Expr {
            expr,
            found: Box::new(location),
        }),
    }
}

/// Parse an item in a SELECT clause
pub(crate) fn parse_project_elem(item: Spanned<SelectItem>) -> Result<ProjectElem, SqlError> {
    match item.value {
        SelectItem::Wildcard(_) => Err(SqlError::MixedWildcardProject),
        SelectItem::QualifiedWildcard(..) => Err(SqlError::MixedWildcardProject),
        SelectItem::UnnamedExpr(expr) => Ok(ProjectElem(parse_expr(expr, item.location)?, None)),
        SelectItem::ExprWithAlias { expr, alias } => {
            Ok(ProjectElem(parse_expr(expr, item.location)?, Some(alias.into())))
        }
    }
}
trait ParserExt {
    fn parse_select_item_spanned(&mut self) -> Result<Spanned<SelectItem>, ParserError>;
    fn parse_order_by_spanned(&mut self) -> Result<Spanned<OrderByExpr>, ParserError>;
    fn parse_table_and_joins_spanned(&mut self) -> Result<Spanned<TableWithJoins>, ParserError>;
}

impl<'a> ParserExt for Parser<'a> {
    fn parse_select_item_spanned(&mut self) -> Result<Spanned<SelectItem>, ParserError> {
        let start = self.peek_token();
        let item = self.parse_select_item()?;
        let end = self.peek_token_no_skip();
        Ok(Spanned::new(item, start, end))
    }

    fn parse_order_by_spanned(&mut self) -> Result<Spanned<OrderByExpr>, ParserError> {
        let start = self.peek_token();
        let item = self.parse_order_by_expr()?;
        let end = self.peek_token_no_skip();
        Ok(Spanned::new(item, start, end))
    }
    fn parse_table_and_joins_spanned(&mut self) -> Result<Spanned<TableWithJoins>, ParserError> {
        let start = self.peek_token();
        let item = self.parse_table_and_joins()?;
        let end = self.peek_token_no_skip();
        Ok(Spanned::new(item, start, end))
    }
}

/// SpacetimeDB SQL Parser based on [`sqlparser`]
///
/// DataFusion mostly follows existing SQL dialects via
/// `sqlparser`. However, certain statements are not valid for subscriptions
pub struct SpaceParser<'a> {
    pub parser: Parser<'a>,
    pub sql: &'a str,
}

impl<'a> SpaceParser<'a> {
    /// Create a new parser using the [`SpacetimeSqlDialect`].
    pub fn new(sql: &'a str) -> Result<Self, SqlError> {
        let dialect = &SpacetimeSqlDialect {};

        let mut tokenizer = Tokenizer::new(dialect, sql);
        let tokens = tokenizer.tokenize_with_location().map_err(Box::new)?;

        Ok(Self {
            parser: Parser::new(dialect).with_tokens_with_locations(tokens),
            sql,
        })
    }

    fn _parse_sql(sql: &'a str) -> Result<VecDeque<SqlStatement>, SqlError> {
        let mut parser = Self::new(sql)?;
        let mut stmts = VecDeque::new();
        let mut expecting_statement_delimiter = false;
        loop {
            // ignore empty statements
            while parser.parser.consume_token(&Token::SemiColon) {
                expecting_statement_delimiter = false;
            }

            if parser.parser.peek_token() == Token::EOF {
                break;
            }
            if expecting_statement_delimiter {
                return parser.expected("end of statement", parser.parser.peek_token());
            }

            let statement = parser.parse_statement()?;
            stmts.push_back(statement);
            expecting_statement_delimiter = true;
        }
        Ok(stmts)
    }

    pub fn parse_sql(sql: &'a str) -> Result<VecDeque<SqlStatement>, SqlErrorWithLocation> {
        Self::_parse_sql(sql).map_err(|e| SqlErrorWithLocation::new(e, sql))
    }

    /// Report an unexpected token
    fn expected<T>(&self, expected: &str, found: TokenWithLocation) -> Result<T, SqlError> {
        Err(SqlError::Expect {
            expected: expected.to_string(),
            found: Box::new(found),
        })
    }

    /// Parse a new expression
    pub fn parse_statement(&mut self) -> Result<SqlStatement, SqlError> {
        match self.parser.peek_token().token {
            Token::Word(w) => match w.keyword {
                Keyword::SELECT => {
                    self.parser.next_token();
                    self.parse_select()
                }
                _ => {
                    todo!("Unsupported keyword: {:?}", w.keyword);
                }
            },
            _ => {
                todo!("Unsupported token: {:?}", self.parser.peek_token());
            }
        }
    }

    fn parse_distinct(&mut self) -> Result<Option<Spanned<()>>, SqlError> {
        let start = self.parser.peek_token();
        let distinct = self.parser.parse_all_or_distinct().map_err(Box::new)?;
        let end = self.parser.peek_token_no_skip();
        Ok(distinct.map(|_| Spanned::location((), Location { start, end })))
    }

    fn parse_projection(&mut self) -> Result<Spanned<Vec<Spanned<Project>>>, SqlError> {
        let start = self.parser.peek_token();
        let projection: Result<Vec<_>, _> = self
            .parser
            .parse_comma_separated(Parser::parse_select_item_spanned)
            .map_err(Box::new)?
            .into_iter()
            .map(parse_project)
            .collect();
        let end = self.parser.peek_token_no_skip();
        Ok(Spanned::new(projection?, start, end))
    }

    fn parse_from(&mut self) -> Result<Vec<Spanned<TableWithJoins>>, SqlError> {
        let from = if self.parser.parse_keyword(Keyword::FROM) {
            self.parser
                .parse_comma_separated(Parser::parse_table_and_joins_spanned)
                .map_err(Box::new)?
        } else {
            vec![]
        };

        Ok(from)
    }

    fn parser_order_by_expr(&mut self) -> Result<Vec<Spanned<OrderByElem>>, SqlError> {
        if self.parser.parse_keywords(&[Keyword::ORDER, Keyword::BY]) {
            let result = self
                .parser
                .parse_comma_separated(Parser::parse_order_by_spanned)
                .map_err(Box::new)?;
            result
                .into_iter()
                .map(|x| {
                    let OrderByExpr {
                        expr,
                        asc,
                        nulls_first: _,
                        with_fill: _,
                    } = x.value;
                    Ok(Spanned::location(
                        OrderByElem(parse_expr(expr, x.location.clone())?, asc.unwrap_or(true)),
                        x.location,
                    ))
                })
                .collect()
        } else {
            Ok(vec![])
        }
    }

    fn parse_selection(&mut self) -> Result<Option<Spanned<Expr>>, SqlError> {
        if self.parser.parse_keyword(Keyword::WHERE) {
            let start = self.parser.peek_token();
            let item = self.parser.parse_expr().map_err(Box::new)?;
            let end = self.parser.peek_token_no_skip();
            Ok(Some(Spanned::new(item, start, end)))
        } else {
            Ok(None)
        }
    }

    fn parse_select(&mut self) -> Result<SqlStatement, SqlError> {
        let distinct = self.parse_distinct()?;

        let projection = self.parse_projection()?;
        let from = self.parse_from()?;
        let selection = self.parse_selection()?;
        let sort_by = self.parser_order_by_expr()?;

        Ok(SqlStatement::Select(Box::from(SqlSelect {
            distinct,
            projection,
            from,
            selection,
            sort_by,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::user_error::to_fancy_error;

    fn check_unsupported(sql: &str, expected: &str) {
        let parser = SpaceParser::parse_sql(sql).unwrap_err();
        let err = to_fancy_error(parser);
        match err.error.error {
            SqlError::Parse(ref boxed_err) => {
                if let ParserError::ParserErrorWithToken { msg, token: _ } = boxed_err.as_ref() {
                    assert_eq!(msg, expected);
                } else {
                    panic!("Expected ParserErrorWithToken, found: {:?}", boxed_err);
                }
            }

            x => panic!("Expected unsupported error, found: {:?}", x),
        }
    }

    #[test]
    fn test_err() {
        check_unsupported("SELECT DISTINCT ON (a) * FROM a", "unsupported DISTINCT ON");
        check_unsupported("SELECT * FROM a  WHERE a LIKE 1", "unsupported binary operator");
        check_unsupported("SELECT * FROM a LEFT JOIN b ON a = b", "unsupported JOIN");
        check_unsupported(
            "SELECT * FROM a  WHERE a = 1 ORDER BY a NULLS FIRST",
            "unsupported keyword",
        );
    }

    #[test]
    fn test_parser() {
        let sql = "SELECT DISTINCT  *,a,a.b FROM t WHERE a = 1 ORDER BY a";
        let mut parser = dbg!(SpaceParser::parse_sql(sql)).unwrap();

        let stmt = parser.pop_front().unwrap();

        dbg!(&stmt);
        match stmt {
            SqlStatement::Select(select) => {
                //assert_eq!(select.distinct, None);
                assert_eq!(select.projection.value.len(), 3);
                assert_eq!(select.from.len(), 1);
                assert!(select.selection.is_some());
                assert_eq!(select.sort_by.len(), 1);
            }
        }
    }
}
