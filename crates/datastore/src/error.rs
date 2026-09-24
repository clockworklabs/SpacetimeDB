use super::system_tables::SystemTable;
use spacetimedb_lib::db::raw_def::{v9::RawSql, RawIndexDefV8};
use spacetimedb_primitives::{ColId, IndexId, SequenceId, TableId, ViewId};
use spacetimedb_sats::buffer::DecodeError;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::raw_identifier::RawNamespacedIdentifier;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue};
use spacetimedb_schema::def::error::LibError;
use spacetimedb_snapshot::SnapshotError;
use spacetimedb_table::{
    bflatn_to, read_column,
    table::{self, ReadViaBsatnError, UniqueConstraintViolation},
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DatastoreError {
    #[error("LibError: {0}")]
    Lib(#[from] LibError),
    #[error("TableError: {0}")]
    Table(#[from] TableError),
    #[error("IndexError: {0}")]
    Index(#[from] IndexError),
    #[error("SequenceError: {0}")]
    Sequence(#[from] SequenceError),
    #[error(transparent)]
    // Box the inner [`SnapshotError`] to keep Clippy quiet about large `Err` variants.
    Snapshot(#[from] Box<SnapshotError>),
    // TODO(cloutiertyler): should this be a TableError? I couldn't get it to compile
    #[error("Error reading a value from a table through BSATN: {0}")]
    ReadViaBsatnError(#[from] ReadViaBsatnError),

    #[error("ViewError: {0}")]
    View(#[from] ViewError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Error, Debug)]
pub enum ViewError {
    #[error("view '{0}' not found")]
    NotFound(RawNamespacedIdentifier),
    #[error("Table backing View '{0}' not found")]
    TableNotFound(ViewId),
    #[error("failed to deserialize view arguments from row")]
    DeserializeArgs,
    #[error("failed to deserialize view return value: {0}")]
    DeserializeReturn(String),
    #[error("failed to serialize row to BSATN")]
    SerializeRow,
    #[error("invalid return type: expected Array or Option, got {0:?}")]
    InvalidReturnType(AlgebraicType),
    #[error("return type is Array but deserialized value is not Array")]
    TypeMismatchArray,
    #[error("return type is Option but deserialized value is not Option")]
    TypeMismatchOption,
    #[error("expected ProductValue in view result")]
    ExpectedProduct,
}

#[derive(Error, Debug)]
pub enum TableError {
    #[error("Table with name `{0}` not found.")]
    NotFound(String),
    #[error("Table with ID `{1}` not found in `{0}`.")]
    IdNotFound(SystemTable, u32),
    #[error("Sql `{1}` not found in `{0}`.")]
    RawSqlNotFound(SystemTable, RawSql),
    #[error("Table with ID `{0}` not found in `TxState`.")]
    IdNotFoundState(TableId),
    #[error("Column `{0}` not found")]
    ColumnNotFound(ColId),
    #[error(transparent)]
    Bflatn(#[from] bflatn_to::Error),
    #[error(transparent)]
    Duplicate(#[from] table::DuplicateError),
    #[error(transparent)]
    ReadColTypeError(#[from] read_column::TypeError),
    #[error(transparent)]
    // Error here is `Box`ed to avoid triggering https://rust-lang.github.io/rust-clippy/master/index.html#result_large_err .
    ChangeColumnsError(#[from] Box<table::ChangeColumnsError>),
    #[error(transparent)]
    AddColumnsError(#[from] Box<table::AddColumnsError>),
    #[error("Event table with ID `{0}` is not empty")]
    EventTableNotEmpty(TableId),
    #[error(
        "Table with ID `{0}` attempted to reschema using `alter_event_table_row_type`, but it is not an event table"
    )]
    ReschemaNotAnEventTable(TableId),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum IndexError {
    #[error("Index not found: {0:?}")]
    NotFound(IndexId),
    #[error("Column not found: {0:?}")]
    ColumnNotFound(RawIndexDefV8),
    #[error(transparent)]
    UniqueConstraintViolation(#[from] UniqueConstraintViolation),
    #[error("Attempt to define a index with more than 1 auto_inc column: Table: {0:?}, Columns: {1:?}")]
    OneAutoInc(TableId, Vec<String>),
    #[error("Could not decode arguments to index scan")]
    Decode(DecodeError),
    #[error("Index was not unique: {0:?}")]
    NotUnique(IndexId),
    #[error("Key {1:?} was not found in index {0:?}")]
    KeyNotFound(IndexId, AlgebraicValue),
    #[error("IndexId {0:?} does not support seeking for a range")]
    IndexCannotSeekRange(IndexId),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SequenceError {
    #[error("Sequence with name `{0}` already exists.")]
    Exist(String),
    #[error("Sequence `{0}`: The increment is 0, and this means the sequence can't advance.")]
    IncrementIsZero(String),
    #[error("Sequence ID `{0}` not found.")]
    NotFound(SequenceId),
    #[error("Incrementing sequence with previous value {0} would result in integer overflow")]
    Overflow(i128),
}

impl From<InvalidFieldError> for DatastoreError {
    fn from(value: InvalidFieldError) -> Self {
        LibError::from(value).into()
    }
}

impl From<spacetimedb_table::read_column::TypeError> for DatastoreError {
    fn from(err: spacetimedb_table::read_column::TypeError) -> Self {
        TableError::from(err).into()
    }
}

impl From<table::InsertError> for DatastoreError {
    fn from(err: table::InsertError) -> Self {
        match err {
            table::InsertError::Duplicate(e) => TableError::from(e).into(),
            table::InsertError::Bflatn(e) => TableError::from(e).into(),
            table::InsertError::IndexError(e) => IndexError::from(e).into(),
        }
    }
}

impl From<bflatn_to::Error> for DatastoreError {
    fn from(err: bflatn_to::Error) -> Self {
        Self::Table(err.into())
    }
}

impl From<SnapshotError> for DatastoreError {
    fn from(e: SnapshotError) -> Self {
        DatastoreError::Snapshot(Box::new(e))
    }
}
