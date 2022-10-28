use crate::{buffer, TypeDef};
use std::fmt;
use std::string::FromUtf8Error;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum DecodeError {
    #[error("Decode UTF8: {0}")]
    Utf8(#[from] FromUtf8Error),
    #[error("TypeDef::decode: Unknown: {0}")]
    TypeDefUnknown(u8),
    #[error("TypeDef::decode: Byte array has invalid length: {0:?}")]
    TypeDef(usize),
    #[error("EnumDef::decode: Byte array has invalid length: {0:?}")]
    EnumDef(usize),
    #[error("TupleDef::decode: Byte array has invalid length: {0:?}")]
    TupleDef(usize),
    #[error("ElementDef::decode: Byte array has invalid length: {0:?}")]
    ElementDef(usize),
    #[error("TypeValue::decode: byte array length not long enough to decode {0:?}")]
    TypeValue(TypeDef),
    #[error("TypeValue::decode: byte array length not long enough to get length of {0:?}")]
    TypeValueGetLength(TypeDef),
    #[error(
    "TypeValue::decode: buffer has no room to decode any more elements from this {kind:?}. (len: {len} <= read:{read})"
    )]
    TypeValueRoom { kind: TypeDef, len: usize, read: usize },
    #[error("TypeValue::decode: Cannot decode {kind:?}, buffer not long enough. (len: {len}, read:{read})")]
    TypeBufferSmall { kind: TypeDef, len: usize, read: usize },
    #[error(
        "TypeValue::decode: byte array length not long enough to decode {kind:?}. (expect: {expect}, read:{read})"
    )]
    TypeTooSmall { kind: TypeDef, expect: usize, read: usize },
    #[error("EnumValue::decode: Byte array length is invalid.")]
    EnumValue,
}

#[derive(Error, Debug, Clone)]
pub enum LibError {
    #[error("DecodeError: {0}")]
    Decode(#[from] DecodeError),
    #[error("BufferError: {0}")]
    Buffer(#[from] buffer::DecodeError),
    #[error("Field {0}({1:?}) not found")]
    TupleFieldNotFound(usize, Option<&'static str>),
    #[error("Field {0}({1:?}) has a invalid type")]
    TupleFieldTypeInvalid(usize, Option<&'static str>),
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
