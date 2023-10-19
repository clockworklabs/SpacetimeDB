use crate::client::ClientActorId;
use crate::db::datastore::traits::IndexDef;
use hex::FromHexError;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::error::{LibError, RelationError};
use spacetimedb_lib::relation::FieldName;
use spacetimedb_lib::{PrimaryKey, ProductValue};
use spacetimedb_primitives::{ColId, IndexId, TableId};
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_sats::satn::Satn;
use spacetimedb_sats::AlgebraicValue;
use spacetimedb_vm::errors::{ErrorKind, ErrorLang, ErrorVm};
use spacetimedb_vm::expr::Crud;
use std::num::ParseIntError;
use std::path::PathBuf;
use std::sync::{MutexGuard, PoisonError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TableError {
    #[error("Table with name `{0}` start with 'st_' and that is reserved for internal system tables.")]
    System(String),
    #[error("Table with name `{0}` already exists.")]
    Exist(String),
    #[error("Table with name `{0}` not found.")]
    NotFound(String),
    #[error("Table with ID `{0}` not found.")]
    IdNotFound(TableId),
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
        field: String,
        expect: String,
        found: String,
    },
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum IndexError {
    #[error("Index not found: {0:?}")]
    NotFound(IndexId),
    #[error("Index already exist: {0:?}: {1}")]
    IndexAlreadyExists(IndexDef, String),
    #[error("Column not found: {0:?}")]
    ColumnNotFound(IndexDef),
    #[error("Unique constraint violation '{}' in table '{}': column(s): '{:?}' value: {}", constraint_name, table_name, col_names, value.to_satn())]
    UniqueConstraintViolation {
        constraint_name: String,
        table_name: String,
        col_names: Vec<String>,
        value: AlgebraicValue,
    },
    #[error("Attempt to define a index with more than 1 auto_inc column: Table: {0:?}, Columns: {1:?}")]
    OneAutoInc(TableId, Vec<String>),
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
}

#[derive(Error, Debug)]
pub enum PlanError {
    #[error("Unsupported feature: `{feature}`")]
    Unsupported { feature: String },
    #[error("Unknown table: `{table}`")]
    UnknownTable { table: String },
    #[error("Qualified Table `{expect}` not found")]
    TableNotFoundQualified { expect: String },
    #[error("Unknown field: `{field}` not found in the table(s): `{tables:?}`")]
    UnknownField { field: FieldName, tables: Vec<String> },
    #[error("Field(s): `{fields:?}` not found in the table(s): `{tables:?}`")]
    UnknownFields {
        fields: Vec<FieldName>,
        tables: Vec<String>,
    },
    #[error("Ambiguous field: `{field}`. Also found in {found:?}")]
    AmbiguousField { field: String, found: Vec<FieldName> },
    #[error("Plan error: `{0}`")]
    Unstructured(String),
    #[error("Internal DBError: `{0}`")]
    DatabaseInternal(Box<DBError>),
    #[error("Relation Error: `{0}`")]
    Relation(#[from] RelationError),
    #[error("{0}")]
    VmError(#[from] ErrorVm),
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("Database instance not found: {0}")]
    NotFound(u64),
    #[error("Database is already opened. Path:`{0}`. Error:{1}")]
    DatabasedOpened(PathBuf, anyhow::Error),
}

#[derive(Error, Debug)]
pub enum DBError {
    #[error("LibError: {0}")]
    Lib(#[from] LibError),
    #[error("BufferError: {0}")]
    Buffer(#[from] DecodeError),
    #[error("TableError: {0}")]
    Table(#[from] TableError),
    #[error("SequenceError: {0}")]
    Sequence2(#[from] crate::db::datastore::locking_tx_datastore::SequenceError),
    #[error("IndexError: {0}")]
    Index(#[from] IndexError),
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
    #[cfg(feature = "odb_rocksdb")]
    #[error("RocksDbError: {0}.")]
    RocksDbError(#[from] rocksdb::Error),
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
    #[error(transparent)]
    Other(#[from] anyhow::Error),
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
    #[error("Primary key {0:?} not found")]
    PrimaryKeyNotFound(PrimaryKey),
    #[error("row with column of given value not found")]
    ColumnValueNotFound,
    #[error("range of rows not found")]
    RangeNotFound,
    #[error("column is out of bounds")]
    BadColumn,
    #[error("can't perform operation; not inside transaction")]
    NotInTransaction,
    #[error("table with name {0:?} already exists")]
    AlreadyExists(String),
    #[error("table with name `{0}` start with 'st_' and that is reserved for internal system tables.")]
    SystemName(String),
    #[error("internal db error: {0}")]
    Internal(#[source] Box<DBError>),
    #[error("invalid index type: {0}")]
    BadIndexType(u8),
}

impl From<DBError> for NodesError {
    fn from(e: DBError) -> Self {
        match e {
            DBError::Table(TableError::Exist(name)) => Self::AlreadyExists(name),
            DBError::Table(TableError::System(name)) => Self::SystemName(name),
            DBError::Table(TableError::IdNotFound(_) | TableError::NotFound(_)) => Self::TableNotFound,
            DBError::Table(TableError::ColumnNotFound(_)) => Self::BadColumn,
            _ => Self::Internal(Box::new(e)),
        }
    }
}
