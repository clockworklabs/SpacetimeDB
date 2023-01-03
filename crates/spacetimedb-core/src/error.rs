use crate::db::index::{IndexDef, IndexId};
use crate::db::message_log::MessageLog;
use crate::db::sequence::SequenceError;
use hex::FromHexError;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::error::LibError;
use spacetimedb_lib::{PrimaryKey, TupleValue, TypeValue};
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
    #[error("Scan Table with ID `{0}` not found.")]
    ScanTableIdNotFound(u32),
    #[error("Scan PK with Table ID `{0}` not found.")]
    ScanPkTableIdNotFound(u32),
    #[error("Decode Row Seek Table with ID `{0}` failed with {1}.")]
    DecodeSeekTableIdNotFound(u32, LibError),
    #[error("Column `{0}.{1}` is missing a name")]
    ColumnWithoutName(String, u32),
    #[error("schema_for_table: Table has invalid schema: {0} Err: {1}")]
    InvalidSchema(u32, LibError),
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum IndexError {
    #[error("Index not found: {0}")]
    NotFound(IndexId),
    #[error("Index already exist: {0}: {1}")]
    IndexAlreadyExists(IndexDef, String),
    #[error("Column not found: {0}")]
    ColumnNotFound(IndexDef),
    #[error("Index is duplicated: {0}:{1}")]
    Duplicated(IndexDef, TypeValue, TupleValue),
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
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<PoisonError<std::sync::MutexGuard<'_, MessageLog>>> for DBError {
    fn from(err: PoisonError<MutexGuard<'_, MessageLog>>) -> Self {
        DBError::MessageLogPoisoned(err.to_string())
    }
}

#[derive(Error, Debug)]
pub enum NodesError {
    #[error("insert: Failed to decode row: table_id: {table_id} Err: {e}")]
    InsertDecode { table_id: u32, e: DecodeError },
    #[error("insert: Failed to insert row: table_id: {table_id} Err: {e}")]
    InsertRow { table_id: u32, e: DBError },
    #[error("delete: Failed to decode row: table_id: {table_id} Err: {e}")]
    DeleteDecode { table_id: u32, e: DecodeError },
    #[error("delete: Failed to delete row: table_id: {table_id} Err: {e}")]
    DeleteRow { table_id: u32, e: DBError },
    #[error("delete: Not found Pk: table_id: {table_id} Pk: {pk:?}")]
    DeleteNotFound { table_id: u32, pk: PrimaryKey },
    #[error("delete: Not found value: table_id: {table_id}")]
    DeleteValueNotFound { table_id: u32 },
    #[error("delete_range: Failed to scan range: {table_id} Err: {e}")]
    DeleteScanRange { table_id: u32, e: DBError },
    #[error("delete_range: Failed to delete in range: {table_id} Err: {e}")]
    DeleteRange { table_id: u32, e: DBError },
    #[error("delete: Not found Range: table_id: {table_id}")]
    DeleteRangeNotFound { table_id: u32 },
}

pub fn log_to_err(err: NodesError) -> NodesError {
    log::error!("{err}");
    err
}
