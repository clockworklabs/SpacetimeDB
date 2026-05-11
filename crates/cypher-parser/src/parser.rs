use crate::ast::*;
use crate::error::{CypherParseError, CypherParseResult};
use crate::lexer::{Lexer, Token};

/// Parse an openCypher query string into a [`CypherQuery`] AST.
pub fn parse_cypher(input: &str) -> CypherParseResult<CypherQuery> {
    let tokens = Lexer::tokenize(input)?;
    let mut parser = Parser::new(&tokens);
    let query = parser.parse_query()?;
    parser.expect_eof()?;
    Ok(query)
}

struct Parser<'a> {
    tokens: &'a [(Token, usize)],
    cursor: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [(Token, usize)]) -> Self {
        Self { tokens, cursor: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.cursor)
            .map(|(t, _)| t)
            .unwrap_or(&Token::Eof)
    }

    fn pos(&self) -> usize {
        self.tokens
            .get(self.cursor)
            .map(|(_, p)| *p)
            .unwrap_or(0)
    }

    fn advance(&mut self) -> &Token {
        let tok = self.tokens.get(self.cursor).map(|(t, _)| t).unwrap_or(&Token::Eof);
        if self.cursor < self.tokens.len() {
            self.cursor += 1;
        }
        tok
    }

    fn eat(&mut self, expected: &Token, label: &'static str) -> CypherParseResult<()> {
        let pos = self.pos();
        let tok = self.advance().clone();
        if &tok != expected {
            return Err(CypherParseError::UnexpectedToken {
                found: tok,
                pos,
                expected: label,
            });
        }
        Ok(())
    }

    fn expect_eof(&self) -> CypherParseResult<()> {
        if *self.peek() != Token::Eof {
            return Err(CypherParseError::UnexpectedToken {
                found: self.peek().clone(),
                pos: self.pos(),
                expected: "end of input",
            });
        }
        Ok(())
    }

    // ── query ──────────────────────────────────────────────────────────

    fn parse_query(&mut self) -> CypherParseResult<CypherQuery> {
        if *self.peek() == Token::Eof {
            return Err(CypherParseError::EmptyQuery);
        }

        let mut match_clause = self.parse_match()?;

        // Multiple MATCH clauses: `MATCH … MATCH …` merges into one pattern list.
        while *self.peek() == Token::Match {
            let next = self.parse_match()?;
            match_clause.patterns.extend(next.patterns);
        }

        let where_clause = if *self.peek() == Token::Where {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        let return_clause = self.parse_return()?;

        Ok(CypherQuery {
            match_clause,
            where_clause,
            return_clause,
        })
    }

    // ── MATCH ──────────────────────────────────────────────────────────

    fn parse_match(&mut self) -> CypherParseResult<MatchClause> {
        self.eat(&Token::Match, "MATCH")?;
        let mut patterns = vec![self.parse_pattern()?];
        while *self.peek() == Token::Comma {
            self.advance();
            patterns.push(self.parse_pattern()?);
        }
        Ok(MatchClause { patterns })
    }

    fn parse_pattern(&mut self) -> CypherParseResult<Pattern> {
        let first_node = self.parse_node_pattern()?;
        let mut nodes = vec![first_node];
        let mut edges = Vec::new();

        loop {
            match self.peek() {
                Token::Dash | Token::LArrow => {
                    let (rel, node) = self.parse_rel_and_node()?;
                    edges.push(rel);
                    nodes.push(node);
                }
                _ => break,
            }
        }

        Ok(Pattern { nodes, edges })
    }

    fn parse_node_pattern(&mut self) -> CypherParseResult<NodePattern> {
        self.eat(&Token::LParen, "'('")?;

        let variable = if let Token::Ident(_) = self.peek() {
            let Token::Ident(name) = self.advance().clone() else {
                unreachable!()
            };
            Some(name)
        } else {
            None
        };

        let label = if *self.peek() == Token::Colon {
            self.advance();
            Some(self.expect_ident("label name")?)
        } else {
            None
        };

        let properties = if *self.peek() == Token::LBrace {
            self.parse_map_literal()?
        } else {
            Vec::new()
        };

        self.eat(&Token::RParen, "')'")?;
        Ok(NodePattern {
            variable,
            label,
            properties,
        })
    }

    /// Parse `-[…]->`, `<-[…]-`, or `-[…]-` followed by a node pattern.
    fn parse_rel_and_node(&mut self) -> CypherParseResult<(RelPattern, NodePattern)> {
        let left_arrow = *self.peek() == Token::LArrow;
        if left_arrow {
            self.advance(); // consume `<-`
        } else {
            self.eat(&Token::Dash, "'-'")?;
        }

        let (variable, rel_type, length) = if *self.peek() == Token::LBracket {
            self.advance();
            let v = if let Token::Ident(_) = self.peek() {
                let Token::Ident(name) = self.advance().clone() else {
                    unreachable!()
                };
                Some(name)
            } else {
                None
            };
            let t = if *self.peek() == Token::Colon {
                self.advance();
                Some(self.expect_ident("relationship type")?)
            } else {
                None
            };
            let l = if *self.peek() == Token::Star {
                Some(self.parse_path_length()?)
            } else {
                None
            };
            self.eat(&Token::RBracket, "']'")?;
            (v, t, l)
        } else {
            (None, None, None)
        };

        let direction = if left_arrow {
            self.eat(&Token::Dash, "'-' (closing <-[…]-)")?;
            Direction::Incoming
        } else if *self.peek() == Token::Arrow {
            self.advance();
            Direction::Outgoing
        } else {
            self.eat(&Token::Dash, "'-' or '->' (relationship end)")?;
            Direction::Undirected
        };

        let node = self.parse_node_pattern()?;
        Ok((
            RelPattern {
                variable,
                rel_type,
                length,
                direction,
            },
            node,
        ))
    }

    fn parse_path_length(&mut self) -> CypherParseResult<PathLength> {
        self.eat(&Token::Star, "'*'")?;
        match self.peek() {
            Token::Integer(n) => {
                let n = *n;
                let pos = self.pos();
                let n = u32::try_from(n).map_err(|_| CypherParseError::PathLengthOutOfRange {
                    value: n,
                    pos,
                })?;
                self.advance();
                if *self.peek() == Token::DotDot {
                    self.advance();
                    if let Token::Integer(m) = self.peek() {
                        let m = *m;
                        let pos = self.pos();
                        let m =
                            u32::try_from(m).map_err(|_| CypherParseError::PathLengthOutOfRange {
                                value: m,
                                pos,
                            })?;
                        self.advance();
                        Ok(PathLength::Range {
                            min: Some(n),
                            max: Some(m),
                        })
                    } else {
                        Ok(PathLength::Range {
                            min: Some(n),
                            max: None,
                        })
                    }
                } else {
                    Ok(PathLength::Exact(n))
                }
            }
            Token::DotDot => {
                self.advance();
                if let Token::Integer(m) = self.peek() {
                    let m = *m;
                    let pos = self.pos();
                    let m =
                        u32::try_from(m).map_err(|_| CypherParseError::PathLengthOutOfRange {
                            value: m,
                            pos,
                        })?;
                    self.advance();
                    Ok(PathLength::Range {
                        min: None,
                        max: Some(m),
                    })
                } else {
                    Ok(PathLength::Unbounded)
                }
            }
            _ => Ok(PathLength::Unbounded),
        }
    }

    // ── RETURN ─────────────────────────────────────────────────────────

    fn parse_return(&mut self) -> CypherParseResult<ReturnClause> {
        self.eat(&Token::Return, "RETURN")?;

        if *self.peek() == Token::Star {
            self.advance();
            return Ok(ReturnClause::All);
        }

        let mut items = vec![self.parse_return_item()?];
        while *self.peek() == Token::Comma {
            self.advance();
            items.push(self.parse_return_item()?);
        }
        Ok(ReturnClause::Items(items))
    }

    fn parse_return_item(&mut self) -> CypherParseResult<ReturnItem> {
        let expr = self.parse_expr()?;
        let alias = if *self.peek() == Token::As {
            self.advance();
            Some(self.expect_ident("alias")?)
        } else {
            None
        };
        Ok(ReturnItem { expr, alias })
    }

    // ── expressions ────────────────────────────────────────────────────

    fn parse_expr(&mut self) -> CypherParseResult<CypherExpr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> CypherParseResult<CypherExpr> {
        let mut left = self.parse_and()?;
        while *self.peek() == Token::Or {
            self.advance();
            let right = self.parse_and()?;
            left = CypherExpr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> CypherParseResult<CypherExpr> {
        let mut left = self.parse_not()?;
        while *self.peek() == Token::And {
            self.advance();
            let right = self.parse_not()?;
            left = CypherExpr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> CypherParseResult<CypherExpr> {
        if *self.peek() == Token::Not {
            self.advance();
            let inner = self.parse_not()?;
            return Ok(CypherExpr::Not(Box::new(inner)));
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> CypherParseResult<CypherExpr> {
        let left = self.parse_primary()?;
        let op = match self.peek() {
            Token::Eq => Some(CmpOp::Eq),
            Token::Ne => Some(CmpOp::Ne),
            Token::Lt => Some(CmpOp::Lt),
            Token::Gt => Some(CmpOp::Gt),
            Token::Lte => Some(CmpOp::Lte),
            Token::Gte => Some(CmpOp::Gte),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let right = self.parse_primary()?;
            Ok(CypherExpr::Cmp(Box::new(left), op, Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_primary(&mut self) -> CypherParseResult<CypherExpr> {
        match self.peek() {
            Token::Integer(_)
            | Token::Float(_)
            | Token::StringLit(_)
            | Token::True
            | Token::False
            | Token::Null
            | Token::Dash => Ok(CypherExpr::Lit(self.parse_literal_value()?)),
            Token::Ident(_) => {
                let Token::Ident(name) = self.advance().clone() else {
                    unreachable!()
                };
                if *self.peek() == Token::Dot {
                    self.advance();
                    let prop = self.expect_ident("property name")?;
                    Ok(CypherExpr::Prop(Box::new(CypherExpr::Var(name)), prop))
                } else {
                    Ok(CypherExpr::Var(name))
                }
            }
            Token::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.eat(&Token::RParen, "')'")?;
                Ok(inner)
            }
            _ => Err(CypherParseError::UnexpectedToken {
                found: self.peek().clone(),
                pos: self.pos(),
                expected: "expression",
            }),
        }
    }

    // ── helpers ─────────────────────────────────────────────────────────

    fn expect_ident(&mut self, expected: &'static str) -> CypherParseResult<String> {
        let pos = self.pos();
        match self.advance().clone() {
            Token::Ident(name) => Ok(name),
            other => Err(CypherParseError::UnexpectedToken {
                found: other,
                pos,
                expected,
            }),
        }
    }

    fn parse_map_literal(&mut self) -> CypherParseResult<Vec<(String, CypherLiteral)>> {
        self.eat(&Token::LBrace, "'{'")?;
        let mut props = Vec::new();
        if *self.peek() != Token::RBrace {
            loop {
                let key = self.expect_ident("property key")?;
                self.eat(&Token::Colon, "':'")?;
                let val = self.parse_literal_value()?;
                props.push((key, val));
                if *self.peek() != Token::Comma {
                    break;
                }
                self.advance();
            }
        }
        self.eat(&Token::RBrace, "'}'")?;
        Ok(props)
    }

    fn parse_literal_value(&mut self) -> CypherParseResult<CypherLiteral> {
        match self.peek().clone() {
            Token::Integer(n) => {
                self.advance();
                Ok(CypherLiteral::Integer(n))
            }
            Token::Float(f) => {
                self.advance();
                Ok(CypherLiteral::Float(f))
            }
            Token::StringLit(s) => {
                self.advance();
                Ok(CypherLiteral::String(s))
            }
            Token::True => {
                self.advance();
                Ok(CypherLiteral::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(CypherLiteral::Bool(false))
            }
            Token::Null => {
                self.advance();
                Ok(CypherLiteral::Null)
            }
            Token::Dash => {
                self.advance();
                match self.peek().clone() {
                    Token::Integer(n) => {
                        self.advance();
                        Ok(CypherLiteral::Integer(-n))
                    }
                    Token::Float(f) => {
                        self.advance();
                        Ok(CypherLiteral::Float(-f))
                    }
                    _ => Err(CypherParseError::UnexpectedToken {
                        found: self.peek().clone(),
                        pos: self.pos(),
                        expected: "number after '-'",
                    }),
                }
            }
            other => Err(CypherParseError::UnexpectedToken {
                found: other,
                pos: self.pos(),
                expected: "literal value",
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── valid queries ──────────────────────────────────────────────────

    #[test]
    fn minimal_match_return() {
        let q = parse_cypher("MATCH (n) RETURN n").unwrap();
        assert_eq!(q.match_clause.patterns.len(), 1);
        assert_eq!(q.match_clause.patterns[0].nodes.len(), 1);
        assert_eq!(
            q.match_clause.patterns[0].nodes[0].variable.as_deref(),
            Some("n")
        );
        assert!(q.where_clause.is_none());
        assert!(matches!(q.return_clause, ReturnClause::Items(ref items) if items.len() == 1));
    }

    #[test]
    fn match_return_star() {
        let q = parse_cypher("MATCH (n) RETURN *").unwrap();
        assert!(matches!(q.return_clause, ReturnClause::All));
    }

    #[test]
    fn match_with_label() {
        let q = parse_cypher("MATCH (p:Person) RETURN p").unwrap();
        let node = &q.match_clause.patterns[0].nodes[0];
        assert_eq!(node.variable.as_deref(), Some("p"));
        assert_eq!(node.label.as_deref(), Some("Person"));
    }

    #[test]
    fn match_with_properties() {
        let q = parse_cypher("MATCH (p:Person {name: 'Alice', age: 30}) RETURN p").unwrap();
        let node = &q.match_clause.patterns[0].nodes[0];
        assert_eq!(node.properties.len(), 2);
        assert_eq!(node.properties[0].0, "name");
        assert_eq!(
            node.properties[0].1,
            CypherLiteral::String("Alice".to_string())
        );
        assert_eq!(node.properties[1].0, "age");
        assert_eq!(node.properties[1].1, CypherLiteral::Integer(30));
    }

    #[test]
    fn outgoing_relationship() {
        let q = parse_cypher("MATCH (a)-[r:KNOWS]->(b) RETURN a, b").unwrap();
        let pat = &q.match_clause.patterns[0];
        assert_eq!(pat.nodes.len(), 2);
        assert_eq!(pat.edges.len(), 1);
        assert_eq!(pat.edges[0].direction, Direction::Outgoing);
        assert_eq!(pat.edges[0].rel_type.as_deref(), Some("KNOWS"));
        assert_eq!(pat.edges[0].variable.as_deref(), Some("r"));
    }

    #[test]
    fn incoming_relationship() {
        let q = parse_cypher("MATCH (a)<-[r:KNOWS]-(b) RETURN a").unwrap();
        assert_eq!(q.match_clause.patterns[0].edges[0].direction, Direction::Incoming);
    }

    #[test]
    fn undirected_relationship() {
        let q = parse_cypher("MATCH (a)-[r:KNOWS]-(b) RETURN a").unwrap();
        assert_eq!(
            q.match_clause.patterns[0].edges[0].direction,
            Direction::Undirected
        );
    }

    #[test]
    fn bare_relationship_no_brackets() {
        let q = parse_cypher("MATCH (a)--(b) RETURN a").unwrap();
        let pat = &q.match_clause.patterns[0];
        assert_eq!(pat.edges.len(), 1);
        assert_eq!(pat.edges[0].direction, Direction::Undirected);
        assert!(pat.edges[0].variable.is_none());
        assert!(pat.edges[0].rel_type.is_none());
    }

    #[test]
    fn bare_outgoing_no_brackets() {
        let q = parse_cypher("MATCH (a)-->(b) RETURN a").unwrap();
        let pat = &q.match_clause.patterns[0];
        assert_eq!(pat.edges[0].direction, Direction::Outgoing);
    }

    #[test]
    fn bare_incoming_no_brackets() {
        let q = parse_cypher("MATCH (a)<--(b) RETURN a").unwrap();
        let pat = &q.match_clause.patterns[0];
        assert_eq!(pat.edges[0].direction, Direction::Incoming);
    }

    #[test]
    fn chain_of_three_nodes() {
        let q = parse_cypher("MATCH (a)-[r1:KNOWS]->(b)-[r2:LIVES_IN]->(c) RETURN a, c").unwrap();
        let pat = &q.match_clause.patterns[0];
        assert_eq!(pat.nodes.len(), 3);
        assert_eq!(pat.edges.len(), 2);
    }

    #[test]
    fn multiple_patterns() {
        let q = parse_cypher("MATCH (a)-[:KNOWS]->(b), (c)-[:WORKS_AT]->(d) RETURN a, d").unwrap();
        assert_eq!(q.match_clause.patterns.len(), 2);
    }

    #[test]
    fn multiple_match_clauses() {
        let q = parse_cypher("MATCH (a:Person) MATCH (b:Company) RETURN a, b").unwrap();
        assert_eq!(q.match_clause.patterns.len(), 2);
        assert_eq!(q.match_clause.patterns[0].nodes[0].label.as_deref(), Some("Person"));
        assert_eq!(q.match_clause.patterns[1].nodes[0].label.as_deref(), Some("Company"));
    }

    #[test]
    fn multiple_match_clauses_with_edges() {
        let q = parse_cypher("MATCH (a)-[:KNOWS]->(b) MATCH (b)-[:WORKS_AT]->(c) RETURN a, c").unwrap();
        assert_eq!(q.match_clause.patterns.len(), 2);
        assert_eq!(q.match_clause.patterns[0].edges[0].rel_type.as_deref(), Some("KNOWS"));
        assert_eq!(q.match_clause.patterns[1].edges[0].rel_type.as_deref(), Some("WORKS_AT"));
    }

    #[test]
    fn three_match_clauses() {
        let q = parse_cypher("MATCH (a) MATCH (b) MATCH (c) RETURN a, b, c").unwrap();
        assert_eq!(q.match_clause.patterns.len(), 3);
    }

    // ── variable-length paths ──────────────────────────────────────────

    #[test]
    fn path_length_unbounded() {
        let q = parse_cypher("MATCH (a)-[*]->(b) RETURN a").unwrap();
        assert_eq!(
            q.match_clause.patterns[0].edges[0].length,
            Some(PathLength::Unbounded)
        );
    }

    #[test]
    fn path_length_exact() {
        let q = parse_cypher("MATCH (a)-[*3]->(b) RETURN a").unwrap();
        assert_eq!(
            q.match_clause.patterns[0].edges[0].length,
            Some(PathLength::Exact(3))
        );
    }

    #[test]
    fn path_length_range() {
        let q = parse_cypher("MATCH (a)-[*1..5]->(b) RETURN a").unwrap();
        assert_eq!(
            q.match_clause.patterns[0].edges[0].length,
            Some(PathLength::Range {
                min: Some(1),
                max: Some(5)
            })
        );
    }

    #[test]
    fn path_length_open_ended_max() {
        let q = parse_cypher("MATCH (a)-[*2..]->(b) RETURN a").unwrap();
        assert_eq!(
            q.match_clause.patterns[0].edges[0].length,
            Some(PathLength::Range {
                min: Some(2),
                max: None
            })
        );
    }

    #[test]
    fn path_length_open_ended_min() {
        let q = parse_cypher("MATCH (a)-[*..5]->(b) RETURN a").unwrap();
        assert_eq!(
            q.match_clause.patterns[0].edges[0].length,
            Some(PathLength::Range {
                min: None,
                max: Some(5)
            })
        );
    }

    #[test]
    fn path_length_with_type() {
        let q = parse_cypher("MATCH (a)-[r:KNOWS *1..3]->(b) RETURN a").unwrap();
        let edge = &q.match_clause.patterns[0].edges[0];
        assert_eq!(edge.rel_type.as_deref(), Some("KNOWS"));
        assert_eq!(edge.variable.as_deref(), Some("r"));
        assert_eq!(
            edge.length,
            Some(PathLength::Range {
                min: Some(1),
                max: Some(3)
            })
        );
    }

    // ── WHERE clause ───────────────────────────────────────────────────

    #[test]
    fn where_simple_comparison() {
        let q = parse_cypher("MATCH (n:Person) WHERE n.age > 25 RETURN n").unwrap();
        assert!(q.where_clause.is_some());
        let expr = q.where_clause.unwrap();
        assert!(matches!(expr, CypherExpr::Cmp(_, CmpOp::Gt, _)));
    }

    #[test]
    fn where_and() {
        let q = parse_cypher("MATCH (n) WHERE n.a = 1 AND n.b = 2 RETURN n").unwrap();
        assert!(matches!(q.where_clause, Some(CypherExpr::And(_, _))));
    }

    #[test]
    fn where_or() {
        let q = parse_cypher("MATCH (n) WHERE n.a = 1 OR n.b = 2 RETURN n").unwrap();
        assert!(matches!(q.where_clause, Some(CypherExpr::Or(_, _))));
    }

    #[test]
    fn where_not() {
        let q = parse_cypher("MATCH (n) WHERE NOT n.active = false RETURN n").unwrap();
        assert!(matches!(q.where_clause, Some(CypherExpr::Not(_))));
    }

    #[test]
    fn where_parenthesized() {
        let q =
            parse_cypher("MATCH (n) WHERE (n.a = 1 OR n.b = 2) AND n.c = 3 RETURN n").unwrap();
        assert!(matches!(q.where_clause, Some(CypherExpr::And(_, _))));
    }

    #[test]
    fn where_string_comparison() {
        let q = parse_cypher("MATCH (n) WHERE n.name = 'Alice' RETURN n").unwrap();
        assert!(q.where_clause.is_some());
    }

    #[test]
    fn where_null_comparison() {
        let q = parse_cypher("MATCH (n) WHERE n.val = null RETURN n").unwrap();
        assert!(q.where_clause.is_some());
    }

    #[test]
    fn where_negative_number() {
        let q = parse_cypher("MATCH (n) WHERE n.val = -42 RETURN n").unwrap();
        if let Some(CypherExpr::Cmp(_, _, ref right)) = q.where_clause {
            assert_eq!(**right, CypherExpr::Lit(CypherLiteral::Integer(-42)));
        } else {
            panic!("expected Cmp");
        }
    }

    // ── RETURN clause ──────────────────────────────────────────────────

    #[test]
    fn return_with_alias() {
        let q = parse_cypher("MATCH (n) RETURN n.name AS name").unwrap();
        if let ReturnClause::Items(items) = &q.return_clause {
            assert_eq!(items[0].alias.as_deref(), Some("name"));
        } else {
            panic!("expected Items");
        }
    }

    #[test]
    fn return_multiple_items() {
        let q = parse_cypher("MATCH (n) RETURN n.name, n.age").unwrap();
        if let ReturnClause::Items(items) = &q.return_clause {
            assert_eq!(items.len(), 2);
        } else {
            panic!("expected Items");
        }
    }

    // ── anonymous/empty nodes ──────────────────────────────────────────

    #[test]
    fn anonymous_node() {
        let q = parse_cypher("MATCH (:Person)-[:KNOWS]->(n) RETURN n").unwrap();
        let pat = &q.match_clause.patterns[0];
        assert!(pat.nodes[0].variable.is_none());
        assert_eq!(pat.nodes[0].label.as_deref(), Some("Person"));
    }

    #[test]
    fn empty_node() {
        let q = parse_cypher("MATCH ()-[:KNOWS]->(n) RETURN n").unwrap();
        let pat = &q.match_clause.patterns[0];
        assert!(pat.nodes[0].variable.is_none());
        assert!(pat.nodes[0].label.is_none());
    }

    #[test]
    fn anonymous_relationship() {
        let q = parse_cypher("MATCH (a)-[:KNOWS]->(b) RETURN a").unwrap();
        let edge = &q.match_clause.patterns[0].edges[0];
        assert!(edge.variable.is_none());
        assert_eq!(edge.rel_type.as_deref(), Some("KNOWS"));
    }

    // ── case insensitive keywords ──────────────────────────────────────

    #[test]
    fn case_insensitive_keywords() {
        let q = parse_cypher("match (n) where n.x = 1 return n").unwrap();
        assert!(q.where_clause.is_some());
    }

    // ── error cases ────────────────────────────────────────────────────

    #[test]
    fn error_empty() {
        assert!(parse_cypher("").is_err());
    }

    #[test]
    fn error_whitespace_only() {
        assert!(parse_cypher("   ").is_err());
    }

    #[test]
    fn error_missing_return() {
        assert!(parse_cypher("MATCH (n)").is_err());
    }

    #[test]
    fn error_missing_match() {
        assert!(parse_cypher("RETURN n").is_err());
    }

    #[test]
    fn error_unclosed_node() {
        assert!(parse_cypher("MATCH (n RETURN n").is_err());
    }

    #[test]
    fn error_unclosed_rel() {
        assert!(parse_cypher("MATCH (a)-[r:KNOWS->(b) RETURN a").is_err());
    }

    #[test]
    fn error_unclosed_string() {
        assert!(parse_cypher("MATCH (n {name: 'Alice}) RETURN n").is_err());
    }

    #[test]
    fn error_trailing_tokens() {
        assert!(parse_cypher("MATCH (n) RETURN n EXTRA").is_err());
    }

    #[test]
    fn error_double_label() {
        assert!(parse_cypher("MATCH (n:Person:Employee) RETURN n").is_err());
    }

    #[test]
    fn error_negative_path_length() {
        let err = parse_cypher("MATCH (a)-[*-1]->(b) RETURN a");
        assert!(err.is_err());
    }

    #[test]
    fn error_path_length_overflow() {
        let big = format!("MATCH (a)-[*{}]->(b) RETURN a", i64::MAX);
        let err = parse_cypher(&big);
        assert!(err.is_err(), "path length exceeding u32::MAX must fail");
    }

    #[test]
    fn error_path_length_max_overflow() {
        let big = format!("MATCH (a)-[*1..{}]->(b) RETURN a", i64::MAX);
        let err = parse_cypher(&big);
        assert!(err.is_err(), "path length max exceeding u32::MAX must fail");
    }

    #[test]
    fn prop_access_uses_boxed_var() {
        let q = parse_cypher("MATCH (n) WHERE n.age > 25 RETURN n").unwrap();
        if let Some(CypherExpr::Cmp(ref left, CmpOp::Gt, _)) = q.where_clause {
            match left.as_ref() {
                CypherExpr::Prop(obj, prop) => {
                    assert_eq!(**obj, CypherExpr::Var("n".to_string()));
                    assert_eq!(prop, "age");
                }
                other => panic!("expected Prop, got {other:?}"),
            }
        } else {
            panic!("expected Cmp with Gt");
        }
    }

    #[test]
    fn display_on_token_in_error_message() {
        let err = parse_cypher("MATCH (n) RETURN n EXTRA").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("`EXTRA`"),
            "error should use Display format, got: {msg}"
        );
    }
}
