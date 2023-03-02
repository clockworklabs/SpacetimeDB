use crate::operator::Op;
use crate::types::Ty;
use spacetimedb_sats::relation::{FieldName, RelationError};
use std::fmt;
use thiserror::Error;

/// Typing Errors
#[derive(Error, Debug)]
pub enum ErrorType {
    #[error("Expect {0}, but got {1}")]
    Expect(Ty, Ty),
    #[error("Function {0} not found")]
    NotFoundFun(String),
    #[error("Binary op {0:?} expect {1} arguments, but got {2}")]
    OpMiss(Op, usize, usize),
}

/// Vm Errors
#[derive(Error, Debug)]
pub enum ErrorVm {
    #[error("TypeError {0}")]
    Type(#[from] ErrorType),
    #[error("The  field `{0}` is not found in the header")]
    FieldNotFound(FieldName),
    #[error("RelationError {0}")]
    Rel(#[from] RelationError),
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
pub struct ErrorUser {
    kind: ErrorKind,
    msg: Option<String>,
    /// Optional context for the Error: Which record was not found, what value was invalid, etc.
    context: Option<Vec<ErrorCtx>>,
}

impl ErrorUser {
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

impl fmt::Display for ErrorUser {
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

impl From<ErrorType> for ErrorUser {
    fn from(x: ErrorType) -> Self {
        ErrorUser::new(ErrorKind::TypeMismatch, Some(&x.to_string()))
    }
}

impl From<ErrorVm> for ErrorUser {
    fn from(err: ErrorVm) -> Self {
        match err {
            ErrorVm::Type(err) => err.into(),
            ErrorVm::FieldNotFound(_) => ErrorUser::new(ErrorKind::Db, Some(&err.to_string())),
            ErrorVm::Other(err) => ErrorUser::new(ErrorKind::Db, Some(&err.to_string())),
            ErrorVm::Rel(err) => ErrorUser::new(ErrorKind::Db, Some(&err.to_string())),
        }
    }
}

impl From<FieldName> for ErrorVm {
    fn from(err: FieldName) -> Self {
        ErrorVm::FieldNotFound(err)
    }
}
