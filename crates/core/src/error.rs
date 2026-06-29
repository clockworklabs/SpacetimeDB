pub use spacetimedb_engine::error::*;

use crate::host::module_host::ViewCallError;
use crate::host::scheduler::ScheduleError;
use crate::host::AbiCall;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_schema::def::error::RelationError;
use spacetimedb_schema::table_name::TableName;
use thiserror::Error;

impl From<ViewCallError> for DBError {
    fn from(err: ViewCallError) -> Self {
        match err {
            ViewCallError::Args(err) => spacetimedb_engine::error::ViewError::Args(err.to_string()).into(),
            ViewCallError::NoSuchModule(err) => {
                spacetimedb_engine::error::ViewError::NoSuchModule(err.to_string()).into()
            }
            ViewCallError::NoSuchView => spacetimedb_engine::error::ViewError::NoSuchView.into(),
            ViewCallError::TableDoesNotExist(view_id) => {
                spacetimedb_engine::error::ViewError::TableDoesNotExist(view_id).into()
            }
            ViewCallError::MissingClientConnection => {
                spacetimedb_engine::error::ViewError::MissingClientConnection.into()
            }
            ViewCallError::DatastoreError(err) => spacetimedb_engine::error::ViewError::DatastoreError(err).into(),
            ViewCallError::InternalError(err) => spacetimedb_engine::error::ViewError::InternalError(err).into(),
        }
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
    #[error("index with provided name or id doesn't exist")]
    IndexNotFound,
    #[error("index was not unique")]
    IndexNotUnique,
    #[error("row was not found in index")]
    IndexRowNotFound,
    #[error("index does not support range scans")]
    IndexCannotSeekRange,
    #[error("column is out of bounds")]
    BadColumn,
    #[error("can't perform operation; not inside transaction")]
    NotInTransaction,
    #[error("can't perform operation; not inside anonymous transaction")]
    NotInAnonTransaction,
    #[error("ABI call not allowed while holding open a transaction: {0}")]
    WouldBlockTransaction(AbiCall),
    #[error("table with name `{0}` start with 'st_' and that is reserved for internal system tables.")]
    SystemName(TableName),
    #[error("internal db error: {0}")]
    Internal(#[source] Box<DBError>),
    #[error(transparent)]
    BadQuery(#[from] RelationError),
    #[error("invalid index type: {0}")]
    BadIndexType(u8),
    #[error("Failed to scheduled timer: {0}")]
    ScheduleError(#[source] ScheduleError),
    #[error("HTTP request failed: {0}")]
    HttpError(String),
}

impl From<DBError> for NodesError {
    fn from(e: DBError) -> Self {
        match e {
            DBError::Datastore(
                DatastoreError::Table(TableError::IdNotFound(_, _)) | DatastoreError::Table(TableError::NotFound(_)),
            ) => Self::TableNotFound,
            DBError::Datastore(DatastoreError::Table(TableError::ColumnNotFound(_))) => Self::BadColumn,
            DBError::Datastore(DatastoreError::Index(IndexError::NotFound(_))) => Self::IndexNotFound,
            DBError::Datastore(DatastoreError::Index(IndexError::Decode(e))) => Self::DecodeRow(e),
            DBError::Datastore(DatastoreError::Index(IndexError::NotUnique(_))) => Self::IndexNotUnique,
            DBError::Datastore(DatastoreError::Index(IndexError::KeyNotFound(..))) => Self::IndexRowNotFound,
            _ => Self::Internal(Box::new(e)),
        }
    }
}
