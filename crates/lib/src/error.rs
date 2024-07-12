use crate::relation::{FieldName, Header};
use spacetimedb_sats::{satn::Satn, AlgebraicType, AlgebraicValue};
use std::fmt;

/// A wrapper for using on test so the error display nicely
pub struct TestError {
    pub error: Box<dyn std::error::Error>,
}

impl fmt::Debug for TestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Format the error in yellow
        write!(f, "\x1b[93m{}\x1b[0m", self.error)
    }
}

impl<E: std::error::Error + 'static> From<E> for TestError {
    fn from(e: E) -> Self {
        Self { error: Box::new(e) }
    }
}

/// A wrapper for using [Result] in tests, so it display nicely
pub type ResultTest<T> = Result<T, TestError>;

#[derive(thiserror::Error, Debug)]
pub enum TypeError {
    #[error("The type of `{{value.to_satns()}}` cannot be inferred")]
    CannotInferType { value: AlgebraicValue },
}

#[derive(thiserror::Error, Debug)]
pub enum RelationError {
    #[error("Field `{1}` not found. Must be one of {0}")]
    FieldNotFound(Header, FieldName),
    #[error("Field `{0}` fail to infer the type: {1}")]
    TypeInference(FieldName, TypeError),
    #[error("Field with value `{}` was not a `bool`", val.to_satn())]
    NotBoolValue { val: AlgebraicValue },
    #[error("Field `{field}` was expected to be `bool` but is `{}`", ty.to_satn())]
    NotBoolType { field: FieldName, ty: AlgebraicType },
    #[error("Field declaration only support `table.field` or `field`. It gets instead `{0}`")]
    FieldPathInvalid(String),
}
