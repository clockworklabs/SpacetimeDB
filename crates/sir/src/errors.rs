use std::fmt;
use thiserror::Error;

use spacetimedb_lib::error::{AuthError, RelationError};

/// Vm Errors
#[derive(Error, Debug)]
pub enum ErrorVm {
    #[error("ErrorLang {0}")]
    Lang(#[from] ErrorLang),
    #[error("AuthError {0}")]
    Auth(#[from] AuthError),
    #[error("RelationError {0}")]
    Rel(#[from] RelationError),
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

impl From<ErrorVm> for ErrorLang {
    fn from(err: ErrorVm) -> Self {
        match err {
            ErrorVm::Auth(err) => ErrorLang::new(ErrorKind::Unauthorized, Some(&err.to_string())),
            ErrorVm::Lang(err) => err,
            ErrorVm::Rel(err) => ErrorLang::new(ErrorKind::Db, Some(&err.to_string())),
        }
    }
}

impl From<RelationError> for ErrorLang {
    fn from(err: RelationError) -> Self {
        ErrorVm::Rel(err).into()
    }
}
