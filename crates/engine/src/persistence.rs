use std::{
    io,
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
};

use async_trait::async_trait;
use spacetimedb_commitlog::SizeOnDisk;
use spacetimedb_paths::server::ServerDataDir;
use spacetimedb_table::{page_pool::PagePool, tiered::TieredStorageConfig};

use crate::util::asyncify;
use spacetimedb_lib::Identity;
use spacetimedb_runtime::Handle;

use super::{
    relational_db::{self, Txdata},
    snapshot::{self, SnapshotWorker},
};

/// Local durability configuration exposed through server config.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct DurabilityConfig {
    #[serde(default)]
    pub commitlog: CommitlogConfig,
}

/// Commitlog configuration exposed through server config.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct CommitlogConfig {
    pub log_format_version: Option<u8>,
    pub max_segment_size: Option<NonZeroU64>,
    #[serde(alias = "offset-interval-bytes")]
    pub offset_index_interval_bytes: Option<NonZeroU64>,
    #[serde(alias = "offset-index-require-fsync")]
    pub offset_index_require_segment_fsync: Option<bool>,
    pub preallocate_segments: Option<bool>,
    pub write_buffer_size: Option<NonZeroUsize>,
}

impl DurabilityConfig {
    fn into_options(self) -> spacetimedb_durability::local::Options {
        let mut opts = spacetimedb_durability::local::Options::default();
        self.commitlog.apply_to(&mut opts.commitlog);
        opts
    }
}

impl CommitlogConfig {
    fn apply_to(self, opts: &mut spacetimedb_commitlog::Options) {
        if let Some(log_format_version) = self.log_format_version {
            opts.log_format_version = log_format_version;
        }
        if let Some(max_segment_size) = self.max_segment_size {
            opts.max_segment_size = max_segment_size.get();
        }
        if let Some(offset_index_interval_bytes) = self.offset_index_interval_bytes {
            opts.offset_index_interval_bytes = offset_index_interval_bytes;
        }
        if let Some(offset_index_require_segment_fsync) = self.offset_index_require_segment_fsync {
            opts.offset_index_require_segment_fsync = offset_index_require_segment_fsync;
        }
        if let Some(preallocate_segments) = self.preallocate_segments {
            opts.preallocate_segments = preallocate_segments;
        }
        if let Some(write_buffer_size) = self.write_buffer_size {
            opts.write_buffer_size = write_buffer_size.get();
        }
    }
}

/// [spacetimedb_durability::Durability] impls with a [`Txdata`] transaction
/// payload, suitable for use in the [`relational_db::RelationalDB`].
pub type Durability = dyn spacetimedb_durability::Durability<TxData = Txdata>;

/// A function to determine the size on disk of the durable state of the
/// local database instance. This is used for metrics and energy accounting
/// purposes.
///
/// It is not part of the [`Durability`] trait because it must report disk
/// usage of the local instance only, even if exclusively remote durability is
/// configured or the database is in follower state.
pub type DiskSizeFn = Arc<dyn Fn() -> io::Result<SizeOnDisk> + Send + Sync>;

#[derive(Clone, Copy, Debug)]
pub struct Database {
    pub id: u64,
    pub database_identity: Identity,
    pub owner_identity: Identity,
}

pub struct Persistent {
    pub durability: Arc<Durability>,
    pub disk_size: DiskSizeFn,
    pub snapshots: Option<SnapshotWorker>,
    pub runtime: Handle,
}

impl Persistent {
    pub fn unzip(
        this: Option<Self>,
    ) -> (
        Option<Arc<Durability>>,
        Option<DiskSizeFn>,
        Option<SnapshotWorker>,
        Option<Handle>,
    ) {
        match this {
            Some(Self {
                durability,
                disk_size,
                snapshots,
                runtime,
            }) => (Some(durability), Some(disk_size), snapshots, Some(runtime)),
            None => (None, None, None, None),
        }
    }

    pub fn snapshots(this: &Option<Self>) -> Option<&SnapshotWorker> {
        let this = this.as_ref()?;
        this.snapshots.as_ref()
    }
}

