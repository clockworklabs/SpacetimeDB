pub mod locking_tx_datastore;
pub mod system_tables;
pub mod traits;

use crate::error::DBError;

pub type Result<T> = core::result::Result<T, DBError>;
