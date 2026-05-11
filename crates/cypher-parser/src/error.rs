use thiserror::Error;

use crate::lexer::Token;

#[derive(Error, Debug)]
pub enum CypherParseError {
    #[error("Unexpected token `{found}` at position {pos}, expected {expected}")]
    UnexpectedToken {
        found: Token,
        pos: usize,
        expected: &'static str,
    },
    #[error("Unexpected end of input, expected {expected}")]
    UnexpectedEof { expected: &'static str },
    #[error("Invalid number literal: {0}")]
    InvalidNumber(String),
    #[error("Unterminated string literal at position {0}")]
    UnterminatedString(usize),
    #[error("Path length value {value} is out of the valid u32 range at position {pos}")]
    PathLengthOutOfRange { value: i64, pos: usize },
    #[error("Empty query")]
    EmptyQuery,
}

pub type CypherParseResult<T> = Result<T, CypherParseError>;
