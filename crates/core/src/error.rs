use std::io;
use std::num::ParseIntError;
use std::path::PathBuf;
use std::sync::{MutexGuard, PoisonError};

use enum_as_inner::EnumAsInner;
use hex::FromHexError;
use spacetimedb_expr::errors::TypingError;
use spacetimedb_sats::AlgebraicType;
use spacetimedb_schema::error::ValidationErrors;
use spacetimedb_snapshot::SnapshotError;
use spacetimedb_table::table::{self, ReadViaBsatnError, UniqueConstraintViolation};
use spacetimedb_table::{bflatn_to, read_column};
use thiserror::Error;

use crate::client::ClientActorId;
use crate::db::datastore::system_tables::SystemTable;
use crate::host::scheduler::ScheduleError;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::db::error::{LibError, RelationError, SchemaErrors};
use spacetimedb_lib::db::raw_def::v9::RawSql;
use spacetimedb_lib::db::raw_def::RawIndexDefV8;
use spacetimedb_lib::relation::FieldName;
use spacetimedb_primitives::*;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_vm::errors::{ErrorKind, ErrorLang, ErrorType, ErrorVm};
use spacetimedb_vm::expr::Crud;

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
pub enum ClientError {
    #[error("Client not found: {0}")]
    NotFound(ClientActorId),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SubscriptionError {
    #[error("Index not found: {0:?}")]
    NotFound(IndexId),
    #[error("Empty string")]
    Empty,
    #[error("Queries with side effects not allowed: {0:?}")]
    SideEffect(Crud),
    #[error("Unsupported query on subscription: {0:?}")]
    Unsupported(String),
    #[error("Subscribing to queries in one call is not supported")]
    Multiple,
}

#[derive(Error, Debug)]
pub enum PlanError {
    #[error("Unsupported feature: `{feature}`")]
    Unsupported { feature: String },
    #[error("Unknown table: `{table}`")]
    UnknownTable { table: Box<str> },
    #[error("Qualified Table `{expect}` not found")]
    TableNotFoundQualified { expect: String },
    #[error("Unknown field: `{field}` not found in the table(s): `{tables:?}`")]
    UnknownField { field: String, tables: Vec<Box<str>> },
    #[error("Unknown field name: `{field}` not found in the table(s): `{tables:?}`")]
    UnknownFieldName { field: FieldName, tables: Vec<Box<str>> },
    #[error("Field(s): `{fields:?}` not found in the table(s): `{tables:?}`")]
    UnknownFields { fields: Vec<String>, tables: Vec<Box<str>> },
    #[error("Ambiguous field: `{field}`. Also found in {found:?}")]
    AmbiguousField { field: String, found: Vec<String> },
    #[error("Plan error: `{0}`")]
    Unstructured(String),
    #[error("Internal DBError: `{0}`")]
    DatabaseInternal(Box<DBError>),
    #[error("Relation Error: `{0}`")]
    Relation(#[from] RelationError),
    #[error("{0}")]
    VmError(#[from] ErrorVm),
    #[error("{0}")]
    TypeCheck(#[from] ErrorType),
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Replica not found: {0}")]
    NotFound(u64),
    #[error("Database is already opened. Path:`{0}`. Error:{1}")]
    DatabasedOpened(PathBuf, anyhow::Error),
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

#[derive(Error, Debug, EnumAsInner)]
pub enum DBError {
    #[error("LibError: {0}")]
    Lib(#[from] LibError),
    #[error("BufferError: {0}")]
    Buffer(#[from] DecodeError),
    #[error("TableError: {0}")]
    Table(#[from] TableError),
    #[error("SequenceError: {0}")]
    Sequence2(#[from] SequenceError),
    #[error("IndexError: {0}")]
    Index(#[from] IndexError),
    #[error("SchemaError: {0}")]
    Schema(SchemaErrors),
    #[error("IOError: {0}.")]
    IoError(#[from] std::io::Error),
    #[error("ParseIntError: {0}.")]
    ParseInt(#[from] ParseIntError),
    #[error("Hex representation of hash decoded to incorrect number of bytes: {0}.")]
    DecodeHexHash(usize),
    #[error("DecodeHexError: {0}.")]
    DecodeHex(#[from] FromHexError),
    #[error("DatabaseError: {0}.")]
    Database(#[from] DatabaseError),
    #[error("SledError: {0}.")]
    SledDbError(#[from] sled::Error),
    #[error("Mutex was poisoned acquiring lock on MessageLog: {0}")]
    MessageLogPoisoned(String),
    #[error("VmError: {0}")]
    Vm(#[from] ErrorVm),
    #[error("VmErrorUser: {0}")]
    VmUser(#[from] ErrorLang),
    #[error("SubscriptionError: {0}")]
    Subscription(#[from] SubscriptionError),
    #[error("ClientError: {0}")]
    Client(#[from] ClientError),
    #[error("SqlParserError: {error}, executing: `{sql}`")]
    SqlParser {
        sql: String,
        error: sqlparser::parser::ParserError,
    },
    #[error("SqlError: {error}, executing: `{sql}`")]
    Plan { sql: String, error: PlanError },
    #[error("Error replaying the commit log: {0}")]
    LogReplay(#[from] LogReplayError),
    #[error(transparent)]
    // Box the inner [`SnapshotError`] to keep Clippy quiet about large `Err` variants.
    Snapshot(#[from] Box<SnapshotError>),
    #[error("Error reading a value from a table through BSATN: {0}")]
    ReadViaBsatnError(#[from] ReadViaBsatnError),
    #[error("Module validation errors: {0}")]
    ModuleValidationErrors(#[from] ValidationErrors),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
    #[error(transparent)]
    TypeError(#[from] TypingError),
}

impl From<bflatn_to::Error> for DBError {
    fn from(err: bflatn_to::Error) -> Self {
        Self::Table(err.into())
    }
}

impl From<table::InsertError> for DBError {
    fn from(err: table::InsertError) -> Self {
        match err {
            table::InsertError::Duplicate(e) => TableError::from(e).into(),
            table::InsertError::Bflatn(e) => TableError::from(e).into(),
            table::InsertError::IndexError(e) => IndexError::from(e).into(),
        }
    }
}

impl From<SnapshotError> for DBError {
    fn from(e: SnapshotError) -> Self {
        DBError::Snapshot(Box::new(e))
    }
}

impl DBError {
    pub fn get_auth_error(&self) -> Option<&ErrorLang> {
        if let Self::VmUser(err) = self {
            if err.kind == ErrorKind::Unauthorized {
                return Some(err);
            }
        }
        None
    }
}

impl From<DBError> for ErrorVm {
    fn from(err: DBError) -> Self {
        ErrorVm::Other(err.into())
    }
}

impl From<InvalidFieldError> for DBError {
    fn from(value: InvalidFieldError) -> Self {
        LibError::from(value).into()
    }
}

impl From<spacetimedb_table::read_column::TypeError> for DBError {
    fn from(err: spacetimedb_table::read_column::TypeError) -> Self {
        TableError::from(err).into()
    }
}

impl From<DBError> for PlanError {
    fn from(err: DBError) -> Self {
        PlanError::DatabaseInternal(Box::new(err))
    }
}

impl<'a, T: ?Sized + 'a> From<PoisonError<std::sync::MutexGuard<'a, T>>> for DBError {
    fn from(err: PoisonError<MutexGuard<'_, T>>) -> Self {
        DBError::MessageLogPoisoned(err.to_string())
    }
}

#[derive(Debug, Error)]
pub enum LogReplayError {
    #[error(
        "Out-of-order commit detected: {} in segment {} after offset {}",
        .commit_offset,
        .segment_offset,
        .last_commit_offset
    )]
    OutOfOrderCommit {
        commit_offset: u64,
        segment_offset: usize,
        last_commit_offset: u64,
    },
    #[error(
        "Error reading segment {}/{} at commit {}: {}",
        .segment_offset,
        .total_segments,
        .commit_offset,
        .source
    )]
    TrailingSegments {
        segment_offset: usize,
        total_segments: usize,
        commit_offset: u64,
        #[source]
        source: io::Error,
    },
    #[error("Could not reset log to offset {}: {}", .offset, .source)]
    Reset {
        offset: u64,
        #[source]
        source: io::Error,
    },
    #[error("Missing object {} referenced from commit {}", .hash, .commit_offset)]
    MissingObject { hash: Hash, commit_offset: u64 },
    #[error(
        "Unexpected I/O error reading commit {} from segment {}: {}",
        .commit_offset,
        .segment_offset,
        .source
    )]
    Io {
        segment_offset: usize,
        commit_offset: u64,
        #[source]
        source: io::Error,
    },
}

#[derive(Error, Debug)]
pub enum NodesError {
    #[error("Failed to decode row: {0}")]
    DecodeRow(#[source] DecodeError),
    #[error("Failed to decode value: {0}")]
    DecodeValue(#[source] DecodeError),
    #[error("Failed to decode primary key: {0}")]
    DecodePrimaryKey(#[source] DecodeError),
    #[error("Failed to decode schema: {0}")]
    DecodeSchema(#[source] DecodeError),
    #[error("Failed to decode filter: {0}")]
    DecodeFilter(#[source] DecodeError),
    #[error("table with provided name or id doesn't exist")]
    TableNotFound,
    #[error("index with provided name or id doesn't exist")]
    IndexNotFound,
    #[error("index was not unique")]
    IndexNotUnique,
    #[error("row was not found in index")]
    IndexRowNotFound,
    #[error("column is out of bounds")]
    BadColumn,
    #[error("can't perform operation; not inside transaction")]
    NotInTransaction,
    #[error("table with name {0:?} already exists")]
    AlreadyExists(String),
    #[error("table with name `{0}` start with 'st_' and that is reserved for internal system tables.")]
    SystemName(Box<str>),
    #[error("internal db error: {0}")]
    Internal(#[source] Box<DBError>),
    #[error(transparent)]
    BadQuery(#[from] RelationError),
    #[error("invalid index type: {0}")]
    BadIndexType(u8),
    #[error("Failed to scheduled timer: {0}")]
    ScheduleError(#[source] ScheduleError),
}

impl From<DBError> for NodesError {
    fn from(e: DBError) -> Self {
        match e {
            DBError::Table(TableError::Exist(name)) => Self::AlreadyExists(name),
            DBError::Table(TableError::System(name)) => Self::SystemName(name),
            DBError::Table(TableError::IdNotFound(_, _) | TableError::NotFound(_)) => Self::TableNotFound,
            DBError::Table(TableError::ColumnNotFound(_)) => Self::BadColumn,
            DBError::Index(IndexError::NotFound(_)) => Self::IndexNotFound,
            DBError::Index(IndexError::Decode(e)) => Self::DecodeRow(e),
            DBError::Index(IndexError::NotUnique(_)) => Self::IndexNotUnique,
            DBError::Index(IndexError::KeyNotFound(..)) => Self::IndexRowNotFound,
            _ => Self::Internal(Box::new(e)),
        }
    }
}

impl From<ErrorVm> for NodesError {
    fn from(err: ErrorVm) -> Self {
        DBError::from(err).into()
    }
}
