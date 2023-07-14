use crate::relation::{FieldName, Header};
use crate::{buffer, AlgebraicType};
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::AlgebraicValue;
use std::fmt;
use std::string::FromUtf8Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TypeError {
    #[error("Arrays must be homogeneous. It expects to be `{{expect.to_satns()}}` but `{{value.to_satns()}}` is of type `{{found.to_satns()}}`")]
    Array {
        expect: AlgebraicType,
        found: AlgebraicType,
        value: AlgebraicValue,
    },
    #[error("Arrays must define a type for the elements")]
    ArrayEmpty,
    #[error("Maps must be homogeneous. It expects to be `{{key_expect.to_satns()}}:{{value_expect.to_satns()}}` but `{{key.to_satns()}}::{{value.to_satns()}}` is of type `{{key_found.to_satns()}}:{{value_found.to_satns()}}`")]
    Map {
        key_expect: AlgebraicType,
        value_expect: AlgebraicType,
        key_found: AlgebraicType,
        value_found: AlgebraicType,
        key: AlgebraicValue,
        value: AlgebraicValue,
    },
    #[error("Maps must define a type for both key & value")]
    MapEmpty,
}

#[derive(Error, Debug, Clone)]
pub enum DecodeError {
    #[error("Decode UTF8: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("AlgebraicType::decode: Unknown: {0}")]
    AlgebraicTypeUnknown(u8),
    #[error("AlgebraicType::decode: Byte array has invalid length: {0:?}")]
    AlgebraicType(usize),
    #[error("SumType::decode: Byte array has invalid length: {0:?}")]
    SumType(usize),
    #[error("ProductType::decode: Byte array has invalid length: {0:?}")]
    ProductType(usize),
    #[error("ProductTypeElement::decode: Byte array has invalid length: {0:?}")]
    ProductTypeElement(usize),
    #[error("AlgebraicValue::decode: byte array length not long enough to decode {0:?}")]
    AlgebraicValue(AlgebraicType),
    #[error("AlgebraicValue::decode: byte array length not long enough to get length of {0:?}")]
    AlgebraicValueGetLength(AlgebraicType),
    #[error(
    "AlgebraicValue::decode: buffer has no room to decode any more elements from this {kind:?}. (len: {len} <= read:{read})"
    )]
    AlgebraicValueRoom {
        kind: AlgebraicType,
        len: usize,
        read: usize,
    },
    #[error("AlgebraicValue::decode: Cannot decode {kind:?}, buffer not long enough. (len: {len}, read:{read})")]
    TypeBufferSmall {
        kind: AlgebraicType,
        len: usize,
        read: usize,
    },
    #[error(
        "AlgebraicValue::decode: byte array length not long enough to decode {kind:?}. (expect: {expect}, read:{read})"
    )]
    TypeTooSmall {
        kind: AlgebraicType,
        expect: usize,
        read: usize,
    },
    #[error("EnumValue::decode: Byte array length is invalid.")]
    EnumValue,
}

#[derive(Error, Debug, Clone)]
pub enum LibError {
    #[error("DecodeError: {0}")]
    Decode(#[from] DecodeError),
    #[error("BufferError: {0}")]
    Buffer(#[from] buffer::DecodeError),
    #[error(transparent)]
    TupleFieldInvalid(#[from] InvalidFieldError),
}

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
pub enum AuthError {
    #[error("Table `{named}` is private")]
    TablePrivate { named: String },
    #[error("Index `{named}` is private")]
    IndexPrivate { named: String },
    #[error("Sequence `{named}` is private")]
    SequencePrivate { named: String },
}

#[derive(thiserror::Error, Debug)]
pub enum RelationError {
    #[error("Field `{1}` not found. Must be one of {0}")]
    FieldNotFound(Header, FieldName),
    #[error("Field `{0}` fail to infer the type: {1}")]
    TypeInference(FieldName, TypeError),
    #[error("Field declaration only support `table.field` or `field`. It gets instead `{0}`")]
    FieldPathInvalid(String),
}
