use super::system_tables::SystemTable;
use enum_as_inner::EnumAsInner;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::{
    db::{
        error::LibError,
        raw_def::{v9::RawSql, RawIndexDefV8},
    },
    AlgebraicType, AlgebraicValue, ProductValue,
};
use spacetimedb_primitives::{ColId, ColList, IndexId, SequenceId, TableId};
use spacetimedb_sats::{product_value::InvalidFieldError, satn::Satn};
use spacetimedb_snapshot::SnapshotError;
use spacetimedb_table::{
    bflatn_to, read_column,
    table::{self, ReadViaBsatnError, UniqueConstraintViolation},
};
use thiserror::Error;

#[derive(Error, Debug, EnumAsInner)]
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
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Error, Debug, EnumAsInner)]
pub enum TableError {
    #[error("Table with name `{0}` start with 'st_' and that is reserved for internal system tables.")]
    System(Box<str>),
    #[error("Table with name `{0}` already exists.")]
    Exist(String),
    #[error("Table with name `{0}` not found.")]
    NotFound(String),
    #[error("Table with ID `{1}` not found in `{0}`.")]
    IdNotFound(SystemTable, u32),
    #[error("Sql `{1}` not found in `{0}`.")]
    RawSqlNotFound(SystemTable, RawSql),
    #[error("Table with ID `{0}` not found in `TxState`.")]
    IdNotFoundState(TableId),
    #[error("Column `{0}.{1}` is missing a name")]
    ColumnWithoutName(String, ColId),
    #[error("schema_for_table: Table has invalid schema: {0} Err: {1}")]
    InvalidSchema(TableId, LibError),
    #[error("Row has invalid row type for table: {0} Err: {1}", table_id, row.to_satn())]
    RowInvalidType { table_id: TableId, row: ProductValue },
    #[error("failed to decode row in table")]
    RowDecodeError(DecodeError),
    #[error("Column with name `{0}` already exists")]
    DuplicateColumnName(String),
    #[error("Column `{0}` not found")]
    ColumnNotFound(ColId),
    #[error(
        "DecodeError for field `{0}.{1}`, expect `{2}` but found `{3}`",
        table,
        field,
        expect,
        found
    )]
    DecodeField {
        table: String,
        field: Box<str>,
        expect: String,
        found: String,
    },
    #[error(transparent)]
    Bflatn(#[from] bflatn_to::Error),
    #[error(transparent)]
    Duplicate(#[from] table::DuplicateError),
    #[error(transparent)]
    ReadColTypeError(#[from] read_column::TypeError),
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
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SequenceError {
    #[error("Sequence with name `{0}` already exists.")]
    Exist(String),
    #[error("Sequence `{0}`: The increment is 0, and this means the sequence can't advance.")]
    IncrementIsZero(String),
    #[error("Sequence `{0}`: The min_value {1} must < max_value {2}.")]
    MinMax(String, i128, i128),
    #[error("Sequence `{0}`: The start value {1} must be >= min_value {2}.")]
    MinStart(String, i128, i128),
    #[error("Sequence `{0}`: The start value {1} must be <= min_value {2}.")]
    MaxStart(String, i128, i128),
    #[error("Sequence `{0}` failed to decode value from Sled (not a u128).")]
    SequenceValue(String),
    #[error("Sequence ID `{0}` not found.")]
    NotFound(SequenceId),
    #[error("Sequence applied to a non-integer field. Column `{col}` is of type {{found.to_sats()}}.")]
    NotInteger { col: String, found: AlgebraicType },
    #[error("Sequence ID `{0}` still had no values left after allocation.")]
    UnableToAllocate(SequenceId),
    #[error("Autoinc constraint on table {0:?} spans more than one column: {1:?}")]
    MultiColumnAutoInc(TableId, ColList),
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
