use hex::FromHexError;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::error::LibError;
use std::num::ParseIntError;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TableError {
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

#[derive(Error, Debug)]
pub enum DBError {
    #[error("LibError: {0}")]
    Lib(#[from] LibError),
    #[error("BufferError: {0}")]
    Buffer(#[from] DecodeError),
    #[error("TableError: {0}")]
    Table(#[from] TableError),
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
    #[cfg(feature = "odb_sled")]
    #[error("SledError: {0}.")]
    SledDbError(#[from] sled::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
