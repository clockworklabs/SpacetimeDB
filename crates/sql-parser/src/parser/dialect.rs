use sqlparser::ast::*;
use sqlparser::dialect::{Dialect, PostgreSqlDialect, Precedence};
use sqlparser::keywords::Keyword;
use sqlparser::parser::{Parser, ParserError};
use sqlparser::tokenizer::{Token, TokenWithLocation};

// Use `Parser::expected` instead, if possible
macro_rules! parser_err {
    ($MSG:expr, $loc:expr) => {
        Err(ParserError::ParserError(format!("{}{}", $MSG, $loc)))
    };
}
// Returns a successful result if the optional expression is some
macro_rules! return_ok_if_some {
    ($e:expr) => {{
        if let Some(v) = $e {
            return Ok(v);
        }
    }};
}

const PG: PostgreSqlDialect = PostgreSqlDialect {};

#[derive(Debug)]
pub struct SpacetimeSqlDialect {}

impl SpacetimeSqlDialect {
    /// Report `Unsupported` `token` with `msg` as the error message.
    pub fn unsupported<T>(&self, msg: &str, token: TokenWithLocation) -> Result<T, ParserError> {
        Err(ParserError::ParserErrorWithToken {
            msg: format!("unsupported {msg}"),
            token,
        })
    }

    fn try_parse_expr_sub_query(&self, parser: &mut Parser) -> Result<Option<Expr>, ParserError> {
        if parser
            .parse_one_of_keywords(&[Keyword::SELECT, Keyword::WITH])
            .is_none()
        {
            return Ok(None);
        }
        parser.prev_token();

        Ok(Some(Expr::Subquery(parser.parse_boxed_query()?)))
    }

    fn try_parse_lambda(&self) -> Option<Expr> {
        None
    }

