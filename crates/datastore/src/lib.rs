
pub mod locking_tx_datastore;
pub mod system_tables;
pub mod traits;
pub mod error;
pub mod execution_context;
pub mod db_metrics;

use crate::error::DatastoreError;

pub type Result<T> = core::result::Result<T, DatastoreError>;
