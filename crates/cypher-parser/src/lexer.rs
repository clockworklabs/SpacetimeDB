use crate::error::{CypherParseError, CypherParseResult};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Punctuation
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Colon,
    Comma,
    Dot,
    DotDot,
    Star,
    Dash,
    Arrow,      // ->
    LArrow,     // <-

    // Comparison
    Eq,
    Ne,         // <>
    Lt,
    Gt,
    Lte,        // <=
    Gte,        // >=

    // Keywords
    Match,
    Where,
    Return,
    As,
    And,
    Or,
    Not,
    True,
    False,
    Null,

    // Literals
    Integer(i64),
    Float(f64),
    StringLit(String),

    // Identifier
    Ident(String),

    Eof,
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::Colon => write!(f, ":"),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
            Token::DotDot => write!(f, ".."),
            Token::Star => write!(f, "*"),
            Token::Dash => write!(f, "-"),
            Token::Arrow => write!(f, "->"),
            Token::LArrow => write!(f, "<-"),
            Token::Eq => write!(f, "="),
            Token::Ne => write!(f, "<>"),
            Token::Lt => write!(f, "<"),
            Token::Gt => write!(f, ">"),
            Token::Lte => write!(f, "<="),
            Token::Gte => write!(f, ">="),
            Token::Match => write!(f, "MATCH"),
            Token::Where => write!(f, "WHERE"),
            Token::Return => write!(f, "RETURN"),
            Token::As => write!(f, "AS"),
            Token::And => write!(f, "AND"),
            Token::Or => write!(f, "OR"),
            Token::Not => write!(f, "NOT"),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::Null => write!(f, "null"),
            Token::Integer(n) => write!(f, "{n}"),
            Token::Float(n) => write!(f, "{n}"),
            Token::StringLit(s) => write!(f, "'{s}'"),
            Token::Ident(s) => write!(f, "{s}"),
            Token::Eof => write!(f, "end of input"),
        }
    }
}

/// Byte-oriented lexer for the supported openCypher subset.
///
/// # ASCII assumption
///
/// The lexer operates on raw bytes (`input.as_bytes()`) and treats each byte
/// as a single character. This works correctly for ASCII-only identifiers,
/// keywords, and operators — which is sufficient for the current Cypher
/// subset — but will mis-lex multi-byte UTF-8 sequences in identifiers or
/// string literals that contain non-ASCII characters.
pub struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            pos: 0,
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    fn peek_byte(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.input.get(self.pos).copied()?;
        self.pos += 1;
        Some(b)
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn read_string(&mut self, quote: u8) -> CypherParseResult<String> {
        let start = self.pos;
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(CypherParseError::UnterminatedString(start - 1)),
                Some(b) if b == quote => {
                    if self.peek_byte() == Some(quote) {
                        self.advance();
                        s.push(quote as char);
                    } else {
                        return Ok(s);
                    }
                }
                Some(b'\\') => match self.advance() {
                    Some(b'n') => s.push('\n'),
                    Some(b't') => s.push('\t'),
                    Some(b'\\') => s.push('\\'),
                    Some(b) if b == quote => s.push(quote as char),
                    Some(b) => {
                        s.push('\\');
                        s.push(b as char);
                    }
                    None => return Err(CypherParseError::UnterminatedString(start - 1)),
                },
                Some(b) => s.push(b as char),
            }
        }
    }

    fn read_number(&mut self, first: u8) -> CypherParseResult<Token> {
        let mut buf = String::new();
        buf.push(first as char);
        let mut is_float = false;

        while let Some(b) = self.peek_byte() {
            if b.is_ascii_digit() {
                buf.push(b as char);
                self.advance();
            } else if b == b'.' && !is_float {
                if self.input.get(self.pos + 1).copied() == Some(b'.') {
                    break;
                }
                is_float = true;
                buf.push('.');
                self.advance();
            } else {
                break;
            }
        }

        if is_float {
            buf.parse::<f64>()
                .map(Token::Float)
                .map_err(|_| CypherParseError::InvalidNumber(buf))
        } else {
            buf.parse::<i64>()
                .map(Token::Integer)
                .map_err(|_| CypherParseError::InvalidNumber(buf))
        }
    }

    fn read_ident(&mut self, first: u8) -> Token {
        let mut s = String::new();
        s.push(first as char);
        while let Some(b) = self.peek_byte() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                s.push(b as char);
                self.advance();
            } else {
                break;
            }
        }
        match s.to_ascii_uppercase().as_str() {
            "MATCH" => Token::Match,
            "WHERE" => Token::Where,
            "RETURN" => Token::Return,
            "AS" => Token::As,
            "AND" => Token::And,
            "OR" => Token::Or,
            "NOT" => Token::Not,
            "TRUE" => Token::True,
            "FALSE" => Token::False,
            "NULL" => Token::Null,
            _ => Token::Ident(s),
        }
    }

    pub fn next_token(&mut self) -> CypherParseResult<Token> {
        self.skip_whitespace();
        let Some(b) = self.advance() else {
            return Ok(Token::Eof);
        };
        match b {
            b'(' => Ok(Token::LParen),
            b')' => Ok(Token::RParen),
            b'[' => Ok(Token::LBracket),
            b']' => Ok(Token::RBracket),
            b'{' => Ok(Token::LBrace),
            b'}' => Ok(Token::RBrace),
            b':' => Ok(Token::Colon),
            b',' => Ok(Token::Comma),
            b'*' => Ok(Token::Star),
            b'.' => {
                if self.peek_byte() == Some(b'.') {
                    self.advance();
                    Ok(Token::DotDot)
                } else {
                    Ok(Token::Dot)
                }
            }
            b'-' => {
                if self.peek_byte() == Some(b'>') {
                    self.advance();
                    Ok(Token::Arrow)
                } else {
                    Ok(Token::Dash)
                }
            }
            b'<' => {
                if self.peek_byte() == Some(b'-') {
                    self.advance();
                    Ok(Token::LArrow)
                } else if self.peek_byte() == Some(b'>') {
                    self.advance();
                    Ok(Token::Ne)
                } else if self.peek_byte() == Some(b'=') {
                    self.advance();
                    Ok(Token::Lte)
                } else {
                    Ok(Token::Lt)
                }
            }
            b'>' => {
                if self.peek_byte() == Some(b'=') {
                    self.advance();
                    Ok(Token::Gte)
                } else {
                    Ok(Token::Gt)
                }
            }
            b'=' => Ok(Token::Eq),
            b'\'' | b'"' => self.read_string(b).map(Token::StringLit),
            b if b.is_ascii_digit() => self.read_number(b),
            b if b.is_ascii_alphabetic() || b == b'_' => Ok(self.read_ident(b)),
            _ => Err(CypherParseError::UnexpectedToken {
                found: Token::Ident(String::from(b as char)),
                pos: self.pos - 1,
                expected: "valid character",
            }),
        }
    }

    pub fn tokenize(input: &str) -> CypherParseResult<Vec<(Token, usize)>> {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();
        loop {
            let pos = lexer.pos();
            let tok = lexer.next_token()?;
            if tok == Token::Eof {
                tokens.push((Token::Eof, pos));
                break;
            }
            tokens.push((tok, pos));
        }
        Ok(tokens)
    }
}