    fn _parse_prefix(&self, parser: &mut Parser) -> Result<Expr, ParserError> {
        // PostgreSQL allows any string literal to be preceded by a type name, indicating that the
        // string literal represents a literal of that type. Some examples:
        //
        //      DATE '2020-05-20'
        //      TIMESTAMP WITH TIME ZONE '2020-05-20 7:43:54'
        //      BOOL 'true'
        //
        // The first two are standard SQL, while the latter is a PostgreSQL extension. Complicating
        // matters is the fact that INTERVAL string literals may optionally be followed by special
        // keywords, e.g.:
        //
        //      INTERVAL '7' DAY
        //
        // Note also that naively `SELECT date` looks like a syntax error because the `date` type
        // name is not followed by a string literal, but in fact in PostgreSQL it is a valid
        // expression that should parse as the column name "date".
        let token = &parser.peek_token();
        return_ok_if_some!(parser.maybe_parse(|parser| {
            match parser.parse_data_type()? {
                // PostgreSQL allows almost any identifier to be used as custom data type name,
                // and we support that in `parse_data_type()`. But unlike Postgres we don't
                // have a list of globally reserved keywords (since they vary across dialects),
                // so given `NOT 'a' LIKE 'b'`, we'd accept `NOT` as a possible custom data type
                // name, resulting in `NOT 'a'` being recognized as a `TypedString` instead of
                // an unary negation `NOT ('a' LIKE 'b')`. To solve this, we don't accept the
                // `type 'string'` syntax for the custom data types at all.
                DataType::Custom(..) => parser_err!("dummy", token.location),
                _ => self.unsupported("type name before string literal", token.clone()),
            }
        }));

        let next_token = parser.next_token();
        let expr = match next_token.token.clone() {
            Token::Word(w) => match w.keyword {
                Keyword::TRUE | Keyword::FALSE | Keyword::NULL => {
                    parser.prev_token();
                    Ok(Expr::Value(parser.parse_value()?))
                }
                Keyword::CURRENT_CATALOG
                | Keyword::CURRENT_USER
                | Keyword::SESSION_USER
                | Keyword::USER
                | Keyword::CURRENT_TIMESTAMP
                | Keyword::CURRENT_TIME
                | Keyword::CURRENT_DATE
                | Keyword::LOCALTIME
                | Keyword::LOCALTIMESTAMP
                | Keyword::CASE
                | Keyword::CONVERT
                | Keyword::CAST
                | Keyword::TRY_CAST
                | Keyword::SAFE_CAST
                | Keyword::EXISTS
                | Keyword::EXTRACT
                | Keyword::CEIL
                | Keyword::FLOOR
                | Keyword::POSITION
                | Keyword::SUBSTRING
                | Keyword::OVERLAY
                | Keyword::TRIM
                | Keyword::INTERVAL => self.unsupported("function", next_token),
                // Treat ARRAY[1,2,3] as an array [1,2,3], otherwise try as subquery or a function call
                Keyword::ARRAY if parser.peek_token() == Token::LBracket => self.unsupported("value", next_token),
                Keyword::ARRAY if parser.peek_token() == Token::LParen => self.unsupported("function", next_token),
                Keyword::NOT => parser.parse_not(),
                Keyword::MATCH | Keyword::STRUCT | Keyword::PRIOR | Keyword::MAP => {
                    self.unsupported("keyword", next_token)
                }
                // Here `w` is a word, check if it's a part of a multipart
                // identifier, a function call, or a simple identifier:
                _ => match parser.peek_token().token {
                    Token::LParen | Token::Period => {
                        let mut id_parts: Vec<Ident> = vec![w.to_ident()];
                        let mut ends_with_wildcard = false;
                        while parser.consume_token(&Token::Period) {
                            let next_token = parser.next_token();
                            match next_token.token {
                                Token::Word(w) => id_parts.push(w.to_ident()),
                                Token::Mul => {
                                    // Postgres explicitly allows funcnm(tablenm.*) and the
                                    // function array_agg traverses this control flow

                                    ends_with_wildcard = true;
                                    break;
                                }
                                Token::SingleQuotedString(s) => id_parts.push(Ident::with_quote('\'', s)),
                                _ => {
                                    return parser.expected("an identifier or a '*' after '.'", next_token);
                                }
                            }
                        }

                        if ends_with_wildcard {
                            Ok(Expr::QualifiedWildcard(ObjectName(id_parts)))
                        } else if parser.consume_token(&Token::LParen) {
                            self.unsupported("outer join", next_token)
                        } else {
                            Ok(Expr::CompoundIdentifier(id_parts))
                        }
                    }
                    // string introducer https://dev.mysql.com/doc/refman/8.0/en/charset-introducer.html
                    Token::SingleQuotedString(_) | Token::DoubleQuotedString(_) | Token::HexStringLiteral(_)
                        if w.value.starts_with('_') =>
                    {
                        self.unsupported("string introducer", next_token)
                    }
                    Token::Arrow if self.supports_lambda_functions() => {
                        unreachable!("Lambda functions are not supported")
                    }
                    _ => Ok(Expr::Identifier(w.to_ident())),
                },
            }, // End of Token::Word
            // array `[1, 2, 3]`
            Token::LBracket => parser.parse_array_expr(false),
            tok @ Token::Minus | tok @ Token::Plus => {
                let op = if tok == Token::Plus {
                    UnaryOperator::Plus
                } else {
                    UnaryOperator::Minus
                };
                Ok(Expr::UnaryOp {
                    op,
                    expr: Box::new(parser.parse_subexpr(self.prec_value(Precedence::MulDivModOp))?),
                })
            }
            Token::DoubleExclamationMark | Token::PGSquareRoot | Token::PGCubeRoot | Token::AtSign | Token::Tilde => {
                self.unsupported("prefix operator", next_token)
            }
            Token::EscapedStringLiteral(_) => {
                parser.prev_token();
                Ok(Expr::Value(parser.parse_value()?))
            }
            Token::UnicodeStringLiteral(_) => {
                parser.prev_token();
                Ok(Expr::Value(parser.parse_value()?))
            }
            Token::Number(_, _)
            | Token::SingleQuotedString(_)
            | Token::DoubleQuotedString(_)
            | Token::TripleSingleQuotedString(_)
            | Token::TripleDoubleQuotedString(_)
            | Token::DollarQuotedString(_)
            | Token::SingleQuotedByteStringLiteral(_)
            | Token::DoubleQuotedByteStringLiteral(_)
            | Token::TripleSingleQuotedByteStringLiteral(_)
            | Token::TripleDoubleQuotedByteStringLiteral(_)
            | Token::SingleQuotedRawStringLiteral(_)
            | Token::DoubleQuotedRawStringLiteral(_)
            | Token::TripleSingleQuotedRawStringLiteral(_)
            | Token::TripleDoubleQuotedRawStringLiteral(_)
            | Token::NationalStringLiteral(_)
            | Token::HexStringLiteral(_) => {
                parser.prev_token();
                Ok(Expr::Value(parser.parse_value()?))
            }
            Token::LParen => {
                let expr = if let Some(expr) = self.try_parse_expr_sub_query(parser)? {
                    expr
                } else if let Some(lambda) = self.try_parse_lambda() {
                    return Ok(lambda);
                } else {
                    let exprs = parser.parse_comma_separated(Parser::parse_expr)?;
                    match exprs.len() {
                        0 => unreachable!(), // parse_comma_separated ensures 1 or more
                        1 => Expr::Nested(Box::new(exprs.into_iter().next().unwrap())),
                        _ => Expr::Tuple(exprs),
                    }
                };
                parser.expect_token(&Token::RParen)?;
                if !parser.consume_token(&Token::Period) {
                    Ok(expr)
                } else {
                    let tok = parser.next_token();
                    let key = match tok.token {
                        Token::Word(word) => word.to_ident(),
                        _ => return parser_err!(format!("Expected identifier, found: {tok}"), tok.location),
                    };
                    Ok(Expr::CompositeAccess {
                        expr: Box::new(expr),
                        key,
                    })
                }
            }
            Token::Placeholder(_) | Token::Colon => {
                parser.prev_token();
                Ok(Expr::Value(parser.parse_value()?))
            }
            Token::LBrace if self.supports_dictionary_syntax() => {
                unimplemented!("Dictionary syntax is not supported")
            }
            _ => parser.expected("an expression:", next_token),
        }?;

        if parser.parse_keyword(Keyword::COLLATE) {
            self.unsupported("collate", parser.peek_token())
        } else {
            Ok(expr)
        }
    }

