pub mod error;
pub mod locking_tx_datastore;
pub mod system_tables;
pub mod traits;

use error::DatastoreError;

pub type Result<T> = core::result::Result<T, DatastoreError>;
