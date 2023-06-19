pub mod gitlike_tx_blobstore;
pub mod locking_tx_datastore;
pub mod system_tables;
pub mod traits;

mod freelist_allocator;
mod memory;

use crate::error::DBError;

pub type Result<T> = core::result::Result<T, DBError>;