    fn _parse_infix(&self, parser: &mut Parser, expr: &Expr, precedence: u8) -> Result<Expr, ParserError> {
        let mut tok = parser.next_token();
        let regular_binary_operator = match &mut tok.token {
            Token::Eq => Some(BinaryOperator::Eq),
            Token::Neq => Some(BinaryOperator::NotEq),
            Token::Gt => Some(BinaryOperator::Gt),
            Token::GtEq => Some(BinaryOperator::GtEq),
            Token::Lt => Some(BinaryOperator::Lt),
            Token::LtEq => Some(BinaryOperator::LtEq),
            Token::Spaceship
            | Token::DoubleEq
            | Token::Plus
            | Token::Minus
            | Token::Mul
            | Token::Mod
            | Token::StringConcat
            | Token::Pipe
            | Token::Caret
            | Token::Ampersand
            | Token::Div
            | Token::DuckIntDiv
            | Token::ShiftLeft
            | Token::ShiftRight
            | Token::Sharp
            | Token::Overlap
            | Token::CaretAt
            | Token::Tilde
            | Token::TildeAsterisk
            | Token::ExclamationMarkTilde
            | Token::ExclamationMarkTildeAsterisk
            | Token::DoubleTilde
            | Token::DoubleTildeAsterisk
            | Token::ExclamationMarkDoubleTilde
            | Token::ExclamationMarkDoubleTildeAsterisk
            | Token::Arrow
            | Token::LongArrow
            | Token::HashArrow
            | Token::HashLongArrow
            | Token::AtArrow
            | Token::ArrowAt
            | Token::HashMinus
            | Token::AtQuestion
            | Token::AtAt
            | Token::Question
            | Token::QuestionAnd
            | Token::QuestionPipe
            | Token::CustomBinaryOperator(_) => return self.unsupported("binary operator", tok),
            Token::Word(w) => match w.keyword {
                Keyword::AND => Some(BinaryOperator::And),
                Keyword::OR => Some(BinaryOperator::Or),
                _ => return self.unsupported("binary operator", tok),
            },
            _ => None,
        };

        if let Some(op) = regular_binary_operator {
            if parser.parse_one_of_keywords(&[Keyword::ANY, Keyword::ALL]).is_some() {
                self.unsupported("binary operator", tok)
            } else {
                Ok(Expr::BinaryOp {
                    left: Box::new(expr.clone()),
                    op,
                    right: Box::new(parser.parse_subexpr(precedence)?),
                })
            }
        } else if let Token::Word(w) = &tok.token {
            match w.keyword {
                Keyword::IS
                | Keyword::AT
                | Keyword::NOT
                | Keyword::IN
                | Keyword::BETWEEN
                | Keyword::LIKE
                | Keyword::ILIKE
                | Keyword::SIMILAR
                | Keyword::REGEXP
                | Keyword::RLIKE => self.unsupported("keyword", tok),
                // Can only happen if `get_next_precedence` got out of sync with this function
                _ => parser_err!(format!("No infix parser for token {:?}", tok.token), tok.location),
            }
        } else if Token::DoubleColon == tok {
            self.unsupported("operator", tok)
        } else if Token::ExclamationMark == tok {
            // PostgreSQL factorial operation
            self.unsupported("factorial", tok)
        } else if Token::LBracket == tok {
            self.unsupported("subscript", tok)
        } else {
            // Can only happen if `get_next_precedence` got out of sync with this function
            parser_err!(format!("No infix parser for token {:?}", tok.token), tok.location)
        }
    }