/// Persistence services for a database.
pub struct Persistence {
    pub persistent: Option<Persistent>,
    /// Provides tiered storage for tables.
    pub table_storage: TieredStorageConfig,
}

impl Persistence {
    pub fn persistent(
        durability: Arc<Durability>,
        disk_size: DiskSizeFn,
        snapshots: Option<SnapshotWorker>,
        table_storage: TieredStorageConfig,
        runtime: Handle,
    ) -> Self {
        Self {
            persistent: Some(Persistent {
                durability,
                disk_size,
                snapshots,
                runtime,
            }),
            table_storage,
        }
    }

    pub fn ephemeral(table_storage: TieredStorageConfig) -> Self {
        Self {
            persistent: None,
            table_storage,
        }
    }

    pub fn for_module_check(page_pool: PagePool) -> Self {
        Self::ephemeral(TieredStorageConfig::new_for_module_check(page_pool))
    }
}

/// A persistence provider is a "factory" of sorts that can produce [Persistence]
/// services for a given replica.
///
/// The host controller uses this to obtain [Persistence]s from
/// an external source, and construct [relational_db::RelationalDB]s with it.
///
/// This is an `async_trait` to allow it to be used as a trait object.
#[async_trait]
pub trait PersistenceProvider: Send + Sync {
    async fn persistent(&self, database: &Database, replica_id: u64) -> anyhow::Result<Persistence>;
    async fn ephemeral(&self, database: &Database, replica_id: u64) -> anyhow::Result<Persistence>;
}

/// The standard [PersistenceProvider] for non-replicated databases.
///
/// [Persistence] services are provided for the local [ServerDataDir].
///
/// Note that its [PersistenceProvider::persistent] impl will spawn a
/// background task that [compresses] older commitlog segments whenever a
/// snapshot is taken.
///
/// [compresses]: relational_db::snapshot_watching_commitlog_compressor
pub struct LocalPersistenceProvider {
    data_dir: Arc<ServerDataDir>,
    durability: DurabilityConfig,
    page_pool: PagePool,
}

impl LocalPersistenceProvider {
    pub fn new(data_dir: impl Into<Arc<ServerDataDir>>, page_pool: PagePool) -> Self {
        Self {
            data_dir: data_dir.into(),
            durability: DurabilityConfig::default(),
            page_pool,
        }
    }

    pub fn with_durability_config(mut self, durability: DurabilityConfig) -> Self {
        self.durability = durability;
        self
    }

    fn table_storage(&self) -> TieredStorageConfig {
        TieredStorageConfig::new_for_memory_database(self.page_pool.clone())
    }
}

#[async_trait]
impl PersistenceProvider for LocalPersistenceProvider {
    async fn persistent(&self, database: &Database, replica_id: u64) -> anyhow::Result<Persistence> {
        let database_identity = database.database_identity;
        let replica_dir = self.data_dir.replica(replica_id);
        let snapshot_dir = replica_dir.snapshots();
        let runtime = Handle::tokio_current();

        let snapshot_worker = asyncify(&runtime, move || {
            relational_db::open_snapshot_repo(snapshot_dir, database_identity, replica_id)
        })
        .await
        .map(|repo| SnapshotWorker::new(repo, snapshot::Compression::Enabled, runtime.clone()))?;
        let (durability, disk_size) = relational_db::local_durability_with_options(
            replica_dir,
            runtime.clone(),
            Some(&snapshot_worker),
            self.durability.into_options(),
        )
        .await?;

        runtime.spawn(relational_db::snapshot_watching_commitlog_compressor(
            snapshot_worker.subscribe(),
            None,
            None,
            durability.clone(),
            runtime.clone(),
        ));

        Ok(Persistence::persistent(
            durability,
            disk_size,
            Some(snapshot_worker),
            self.table_storage(),
            runtime,
        ))
    }

    async fn ephemeral(&self, _database: &Database, _replica_id: u64) -> anyhow::Result<Persistence> {
        Ok(Persistence::ephemeral(self.table_storage()))
    }
}
