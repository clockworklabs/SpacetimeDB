pub mod commit_log;
pub mod cursor;
pub mod datastore;
pub mod db_metrics;
pub mod message_log;
pub mod messages;
pub mod ostorage;
pub mod relational_db;
mod relational_operators;
pub mod update;

pub use spacetimedb_lib::{AlgebraicType, AlgebraicValue, ProductType, ProductValue};

/// Whether SpacetimeDB is run in memory, or persists objects and
/// a message log to disk.
#[derive(Clone, Copy)]
pub enum Storage {
    /// The object store is in memory, and no message log is kept.
    Memory,

    /// The object store is persisted to disk, and a message log is kept.
    Disk,
}

/// How often Txn messages are physically persisted to the WAL.
#[derive(Clone, Copy, PartialEq)]
pub enum FsyncPolicy {
    /// Flush WAL writes to OS buffers and let OS schedule the write to disk.
    Never,
    /// Every Txn should be fsync'd to disk.
    EveryTx,
}

/// Internal database config parameters
#[derive(Clone, Copy)]
pub struct Config {
    /// Specifies whether writes to the WAL should be fsync'd.
    pub fsync: FsyncPolicy,
    /// Specifies the object storage model.
    pub storage: Storage,
}
