pub mod error;
pub mod locking_tx_datastore;
pub mod system_tables;
pub mod traits;
pub mod execution_context;
pub mod db_metrics;

use error::DatastoreError;

pub type Result<T> = core::result::Result<T, DatastoreError>;
