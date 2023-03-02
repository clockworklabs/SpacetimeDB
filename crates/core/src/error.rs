use crate::client::ClientActorId;
use crate::db::index::{IndexDef, IndexId};
use crate::db::sequence::SequenceError;
use hex::FromHexError;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::error::LibError;
use spacetimedb_lib::{PrimaryKey, TupleValue, TypeValue};
use spacetimedb_sats::product_value::InvalidFieldError;
use spacetimedb_vm::errors::{ErrorUser, ErrorVm};
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
    IdNotFound(u32),
    #[error("Column `{0}.{1}` is missing a name")]
    ColumnWithoutName(String, u32),
    #[error("schema_for_table: Table has invalid schema: {0} Err: {1}")]
    InvalidSchema(u32, LibError),
    #[error("failed to decode row in table")]
    RowDecodeError(DecodeError),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum IndexError {
    #[error("Index not found: {0}")]
    NotFound(IndexId),
    #[error("Index already exist: {0}: {1}")]
    IndexAlreadyExists(IndexDef, String),
    #[error("Column not found: {0}")]
    ColumnNotFound(IndexDef),
    #[error("Index is duplicated: {0:?}:{1:?}")]
    Duplicated(IndexDef, TypeValue, TupleValue),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ClientError {
    #[error("Client not found: {0}")]
    NotFound(ClientActorId),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SubscriptionError {
    #[error("Index not found: {0}")]
    NotFound(IndexId),
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
    Sequence(#[from] SequenceError),
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
    #[error("Database is already opened. Path:`{0}`. Error:{1}")]
    DatabasedOpened(PathBuf, anyhow::Error),
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
    VmUser(#[from] ErrorUser),
    #[error("SubscriptionError: {0}")]
    Subscription(#[from] SubscriptionError),
    #[error("ClientError: {0}")]
    Client(#[from] ClientError),
    #[error("ProstError: {0}")]
    Prost(#[from] prost::EncodeError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<InvalidFieldError> for DBError {
    fn from(value: InvalidFieldError) -> Self {
        LibError::from(value).into()
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
}

impl From<DBError> for NodesError {
    fn from(e: DBError) -> Self {
        match e {
            DBError::Table(TableError::Exist(name)) => Self::AlreadyExists(name),
            DBError::Table(TableError::System(name)) => Self::SystemName(name),
            DBError::Table(TableError::IdNotFound(_) | TableError::NotFound(_)) => Self::TableNotFound,
            _ => Self::Internal(Box::new(e)),
        }
    }
}
