use crate::error::DBError;
use spacetimedb_lib::buffer::DecodeError;
use spacetimedb_lib::PrimaryKey;
use thiserror::Error;

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
