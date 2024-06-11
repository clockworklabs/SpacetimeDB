use crate::db::def::IndexType;
use crate::product_value::InvalidFieldError;
use crate::relation::{FieldName, Header};
use crate::satn::Satn as _;
use crate::{buffer, AlgebraicType, AlgebraicValue};
use derive_more::Display;
use spacetimedb_primitives::{ColId, ColList, TableId};
use std::fmt;
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

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum SchemaError {
    #[error("Multiple primary columns defined for table: {table} columns: {pks:?}")]
    MultiplePrimaryKeys { table: Box<str>, pks: Vec<String> },
    #[error("table id `{table_id}` should have name")]
    EmptyTableName { table_id: TableId },
    #[error("{ty} {name} columns `{columns:?}` not found  in table `{table}`")]
    ColumnsNotFound {
        name: Box<str>,
        table: Box<str>,
        columns: Vec<ColId>,
        ty: DefType,
    },
    #[error("table `{table}` {ty} should have name. {ty} id: {id}")]
    EmptyName { table: Box<str>, ty: DefType, id: u32 },
    #[error("table `{table}` have `Constraints::unset()` for columns: {columns:?}")]
    ConstraintUnset {
        table: Box<str>,
        name: Box<str>,
        columns: ColList,
    },
    #[error("Attempt to define a column with more than 1 auto_inc sequence: Table: `{table}`, Field: `{field}`")]
    OneAutoInc { table: Box<str>, field: Box<str> },
    #[error("Only Btree Indexes are supported: Table: `{table}`, Index: `{index}` is a `{index_type}`")]
    OnlyBtree {
        table: Box<str>,
        index: Box<str>,
        index_type: IndexType,
    },
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum IdentifierError {
    #[error("Identifier `{name}` is reserved by spacetimedb and cannot be used for table, column, or reducer names.")]
    Reserved { name: Box<str> },

    #[error("Identifier `{name}`'s starting character '{invalid_start}' does not start with an underscore or Unicode XID start character (according to Unicode Standard Annex 31).")]
    InvalidStart { name: Box<str>, invalid_start: char },

    #[error("Identifier `{name}` contains a character '{invalid_continue}' that is not a Unicode XID continue character (according to Unicode Standard Annex 31).")]
    InvalidContinue { name: Box<str>, invalid_continue: char },

    // This is not a particularly useful error without a link to WHICH identifier is empty.
    #[error("Identifier is empty.")]
    Empty {},
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub struct SchemaErrors(pub Vec<SchemaError>);

impl fmt::Display for SchemaErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.0.iter()).finish()
    }
}
