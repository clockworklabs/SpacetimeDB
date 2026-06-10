pub mod persistence {
    pub use spacetimedb_engine::persistence::*;
}

pub mod relational_db {
    pub use spacetimedb_engine::relational_db::*;
}

pub mod sql {
    pub mod ast {
        pub use spacetimedb_engine::sql::ast::*;
    }

    pub mod rls {
        pub use spacetimedb_engine::sql::rls::*;
    }
}

pub mod snapshot {
    pub use spacetimedb_engine::snapshot::*;
}

pub mod update {
    pub use spacetimedb_engine::update::*;
}

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
    /// Specifies the page pool max size in bytes.
    pub page_pool_max_size: Option<usize>,
}

pub type MetricsRecorderQueue = spacetimedb_engine::MetricsRecorderQueue;

pub type SchemaViewer<'a, T> = spacetimedb_engine::sql::ast::SchemaViewer<'a, T>;

pub fn spawn_tx_metrics_recorder(
    handle: &spacetimedb_runtime::Handle,
) -> (MetricsRecorderQueue, spacetimedb_runtime::AbortHandle) {
    spacetimedb_engine::spawn_tx_metrics_recorder(handle)
}
