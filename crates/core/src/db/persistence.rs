use std::{io, sync::Arc};

use async_trait::async_trait;
use spacetimedb_durability::{DurabilityExited, TxOffset};
use spacetimedb_paths::server::ServerDataDir;
use spacetimedb_snapshot::SnapshotRepository;

use crate::{messages::control_db::Database, util::asyncify};

use super::{
    relational_db::{self, Txdata},
    snapshot::{self, SnapshotDatabaseState, SnapshotWorker},
};

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
pub type DiskSizeFn = Arc<dyn Fn() -> io::Result<u64> + Send + Sync>;

/// Persistence services for a database.
pub struct Persistence {
    /// The [Durability] to use, for persisting transactions.
    pub durability: Arc<Durability>,
    /// The [DiskSizeFn].
    ///
    /// Currently the expectation is that the reported size is the commitlog
    /// size only.
    pub disk_size: DiskSizeFn,
    /// An optional [SnapshotWorker].
    ///
    /// The current expectation is that snapshots are only enabled for
    /// persistent (as opposed to in-memory) databases. This is enforced by
    /// this type.
    pub snapshots: Option<SnapshotWorker>,
}

impl Persistence {
    /// Convenience constructor of a [Persistence] that handles boxing.
    pub fn new(
        durability: impl spacetimedb_durability::Durability<TxData = Txdata> + 'static,
        disk_size: impl Fn() -> io::Result<u64> + Send + Sync + 'static,
        snapshots: Option<SnapshotWorker>,
    ) -> Self {
        Self {
            durability: Arc::new(durability),
            disk_size: Arc::new(disk_size),
            snapshots,
        }
    }

    /// If snapshots are enabled, get the [SnapshotRepository] they are stored in.
    pub fn snapshot_repo(&self) -> Option<&SnapshotRepository> {
        self.snapshots.as_ref().map(|worker| worker.repo())
    }

    /// Get the [TxOffset] reported as durable by the [Durability] impl.
    ///
    /// Returns `Ok(None)` if no offset is durable yet, and `Err(DurabilityExited)`
    /// if the [Durability] has shut down already.
    pub fn durable_tx_offset(&self) -> Result<Option<TxOffset>, DurabilityExited> {
        self.durability.durable_tx_offset().get()
    }

    /// Initialize the [SnapshotWorker], no-op if snapshots are not enabled.
    pub(super) fn set_snapshot_state(&self, state: SnapshotDatabaseState) {
        if let Some(worker) = &self.snapshots {
            worker.set_state(state)
        }
    }

    /// Convenience to deconstruct an [Option<Self>] into parts.
    ///
    /// Returns `(Some(durability), Some(disk_size), Option<SnapshotWorker>)`
    /// if `this` is `Some`, and `(None, None, None)` if `this` is `None`.
    pub(super) fn unzip(this: Option<Self>) -> (Option<Arc<Durability>>, Option<DiskSizeFn>, Option<SnapshotWorker>) {
        this.map(
            |Self {
                 durability,
                 disk_size,
                 snapshots,
             }| (Some(durability), Some(disk_size), snapshots),
        )
        .unwrap_or_default()
    }
}

/// A persistence provider is a "factory" of sorts that can produce [Persistence]
/// services for a given replica.
///
/// The [crate::host::HostController] uses this to obtain [Persistence]s from
/// an external source, and construct [relational_db::RelationalDB]s with it.
///
/// This is an `async_trait` to allow it to be used as a trait object.
#[async_trait]
pub trait PersistenceProvider: Send + Sync {
    async fn persistence(&self, database: &Database, replica_id: u64) -> anyhow::Result<Persistence>;
}

/// The standard [PersistenceProvider] for non-replicated databases.
///
/// [Persistence] services are provided for the local [ServerDataDir].
///
/// Note that its [PersistenceProvider::persistence] impl will spawn a
/// background task that [compresses] older commitlog segments whenever a
/// snapshot is taken.
///
/// [compresses]: relational_db::snapshot_watching_commitlog_compressor
pub struct LocalPersistenceProvider {
    data_dir: Arc<ServerDataDir>,
}

impl LocalPersistenceProvider {
    pub fn new(data_dir: impl Into<Arc<ServerDataDir>>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }
}

#[async_trait]
impl PersistenceProvider for LocalPersistenceProvider {
    async fn persistence(&self, database: &Database, replica_id: u64) -> anyhow::Result<Persistence> {
        let replica_dir = self.data_dir.replica(replica_id);
        let commitlog_dir = replica_dir.commit_log();
        let snapshot_dir = replica_dir.snapshots();

        let database_identity = database.database_identity;
        let snapshot_worker =
            asyncify(move || relational_db::open_snapshot_repo(snapshot_dir, database_identity, replica_id))
                .await
                .map(|repo| SnapshotWorker::new(repo, snapshot::Compression::Enabled))?;
        let (durability, disk_size) = relational_db::local_durability(commitlog_dir, Some(&snapshot_worker)).await?;

        tokio::spawn(relational_db::snapshot_watching_commitlog_compressor(
            snapshot_worker.subscribe(),
            None,
            None,
            durability.clone(),
        ));

        Ok(Persistence {
            durability,
            disk_size,
            snapshots: Some(snapshot_worker),
        })
    }
}
