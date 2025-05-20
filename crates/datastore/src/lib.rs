pub mod locking_tx_datastore;
pub mod system_tables;
pub mod traits;
pub mod error;
pub mod execution_context;

use spacetimedb::error::DBError;

pub type Result<T> = core::result::Result<T, DBError>;
