use spacetimedb_lib::operator::OpLogic;
use spacetimedb_sats::db::error::{AuthError, RelationError};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
use std::fmt;
use thiserror::Error;

use crate::expr::SourceId;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Config parameter `{0}` not found.")]
    NotFound(String),
    #[error("Value for config parameter `{0}` is invalid: `{1:?}`. Expected: `{2:?}`")]
    TypeError(String, AlgebraicValue, AlgebraicType),
}

/// Typing Errors
#[derive(Error, Debug)]
pub enum ErrorType {
    #[error("Error Parsing `{value}` into type [{ty}]: {err}")]
    Parse { value: String, ty: String, err: String },
    #[error("Type Mismatch Join: `{lhs}` != `{rhs}`")]
    TypeMismatchJoin { lhs: String, rhs: String },
    #[error("Type Mismatch: `{lhs}` != `{rhs}`")]
    TypeMismatch { lhs: String, rhs: String },
    #[error("Type Mismatch: `{lhs}` {op} `{rhs}`, both sides must be an `{expected}` expression")]
    TypeMismatchLogic {
        op: OpLogic,
        lhs: String,
        rhs: String,
        expected: String,
    },
}

/// Vm Errors
#[derive(Error, Debug)]
pub enum ErrorVm {
    #[error("TypeError {0}")]
    Type(#[from] ErrorType),
    #[error("ErrorLang {0}")]
    Lang(#[from] ErrorLang),
    #[error("RelationError {0}")]
    Rel(#[from] RelationError),
    #[error("AuthError {0}")]
    Auth(#[from] AuthError),
    #[error("Unsupported: {0}")]
    Unsupported(String),
    #[error("No source table with index {0:?}")]
    NoSuchSource(SourceId),
    #[error("ConfigError: {0}")]
    Config(#[from] ConfigError),
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorKind {
    Custom(String),
    Compiler,
    TypeMismatch,
    Db,
    Query,
    Duplicated,
    Invalid,
    NotFound,
    Params,
    OutOfBounds,
    Timeout,
    Unauthorized,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ErrorCtx {
    key: String,
    value: String,
}

impl ErrorCtx {
    pub fn new(key: &str, value: &str) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// Define the main User Error type for the VM
#[derive(Error, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ErrorLang {
    pub kind: ErrorKind,
    pub msg: Option<String>,
    /// Optional context for the Error: Which record was not found, what value was invalid, etc.
    pub context: Option<Vec<ErrorCtx>>,
}

impl ErrorLang {
    pub fn new(kind: ErrorKind, msg: Option<&str>) -> Self {
        Self {
            kind,
            msg: msg.map(|x| x.to_string()),
            context: None,
        }
    }

    pub fn with_ctx(self, of: ErrorCtx) -> Self {
        let mut x = self;
        if let Some(ref mut s) = x.context {
            s.push(of)
        } else {
            x.context = Some(vec![of])
        }
        x
    }
}

impl fmt::Display for ErrorLang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}Error", self.kind)?;
        if let Some(msg) = &self.msg {
            writeln!(f, ": \"{}\"", msg)?;
        }
        if let Some(err) = self.context.as_deref() {
            writeln!(f, " Context:")?;
            for e in err {
                writeln!(f, " {}: {}", e.key, e.value)?;
            }
        }
        Ok(())
    }
}

impl From<ErrorType> for ErrorLang {
    fn from(x: ErrorType) -> Self {
        ErrorLang::new(ErrorKind::TypeMismatch, Some(&x.to_string()))
    }
}

impl From<ErrorVm> for ErrorLang {
    fn from(err: ErrorVm) -> Self {
        match err {
            ErrorVm::Type(err) => err.into(),
            ErrorVm::Other(err) => ErrorLang::new(ErrorKind::Db, Some(&err.to_string())),
            ErrorVm::Rel(err) => ErrorLang::new(ErrorKind::Db, Some(&err.to_string())),
            ErrorVm::Unsupported(err) => ErrorLang::new(ErrorKind::Compiler, Some(&err)),
            ErrorVm::Lang(err) => err,
            ErrorVm::Auth(err) => ErrorLang::new(ErrorKind::Unauthorized, Some(&err.to_string())),
            ErrorVm::Config(err) => ErrorLang::new(ErrorKind::Db, Some(&err.to_string())),
            err @ ErrorVm::NoSuchSource(_) => ErrorLang {
                kind: ErrorKind::Invalid,
                msg: Some(format!("{err:?}")),
                context: None,
            },
        }
    }
}

impl From<RelationError> for ErrorLang {
    fn from(err: RelationError) -> Self {
        ErrorVm::Rel(err).into()
    }
}
