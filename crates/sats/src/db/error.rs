use crate::product_value::InvalidFieldError;
use crate::relation::{FieldName, Header};
use crate::satn::Satn as _;
use crate::{buffer, AlgebraicType, AlgebraicValue};
use derive_more::Display;
use std::string::FromUtf8Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TypeError {
    #[error("The type of `{{value.to_satns()}}` cannot be inferred")]
    CannotInferType { value: AlgebraicValue },
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

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Table `{named}` is private")]
    TablePrivate { named: String },
    #[error("Index `{named}` is private")]
    IndexPrivate { named: String },
    #[error("Sequence `{named}` is private")]
    SequencePrivate { named: String },
    #[error("Only the database owner can perform the requested operation")]
    OwnerRequired,
    #[error("Constraint `{named}` is private")]
    ConstraintPrivate { named: String },
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Display)]
pub enum DefType {
    Column,
    Index,
    Sequence,
    Constraint,
}
