pub mod datastore;
pub mod db_metrics;
pub mod query_context;
pub mod relational_db;
mod relational_operators;
pub mod update;

/// Whether SpacetimeDB is run in memory, or persists objects and
/// a message log to disk.
#[derive(Clone, Copy)]
pub enum Storage {
    /// The object store is in memory, and no message log is kept.
    Memory,

    /// The object store is persisted to disk, and a message log is kept.
    Disk,
}

/// Internal database config parameters
#[derive(Clone, Copy)]
pub struct Config {
    /// Specifies the object storage model.
    pub storage: Storage,
}