    fn _parse_table_and_joins(&self, parser: &mut Parser) -> Result<TableWithJoins, ParserError> {
        let relation = parser.parse_table_factor()?;
        // Note that for keywords to be properly handled here, they need to be
        // added to `RESERVED_FOR_TABLE_ALIAS`, otherwise they may be parsed as
        // a table alias.
        let mut joins = vec![];
        loop {
            let next_token = parser.peek_token();
            if parser.parse_keywords(&[
                Keyword::GLOBAL,
                Keyword::CROSS,
                Keyword::OUTER,
                Keyword::ASOF,
                Keyword::NATURAL,
            ]) {
                return self.unsupported("JOIN", next_token);
            }
            let join = {
                let peek_keyword = if let Token::Word(w) = next_token.token {
                    w.keyword
                } else {
                    Keyword::NoKeyword
                };

                let join_operator_type = match peek_keyword {
                    Keyword::INNER | Keyword::JOIN => {
                        let _ = parser.parse_keyword(Keyword::INNER); // [ INNER ]
                        parser.expect_keyword(Keyword::JOIN)?;
                        JoinOperator::Inner
                    }
                    Keyword::LEFT | Keyword::RIGHT | Keyword::FULL | Keyword::OUTER => {
                        return self.unsupported("JOIN", parser.peek_token())
                    }
                    _ => break,
                };

                let relation = parser.parse_table_factor()?;
                let join_constraint = parser.parse_join_constraint(false)?;
                Join {
                    relation,
                    global: false,
                    join_operator: join_operator_type(join_constraint),
                }
            };
            joins.push(join);
        }
        Ok(TableWithJoins { relation, joins })
    }

    fn _parse_order_by_expr(&self, parser: &mut Parser) -> Result<OrderByExpr, ParserError> {
        let expr = parser.parse_expr()?;

        let asc = if parser.parse_keyword(Keyword::ASC) {
            Some(true)
        } else if parser.parse_keyword(Keyword::DESC) {
            Some(false)
        } else {
            None
        };

        let current = parser.peek_token();
        if parser.parse_keywords(&[Keyword::NULLS, Keyword::FIRST])
            || parser.parse_keywords(&[Keyword::NULLS, Keyword::LAST])
            || parser.parse_keywords(&[Keyword::WITH, Keyword::FILL])
        {
            return self.unsupported("keyword", current);
        };

        Ok(OrderByExpr {
            expr,
            asc,
            nulls_first: None,
            with_fill: None,
        })
    }

    fn _parse_all_or_distinct(&self, parser: &mut Parser) -> Result<Option<Distinct>, ParserError> {
        let start = parser.peek_token();
        if parser.parse_keywords([Keyword::ALL, Keyword::TOP].as_ref()) {
            return self.unsupported("keyword", start);
        }

        let distinct = parser.parse_keyword(Keyword::DISTINCT);
        if !distinct {
            return Ok(None);
        }

        let on = parser.parse_keyword(Keyword::ON);
        if !on {
            return Ok(Some(Distinct::Distinct));
        }
        let end = parser.peek_token();
        if end == Token::LParen {
            self.unsupported("DISTINCT ON", end)
        } else {
            Ok(None)
        }
    }
}

impl Dialect for SpacetimeSqlDialect {
    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        PG.is_delimited_identifier_start(ch)
    }

    fn identifier_quote_style(&self, identifier: &str) -> Option<char> {
        PG.identifier_quote_style(identifier)
    }

    fn is_identifier_start(&self, ch: char) -> bool {
        PG.is_identifier_start(ch)
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        PG.is_identifier_part(ch)
    }

    fn is_custom_operator_part(&self, ch: char) -> bool {
        PG.is_custom_operator_part(ch)
    }

    fn supports_unicode_string_literal(&self) -> bool {
        PG.supports_unicode_string_literal()
    }

    fn supports_filter_during_aggregation(&self) -> bool {
        false
    }

    fn supports_group_by_expr(&self) -> bool {
        false
    }

    fn parse_prefix(&self, parser: &mut Parser) -> Option<Result<Expr, ParserError>> {
        Some(self._parse_prefix(parser))
    }

    fn parse_infix(&self, parser: &mut Parser, expr: &Expr, precedence: u8) -> Option<Result<Expr, ParserError>> {
        Some(self._parse_infix(parser, expr, precedence))
    }

    fn parse_table_and_joins(&self, parser: &mut Parser) -> Option<Result<TableWithJoins, ParserError>> {
        Some(self._parse_table_and_joins(parser))
    }

    fn parse_order_by_expr(&self, parser: &mut Parser) -> Option<Result<OrderByExpr, ParserError>> {
        Some(self._parse_order_by_expr(parser))
    }

    fn parse_all_or_distinct(&self, parser: &mut Parser) -> Option<Result<Option<Distinct>, ParserError>> {
        Some(self._parse_all_or_distinct(parser))
    }
    fn get_next_precedence(&self, parser: &Parser) -> Option<Result<u8, ParserError>> {
        PG.get_next_precedence(parser)
    }

    fn parse_statement(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        PG.parse_statement(parser)
    }

    fn prec_value(&self, prec: Precedence) -> u8 {
        PG.prec_value(prec)
    }
}
