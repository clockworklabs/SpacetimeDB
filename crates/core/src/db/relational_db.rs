use crate::db::MetricsRecorderQueue;
use crate::error::{DBError, DatabaseError, RestoreSnapshotError};
use crate::messages::control_db::HostType;
use crate::subscription::ExecutionCounters;
use crate::util::{asyncify, spawn_rayon};
use crate::worker_metrics::WORKER_METRICS;
use anyhow::{anyhow, Context};
use enum_map::EnumMap;
use fs2::FileExt;
use spacetimedb_commitlog as commitlog;
use spacetimedb_commitlog::repo::OnNewSegmentFn;
use spacetimedb_data_structures::map::IntSet;
use spacetimedb_datastore::db_metrics::DB_METRICS;
use spacetimedb_datastore::error::{DatastoreError, TableError};
use spacetimedb_datastore::execution_context::{ReducerContext, Workload, WorkloadType};
use spacetimedb_datastore::locking_tx_datastore::datastore::TxMetrics;
use spacetimedb_datastore::locking_tx_datastore::state_view::{
    IterByColEqMutTx, IterByColRangeMutTx, IterMutTx, IterTx, StateView,
};
use spacetimedb_datastore::locking_tx_datastore::{MutTxId, TxId};
use spacetimedb_datastore::system_tables::{system_tables, StModuleRow};
use spacetimedb_datastore::system_tables::{StFields, StVarFields, StVarName, StVarRow, ST_MODULE_ID, ST_VAR_ID};
use spacetimedb_datastore::traits::{
    InsertFlags, IsolationLevel, Metadata, MutTx as _, MutTxDatastore, Program, RowTypeForTable, Tx as _, TxDatastore,
    UpdateFlags,
};
use spacetimedb_datastore::{
    locking_tx_datastore::{
        datastore::Locking,
        state_view::{IterByColEqTx, IterByColRangeTx},
    },
    traits::TxData,
};
use spacetimedb_durability as durability;
use spacetimedb_lib::db::auth::StAccess;
use spacetimedb_lib::db::raw_def::v9::{btree, RawModuleDefV9Builder, RawSql};
use spacetimedb_lib::st_var::StVarValue;
use spacetimedb_lib::ConnectionId;
use spacetimedb_lib::Identity;
use spacetimedb_paths::server::{CommitLogDir, ReplicaDir, SnapshotsPath};
use spacetimedb_primitives::*;
use spacetimedb_sats::algebraic_type::fmt::fmt_algebraic_type;
use spacetimedb_sats::memory_usage::MemoryUsage;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductType, ProductValue};
use spacetimedb_schema::def::{ModuleDef, TableDef, ViewDef};
use spacetimedb_schema::schema::{
    ColumnSchema, IndexSchema, RowLevelSecuritySchema, Schema, SequenceSchema, TableSchema,
};
use spacetimedb_snapshot::{ReconstructedSnapshot, SnapshotError, SnapshotRepository};
use spacetimedb_table::indexes::RowPointer;
use spacetimedb_table::page_pool::PagePool;
use spacetimedb_table::table::RowRef;
use spacetimedb_vm::errors::{ErrorType, ErrorVm};
use spacetimedb_vm::ops::parse;
use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt;
use std::fs::File;
use std::io;
use std::ops::{Bound, RangeBounds};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::watch;

pub use super::persistence::{DiskSizeFn, Durability, Persistence};
pub use super::snapshot::SnapshotWorker;
pub use durability::{DurableOffset, TxOffset};

// NOTE(cloutiertyler): We should be using the associated types, but there is
// a bug in the Rust compiler that prevents us from doing so.
pub type MutTx = MutTxId; //<Locking as spacetimedb_datastore::traits::MutTx>::MutTx;
pub type Tx = TxId; //<Locking as spacetimedb_datastore::traits::Tx>::Tx;

type RowCountFn = Arc<dyn Fn(TableId, &str) -> i64 + Send + Sync>;

/// The type of transactions committed by [RelationalDB].
pub type Txdata = commitlog::payload::Txdata<ProductValue>;

/// We've added a module version field to the system tables, but we don't yet
/// have the infrastructure to support multiple versions.
/// All modules are currently locked to this version, but this will be
/// relaxed post 1.0.
pub const ONLY_MODULE_VERSION: &str = "0.0.1";

/// The set of clients considered connected to the database.
///
/// A client is considered connected if there exists a corresponding row in the
/// `st_clients` system table.
///
/// If rows exist in `st_clients` upon [`RelationalDB::open`], the database was
/// not shut down gracefully. Such "dangling" clients should be removed by
/// calling [`crate::host::ModuleHost::call_identity_connected_disconnected`]
/// for each entry in [`ConnectedClients`].
pub type ConnectedClients = HashSet<(Identity, ConnectionId)>;

#[derive(Clone)]
pub struct RelationalDB {
    database_identity: Identity,
    owner_identity: Identity,

    inner: Locking,
    durability: Option<Arc<Durability>>,
    snapshot_worker: Option<SnapshotWorker>,

    row_count_fn: RowCountFn,
    /// Function to determine the durable size on disk.
    /// `Some` if `durability` is `Some`, `None` otherwise.
    disk_size_fn: Option<DiskSizeFn>,

    /// A map from workload types to their cached prometheus counters.
    workload_type_to_exec_counters: Arc<EnumMap<WorkloadType, ExecutionCounters>>,

    /// An async queue for recording transaction metrics off the main thread
    metrics_recorder_queue: Option<MetricsRecorderQueue>,

    // DO NOT ADD FIELDS AFTER THIS.
    // By default, fields are dropped in declaration order.
    // We want to release the file lock last.
    // TODO(noa): is this lockfile still necessary now that we have data-dir?
    _lock: LockFile,
}

/// Perform a snapshot every `SNAPSHOT_FREQUENCY` transactions.
// TODO(config): Allow DBs to specify how frequently to snapshot.
// TODO(bikeshedding): Snapshot based on number of bytes written to commitlog, not tx offsets.
//
// NOTE: Replicas must agree on the snapshot frequency. By making them consult
// this value, later introduction of dynamic configuration will allow the
// compiler to find external dependencies.
pub const SNAPSHOT_FREQUENCY: u64 = 1_000_000;

impl std::fmt::Debug for RelationalDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelationalDB")
            .field("identity", &self.database_identity)
            .finish()
    }
}

impl RelationalDB {
    fn new(
        lock: LockFile,
        database_identity: Identity,
        owner_identity: Identity,
        inner: Locking,
        persistence: Option<Persistence>,
        metrics_recorder_queue: Option<MetricsRecorderQueue>,
    ) -> Self {
        let workload_type_to_exec_counters =
            Arc::new(EnumMap::from_fn(|ty| ExecutionCounters::new(&ty, &database_identity)));

        let (durability, disk_size_fn, snapshot_worker) = Persistence::unzip(persistence);

        Self {
            inner,
            durability,
            snapshot_worker,

            database_identity,
            owner_identity,

            row_count_fn: default_row_count_fn(database_identity),
            disk_size_fn,

            workload_type_to_exec_counters,
            metrics_recorder_queue,

            _lock: lock,
        }
    }

    /// Open a database, which may or may not already exist.
    ///
    /// # Initialization
    ///
    /// When this method returns, the internal state of the database has been
    /// initialized with nothing written to disk (regardless of the `durability`
    /// setting).
    ///
    /// This allows to hand over a pointer to the database to a [`ModuleHost`][ModuleHost]
    /// for initialization, which will call [`Self::set_initialized`],
    /// initializing the database's [`Metadata`] transactionally.
    ///
    /// If, however, a non-empty `history` was supplied, [`Metadata`] will
    /// already be be set. In this case, i.e. if either [`Self::metadata`] or
    /// [`Self::program_bytes`] return a `Some` value, [`Self::set_initialized`]
    /// should _not_ be called.
    ///
    /// Sometimes, one may want to obtain a database without a module (e.g. for
    /// testing). In this case, **always** call [`Self::set_initialized`],
    /// supplying a zero `program_hash` and empty `program_bytes`.
    ///
    /// # Parameters
    ///
    /// - `root`
    ///
    ///   The database directory. Does not need to exist.
    ///
    ///   Note that, even if no `durability` is supplied, the directory will be
    ///   created and equipped with an advisory lock file.
    ///
    /// - `database_identity`
    ///
    ///   The [`Identity`] of the database.
    ///
    ///   An error is returned if the database already exists, but has a
    ///   different identity.
    ///   If it is a new database, the identity is stored in the database's
    ///   system tables upon calling [`Self::set_initialized`].
    ///
    /// - `owner_identity`
    ///
    ///   The [`Identity`] of the database's owner.
    ///
    ///   An error is returned if the database already exists, but has a
    ///   different owner.
    ///   If it is a new database, the identity is stored in the database's
    ///   system tables upon calling [`Self::set_initialized`].
    ///
    /// - `history`
    ///
    ///   The [`durability::History`] to restore the database from.
    ///
    ///   If using local durability, this must be a pointer to the same object.
    ///   [`durability::EmptyHistory`] can be used to start from an empty history.
    ///
    /// - `durability`
    ///
    ///   The [`Durability`] implementation to use, along with a [`DiskSizeFn`]
    ///   reporting its size on disk. The [`DiskSizeFn`] must report zero if
    ///   this database is a follower instance.
    ///
    ///   `None` may be passed to obtain an in-memory only database.
    ///
    /// - `snapshot_repo`
    ///
    ///   The [`SnapshotRepository`] which stores snapshots of this database.
    ///   This is only meaningful if `history` and `durability` are also supplied.
    ///   If restoring from an existing database, the `snapshot_repo` must
    ///   store views of the same sequence of TXes as the `history`.
    ///
    /// - `metrics_recorder_queue`
    ///
    ///   The send side of a queue for recording transaction metrics.
    ///
    /// # Return values
    ///
    /// Alongside `Self`, [`ConnectedClients`] is returned, which is the set of
    /// clients considered connected at the given snapshot and `history`.
    ///
    /// If [`ConnectedClients`] is non-empty, the database did not shut down
    /// gracefully. The caller is responsible for disconnecting the clients.
    ///
    /// [ModuleHost]: crate::host::module_host::ModuleHost
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        root: &ReplicaDir,
        database_identity: Identity,
        owner_identity: Identity,
        history: impl durability::History<TxData = Txdata>,
        mut persistence: Option<Persistence>,
        metrics_recorder_queue: Option<MetricsRecorderQueue>,
        page_pool: PagePool,
    ) -> Result<(Self, ConnectedClients), DBError> {
        log::trace!("[{database_identity}] DATABASE: OPEN");

        let lock = LockFile::lock(root)?;

        // Check the latest durable TX and restore from a snapshot no newer than it,
        // so that you drop TXes which were committed but not durable before the restart.
        // TODO: delete or mark as invalid snapshots newer than this.
        let durable_tx_offset = persistence
            .as_ref()
            .map(|p| p.durable_tx_offset())
            .transpose()?
            .flatten();
        let (min_commitlog_offset, _) = history.tx_range_hint();

        log::info!("[{database_identity}] DATABASE: durable_tx_offset is {durable_tx_offset:?}");

        let start_time = std::time::Instant::now();

        let inner = Self::restore_from_snapshot_or_bootstrap(
            database_identity,
            persistence.as_ref().and_then(|p| p.snapshot_repo()),
            durable_tx_offset,
            min_commitlog_offset,
            page_pool,
        )?;
        if let Some(persistence) = &mut persistence {
            // Sanity check because the snapshot worker could've been used before.
            debug_assert!(
                persistence
                    .snapshot_repo()
                    .map(|repo| repo.database_identity() == database_identity)
                    .unwrap_or(true),
                "snapshot repository does not match database identity",
            );
            persistence.set_snapshot_state(inner.committed_state.clone());
        }

        apply_history(&inner, database_identity, history)?;

        let elapsed_time = start_time.elapsed();
        WORKER_METRICS
            .replay_total_time_seconds
            .with_label_values(&database_identity)
            .set(elapsed_time.as_secs_f64());

        let db = Self::new(
            lock,
            database_identity,
            owner_identity,
            inner,
            persistence,
            metrics_recorder_queue,
        );
        db.migrate_system_tables()?;

        if let Some(meta) = db.metadata()? {
            if meta.database_identity != database_identity {
                return Err(anyhow!(
                    "mismatched database identity: {} != {}",
                    meta.database_identity,
                    database_identity
                )
                .into());
            }
            if meta.owner_identity != owner_identity {
                return Err(anyhow!(
                    "mismatched owner identity: {} != {}",
                    meta.owner_identity,
                    owner_identity
                )
                .into());
            }
        };
        let connected_clients = db.connected_clients()?;

        Ok((db, connected_clients))
    }

    fn migrate_system_tables(&self) -> Result<(), DBError> {
        let mut tx = self.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
        for schema in system_tables() {
            if !self.table_id_exists_mut(&tx, &schema.table_id) {
                log::info!(
                    "[{}] DATABASE: adding missing system table {}",
                    self.database_identity,
                    schema.table_name
                );
                let _ = self.create_table(&mut tx, schema.clone())?;
            }
        }
        let _ = self.commit_tx(tx)?;
        self.inner.assert_system_tables_match()?;
        Ok(())
    }

    /// Mark the database as initialized with the given module parameters.
    ///
    /// Records the database's identity, owner and module parameters in the
    /// system tables. The transactional context is supplied by the caller.
    ///
    /// It is an error to call this method on an already-initialized database.
    ///
    /// See [`Self::open`] for further information.
    pub fn set_initialized(&self, tx: &mut MutTx, host_type: HostType, program: Program) -> Result<(), DBError> {
        log::trace!(
            "[{}] DATABASE: set initialized owner={} program_hash={}",
            self.database_identity,
            self.owner_identity,
            program.hash
        );

        // Probably a bug: the database is already initialized.
        // Ignore if it would be a no-op.
        if let Some(meta) = self.inner.metadata_mut_tx(tx)? {
            if program.hash == meta.program_hash
                && self.database_identity == meta.database_identity
                && self.owner_identity == meta.owner_identity
            {
                return Ok(());
            }
            return Err(anyhow!("database {} already initialized", self.database_identity).into());
        }
        let row = StModuleRow {
            database_identity: self.database_identity.into(),
            owner_identity: self.owner_identity.into(),

            program_kind: host_type.into(),
            program_hash: program.hash,
            program_bytes: program.bytes,
            module_version: ONLY_MODULE_VERSION.into(),
        };
        Ok(tx.insert_via_serialize_bsatn(ST_MODULE_ID, &row).map(drop)?)
    }

    /// Obtain the [`Metadata`] of this database.
    ///
    /// `None` if the database is not yet fully initialized.
    pub fn metadata(&self) -> Result<Option<Metadata>, DBError> {
        Ok(self.with_read_only(Workload::Internal, |tx| self.inner.metadata(tx))?)
    }

    /// Obtain the module associated with this database.
    ///
    /// `None` if the database is not yet fully initialized.
    /// Note that a `Some` result may yield an empty slice.
    pub fn program(&self) -> Result<Option<Program>, DBError> {
        Ok(self.with_read_only(Workload::Internal, |tx| self.inner.program(tx))?)
    }

    /// Read the set of clients currently connected to the database.
    pub fn connected_clients(&self) -> Result<ConnectedClients, DBError> {
        self.with_read_only(Workload::Internal, |tx| {
            self.inner
                .connected_clients(tx)?
                .collect::<Result<ConnectedClients, _>>()
        })
        .map_err(DBError::from)
    }

    /// Update the module associated with this database.
    ///
    /// The caller must ensure that:
    ///
    /// - `program.hash` is the [`Hash`] over `program.bytes`.
    /// - `program.bytes` is a valid module acc. to `host_type`.
    /// - the schema updates contained in the module have been applied within
    ///   the transactional context `tx`.
    /// - the `__init__` reducer contained in the module has been executed
    ///   within the transactional context `tx`.
    pub fn update_program(&self, tx: &mut MutTx, host_type: HostType, program: Program) -> Result<(), DBError> {
        Ok(self.inner.update_program(tx, host_type.into(), program)?)
    }

    fn restore_from_snapshot_or_bootstrap(
        database_identity: Identity,
        snapshot_repo: Option<&SnapshotRepository>,
        durable_tx_offset: Option<TxOffset>,
        min_commitlog_offset: TxOffset,
        page_pool: PagePool,
    ) -> Result<Locking, RestoreSnapshotError> {
        // Try to load the `ReconstructedSnapshot` at `snapshot_offset`.
        fn try_load_snapshot(
            database_identity: &Identity,
            snapshot_repo: &SnapshotRepository,
            snapshot_offset: TxOffset,
            page_pool: &PagePool,
        ) -> Result<ReconstructedSnapshot, Box<SnapshotError>> {
            log::info!("[{database_identity}] DATABASE: restoring snapshot of tx_offset {snapshot_offset}");
            let start = std::time::Instant::now();

            let snapshot = snapshot_repo
                .read_snapshot(snapshot_offset, page_pool)
                .map_err(Box::new)?;

            let elapsed_time = start.elapsed();

            WORKER_METRICS
                .replay_snapshot_read_time_seconds
                .with_label_values(database_identity)
                .set(elapsed_time.as_secs_f64());

            log::info!(
                "[{database_identity}] DATABASE: read snapshot of tx_offset {snapshot_offset} in {elapsed_time:?}",
            );

            Ok(snapshot)
        }

        // Do restore a `Locking` from the `ReconstructedSnapshot`.
        fn restore_from_snapshot(
            database_identity: &Identity,
            snapshot: ReconstructedSnapshot,
            page_pool: PagePool,
        ) -> Result<Locking, Box<DBError>> {
            let start = std::time::Instant::now();
            let snapshot_offset = snapshot.tx_offset;
            Locking::restore_from_snapshot(snapshot, page_pool)
                .inspect(|_| {
                    let elapsed_time = start.elapsed();

                    WORKER_METRICS.replay_snapshot_restore_time_seconds.with_label_values(database_identity).set(elapsed_time.as_secs_f64());

                    log::info!(
                        "[{database_identity}] DATABASE: restored from snapshot of tx_offset {snapshot_offset} in {elapsed_time:?}",
                    )
                })
                .inspect_err(|e| {
                    log::warn!(
                        "[{database_identity}] DATABASE: failed to restore snapshot of tx_offset {snapshot_offset}: {e}"
                    )
                })
                .map_err(DBError::from)
                .map_err(Box::new)
        }

        // `true` if the `SnapshotError` can be considered transient.
        // It is not transient if it has to do with hash verification,
        // deserialization or the snapshot format itself.
        fn is_transient_error(e: &SnapshotError) -> bool {
            match e {
                SnapshotError::Open(_)
                | SnapshotError::WriteObject { .. }
                | SnapshotError::ReadObject { .. }
                | SnapshotError::Serialize { .. }
                | SnapshotError::Incomplete { .. }
                | SnapshotError::NotDirectory { .. }
                | SnapshotError::Lockfile(_)
                | SnapshotError::Io(_) => true,

                SnapshotError::HashMismatch { .. }
                | SnapshotError::Deserialize { .. }
                | SnapshotError::BadMagic { .. }
                | SnapshotError::BadVersion { .. } => false,
            }
        }

        if let Some((snapshot_repo, durable_tx_offset)) = snapshot_repo.zip(durable_tx_offset) {
            // Mark any newer snapshots as invalid, as the history past
            // `durable_tx_offset` may have been reset and thus diverge from
            // any snapshots taken earlier.
            snapshot_repo
                .invalidate_newer_snapshots(durable_tx_offset)
                .map_err(|e| RestoreSnapshotError::Invalidate {
                    offset: durable_tx_offset,
                    source: Box::new(e),
                })?;

            // Try to restore from any snapshot that was taken within the
            // range `(min_commitlog_offset + 1)..=durable_tx_offset`.
            let mut upper_bound = durable_tx_offset;
            loop {
                let Some(snapshot_offset) = snapshot_repo
                    .latest_snapshot_older_than(upper_bound)
                    .map_err(Box::new)?
                else {
                    break;
                };
                if min_commitlog_offset > 0 && min_commitlog_offset > snapshot_offset + 1 {
                    log::debug!("snapshot_offset={snapshot_offset} min_commitlog_offset={min_commitlog_offset}");
                    break;
                }
                match try_load_snapshot(&database_identity, snapshot_repo, snapshot_offset, &page_pool) {
                    Ok(snapshot) if snapshot.database_identity != database_identity => {
                        return Err(RestoreSnapshotError::IdentityMismatch {
                            expected: database_identity,
                            actual: snapshot.database_identity,
                        });
                    }
                    Ok(snapshot) => {
                        return restore_from_snapshot(&database_identity, snapshot, page_pool)
                            .map_err(RestoreSnapshotError::Datastore);
                    }
                    Err(e) => {
                        // Invalidate the snapshot if the error is permanent.
                        // Newly created snapshots should not depend on it.
                        if !is_transient_error(&e) {
                            let path = snapshot_repo.snapshot_dir_path(snapshot_offset);
                            log::info!("invalidating bad snapshot at {}", path.display());
                            path.rename_invalid().map_err(|e| RestoreSnapshotError::Invalidate {
                                offset: snapshot_offset,
                                source: Box::new(e.into()),
                            })?;
                        }
                        // Try the next older one if the error was transient.
                        //
                        // `latest_snapshot_older_than` is inclusive of the
                        // upper bound, so subtract one and give up if there
                        // are no more offsets to try.
                        match snapshot_offset.checked_sub(1) {
                            None => break,
                            Some(older_than) => upper_bound = older_than,
                        }
                    }
                }
            }
        }
        log::info!("[{database_identity}] DATABASE: no usable snapshot on disk");

        // If we didn't find a snapshot and the commitlog doesn't start at the
        // zero-th commit (e.g. due to archiving), there is no way to restore
        // the database.
        if min_commitlog_offset > 0 {
            return Err(RestoreSnapshotError::NoConnectedSnapshot { min_commitlog_offset });
        }

        Locking::bootstrap(database_identity, page_pool)
            .map_err(DBError::from)
            .map_err(Box::new)
            .map_err(RestoreSnapshotError::Bootstrap)
    }

    /// Apply the provided [`spacetimedb_durability::History`] onto the database
    /// state.
    ///
    /// Consumes `self` in order to ensure exclusive access, and to prevent use
    /// of the database in case of an incomplete replay.
    /// This restriction may be lifted in the future to allow for "live" followers.
    pub fn apply<T>(self, history: T) -> Result<Self, DBError>
    where
        T: durability::History<TxData = Txdata>,
    {
        apply_history(&self.inner, self.database_identity, history)?;
        Ok(self)
    }

    /// Returns an approximate row count for a particular table.
    /// TODO: Unify this with `Relation::row_count` when more statistics are added.
    pub fn row_count(&self, table_id: TableId, table_name: &str) -> i64 {
        (self.row_count_fn)(table_id, table_name)
    }

    /// Update this `RelationalDB` with an approximate row count function.
    pub fn with_row_count(mut self, row_count: RowCountFn) -> Self {
        self.row_count_fn = row_count;
        self
    }

    /// Returns the identity for this database
    pub fn database_identity(&self) -> Identity {
        self.database_identity
    }

    /// The number of bytes on disk occupied by the durability layer.
    ///
    /// If this is an in-memory instance, `Ok(0)` is returned.
    pub fn size_on_disk(&self) -> io::Result<u64> {
        self.disk_size_fn.as_ref().map_or(Ok(0), |f| f())
    }

    /// The size in bytes of all of the in-memory data in this database.
    pub fn size_in_memory(&self) -> usize {
        self.inner.heap_usage()
    }

    /// Update data size metrics.
    pub fn update_data_size_metrics(&self) {
        let cs = self.inner.committed_state.read();

        cs.report_data_size(self.database_identity)
    }

    pub fn encode_row(row: &ProductValue, bytes: &mut Vec<u8>) {
        // TODO: large file storage of the row elements
        row.encode(bytes);
    }

    pub fn schema_for_table_mut(&self, tx: &MutTx, table_id: TableId) -> Result<Arc<TableSchema>, DBError> {
        Ok(self.inner.schema_for_table_mut_tx(tx, table_id)?)
    }

    pub fn schema_for_table(&self, tx: &Tx, table_id: TableId) -> Result<Arc<TableSchema>, DBError> {
        Ok(self.inner.schema_for_table_tx(tx, table_id)?)
    }

    pub fn row_schema_for_table<'tx>(
        &self,
        tx: &'tx MutTx,
        table_id: TableId,
    ) -> Result<RowTypeForTable<'tx>, DBError> {
        Ok(self.inner.row_type_for_table_mut_tx(tx, table_id)?)
    }

    pub fn get_all_tables_mut(&self, tx: &MutTx) -> Result<Vec<Arc<TableSchema>>, DBError> {
        Ok(self.inner.get_all_tables_mut_tx(tx)?)
    }

    pub fn get_all_tables(&self, tx: &Tx) -> Result<Vec<Arc<TableSchema>>, DBError> {
        Ok(self.inner.get_all_tables_tx(tx)?)
    }

    pub fn table_scheduled_id_and_at(
        &self,
        tx: &impl StateView,
        table_id: TableId,
    ) -> Result<Option<(ColId, ColId)>, DBError> {
        let schema = tx.schema_for_table(table_id)?;
        let Some(sched) = &schema.schedule else { return Ok(None) };
        let primary_key = schema
            .primary_key
            .context("scheduled table doesn't have a primary key?")?;
        Ok(Some((primary_key, sched.at_column)))
    }

    pub fn decode_column(
        &self,
        tx: &MutTx,
        table_id: TableId,
        col_id: ColId,
        bytes: &[u8],
    ) -> Result<AlgebraicValue, DBError> {
        // We need to do a manual bounds check here
        // since we want to do `swap_remove` to get an owned value
        // in the case of `Cow::Owned` and avoid a `clone`.
        let check_bounds = |schema: &ProductType| -> Result<_, DBError> {
            let col_idx = col_id.idx();
            if col_idx >= schema.elements.len() {
                return Err(DatastoreError::Table(TableError::ColumnNotFound(col_id)).into());
            }
            Ok(col_idx)
        };
        let row_ty = &*self.row_schema_for_table(tx, table_id)?;
        let col_idx = check_bounds(row_ty)?;
        let col_ty = &row_ty.elements[col_idx].algebraic_type;
        Ok(AlgebraicValue::decode(col_ty, &mut &*bytes)?)
    }

    /// Returns the execution counters for this database.
    pub fn exec_counter_map(&self) -> Arc<EnumMap<WorkloadType, ExecutionCounters>> {
        self.workload_type_to_exec_counters.clone()
    }

    /// Returns the execution counters for `workload_type` for this database.
    pub fn exec_counters_for(&self, workload_type: WorkloadType) -> &ExecutionCounters {
        &self.workload_type_to_exec_counters[workload_type]
    }

    /// Begin a transaction.
    ///
    /// **Note**: this call **must** be paired with [`Self::rollback_mut_tx`] or
    /// [`Self::commit_tx`], otherwise the database will be left in an invalid
    /// state. See also [`Self::with_auto_commit`].
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn begin_mut_tx(&self, isolation_level: IsolationLevel, workload: Workload) -> MutTx {
        log::trace!("BEGIN MUT TX");
        let r = self.inner.begin_mut_tx(isolation_level, workload);
        log::trace!("ACQUIRED MUT TX");
        r
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn begin_tx(&self, workload: Workload) -> Tx {
        log::trace!("BEGIN TX");
        let r = self.inner.begin_tx(workload);
        log::trace!("ACQUIRED TX");
        r
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn rollback_mut_tx(&self, tx: MutTx) -> (TxOffset, TxMetrics, String) {
        log::trace!("ROLLBACK MUT TX");
        self.inner.rollback_mut_tx(tx)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn rollback_mut_tx_downgrade(&self, tx: MutTx, workload: Workload) -> (TxMetrics, Tx) {
        log::trace!("ROLLBACK MUT TX");
        self.inner.rollback_mut_tx_downgrade(tx, workload)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn release_tx(&self, tx: Tx) -> (TxOffset, TxMetrics, String) {
        log::trace!("RELEASE TX");
        self.inner.release_tx(tx)
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn commit_tx(&self, tx: MutTx) -> Result<Option<(TxOffset, TxData, TxMetrics, String)>, DBError> {
        log::trace!("COMMIT MUT TX");

        // TODO: Never returns `None` -- should it?
        let reducer_context = tx.ctx.reducer_context().cloned();
        let Some((tx_offset, tx_data, tx_metrics, reducer)) = self.inner.commit_mut_tx(tx)? else {
            return Ok(None);
        };

        self.maybe_do_snapshot(&tx_data);

        if let Some(durability) = &self.durability {
            Self::do_durability(&**durability, reducer_context.as_ref(), &tx_data)
        }

        Ok(Some((tx_offset, tx_data, tx_metrics, reducer)))
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn commit_tx_downgrade(
        &self,
        tx: MutTx,
        workload: Workload,
    ) -> Result<Option<(TxData, TxMetrics, Tx)>, DBError> {
        log::trace!("COMMIT MUT TX");

        let Some((tx_data, tx_metrics, tx)) = self.inner.commit_mut_tx_downgrade(tx, workload)? else {
            return Ok(None);
        };

        self.maybe_do_snapshot(&tx_data);

        if let Some(durability) = &self.durability {
            Self::do_durability(&**durability, tx.ctx.reducer_context(), &tx_data)
        }

        Ok(Some((tx_data, tx_metrics, tx)))
    }

    /// If `(tx_data, ctx)` should be appended to the commitlog, do so.
    ///
    /// Note that by this stage,
    /// [`spacetimedb_datastore::locking_tx_datastore::committed_state::tx_consumes_offset`]
    /// has already decided based on the reducer and operations whether the transaction should be appended;
    /// this method is responsible only for reading its decision out of the `tx_data`
    /// and calling `durability.append_tx`.
    fn do_durability(durability: &Durability, reducer_context: Option<&ReducerContext>, tx_data: &TxData) {
        use commitlog::payload::{
            txdata::{Mutations, Ops},
            Txdata,
        };

        if tx_data.tx_offset().is_some() {
            let inserts: Box<_> = tx_data
                .inserts()
                .map(|(table_id, rowdata)| Ops {
                    table_id: *table_id,
                    rowdata: rowdata.clone(),
                })
                .collect();

            let truncates: IntSet<TableId> = tx_data.truncates().collect();

            let deletes: Box<_> = tx_data
                .deletes()
                .map(|(table_id, rowdata)| Ops {
                    table_id: *table_id,
                    rowdata: rowdata.clone(),
                })
                // filter out deletes for tables that are truncated in the same transaction.
                .filter(|ops| !truncates.contains(&ops.table_id))
                .collect();

            let inputs = reducer_context.map(|rcx| rcx.into());

            let txdata = Txdata {
                inputs,
                outputs: None,
                mutations: Some(Mutations {
                    inserts,
                    deletes,
                    truncates: truncates.into_iter().collect(),
                }),
            };

            // TODO: Should measure queuing time + actual write
            durability.append_tx(txdata);
        } else {
            debug_assert!(
                !tx_data.has_rows_or_connect_disconnect(reducer_context),
                "tx_data has no rows but has connect/disconnect: `{:?}`",
                reducer_context.map(|rcx| &rcx.name),
            );
        }
    }

    /// Get the [`DurableOffset`] of this database, or `None` if this is an
    /// in-memory instance.
    pub fn durable_tx_offset(&self) -> Option<DurableOffset> {
        self.durability
            .as_ref()
            .map(|durability| durability.durable_tx_offset())
    }

    /// Decide based on the `committed_state.next_tx_offset`
    /// whether to request that the [`SnapshotWorker`] in `self` capture a snapshot of the database.
    ///
    /// Actual snapshotting happens asynchronously in a Tokio worker.
    ///
    /// Snapshotting must happen independent of the durable TX offset known by the [`Durability`]
    /// because capturing a snapshot requires access to the committed state,
    /// which in the general case may advance beyond the durable TX offset,
    /// as our durability is an asynchronous write-behind log.
    /// An alternate implementation might keep a second materialized [`CommittedState`]
    /// which followed the durable TX offset rather than the committed-not-yet-durable state,
    /// in which case we would be able to snapshot only TXes known to be durable.
    /// In this implementation, we snapshot the existing [`CommittedState`]
    /// which stores the committed-not-yet-durable state.
    /// This requires a small amount of additional logic when restoring from a snapshot
    /// to ensure we don't restore a snapshot more recent than the durable TX offset.
    fn maybe_do_snapshot(&self, tx_data: &TxData) {
        if let Some(snapshot_worker) = &self.snapshot_worker {
            if let Some(tx_offset) = tx_data.tx_offset() {
                if tx_offset % SNAPSHOT_FREQUENCY == 0 {
                    snapshot_worker.request_snapshot();
                }
            }
        }
    }

    /// Subscribe to a channel of snapshot offsets.
    ///
    /// If a `snapshot_repo` was provided when this database was opened, this method
    /// returns a `watch::Receiver` that updates with the latest [`TxOffset`] a snapshot
    /// was taken at.
    pub fn subscribe_to_snapshots(&self) -> Option<watch::Receiver<TxOffset>> {
        self.snapshot_worker.as_ref().map(|snap| snap.subscribe())
    }

    /// Run a fallible function in a transaction.
    ///
    /// If the supplied function returns `Ok`, the transaction is automatically
    /// committed. Otherwise, the transaction is rolled back.
    ///
    /// This method is provided for convenience, as it allows to safely use the
    /// `?` operator in code running within a transaction context. Recall that a
    /// [`MutTx`] does not follow the RAII pattern, so the following code is
    /// wrong:
    ///
    /// ```ignore
    /// let tx = db.begin_mut_tx(IsolationLevel::Serializable);
    /// let _ = db.schema_for_table(tx, 42)?;
    /// // ...
    /// let _ = db.commit_tx(tx)?;
    /// ```
    ///
    /// If `schema_for_table` returns an error, the transaction is not properly
    /// cleaned up, as the `?` short-circuits. To avoid this, but still be able
    /// to use `?`, you can write:
    ///
    /// ```ignore
    /// db.with_auto_commit(|tx| {
    ///     let _ = db.schema_for_table(tx, 42)?;
    ///     // ...
    ///     Ok(())
    /// })?;
    /// ```
    pub fn with_auto_commit<F, A, E>(&self, workload: Workload, f: F) -> Result<A, E>
    where
        F: FnOnce(&mut MutTx) -> Result<A, E>,
        E: From<DBError>,
    {
        let mut tx = self.begin_mut_tx(IsolationLevel::Serializable, workload);
        let res = f(&mut tx);
        self.finish_tx(tx, res)
    }

    /// Run a fallible function in a transaction, rolling it back if the
    /// function returns `Err`.
    ///
    /// Similar in purpose to [`Self::with_auto_commit`], but returns the
    /// [`MutTx`] alongside the `Ok` result of the function `F` without
    /// committing the transaction.
    pub fn with_auto_rollback<F, A, E>(&self, mut tx: MutTx, f: F) -> Result<(MutTx, A), E>
    where
        F: FnOnce(&mut MutTx) -> Result<A, E>,
    {
        let res = f(&mut tx);
        self.rollback_on_err(tx, res)
    }

    /// Run a fallible function in a transaction.
    ///
    /// This is similar to `with_auto_commit`, but regardless of the return value of
    /// the fallible function, the transaction will ALWAYS be rolled back. This can be used to
    /// emulate a read-only transaction.
    ///
    /// TODO(jgilles): when we support actual read-only transactions, use those here instead.
    /// TODO(jgilles, kim): get this merged with the above function (two people had similar ideas
    /// at the same time)
    pub fn with_read_only<F, T>(&self, workload: Workload, f: F) -> T
    where
        F: FnOnce(&mut Tx) -> T,
    {
        let mut tx = self.begin_tx(workload);
        let res = f(&mut tx);
        let (_tx_offset, tx_metrics, reducer) = self.release_tx(tx);
        self.report_read_tx_metrics(reducer, tx_metrics);
        res
    }

    /// Perform the transactional logic for the `tx` according to the `res`
    pub fn finish_tx<A, E>(&self, tx: MutTx, res: Result<A, E>) -> Result<A, E>
    where
        E: From<DBError>,
    {
        if res.is_err() {
            let (_, tx_metrics, reducer) = self.rollback_mut_tx(tx);
            self.report_mut_tx_metrics(reducer, tx_metrics, None);
        } else {
            match self.commit_tx(tx).map_err(E::from)? {
                Some((_tx_offset, tx_data, tx_metrics, reducer)) => {
                    self.report_mut_tx_metrics(reducer, tx_metrics, Some(tx_data));
                }
                None => panic!("TODO: retry?"),
            }
        }

        res
    }

    /// Roll back transaction `tx` if `res` is `Err`, otherwise return it
    /// alongside the `Ok` value.
    pub fn rollback_on_err<A, E>(&self, tx: MutTx, res: Result<A, E>) -> Result<(MutTx, A), E> {
        match res {
            Err(e) => {
                let (_, tx_metrics, reducer) = self.rollback_mut_tx(tx);
                self.report_mut_tx_metrics(reducer, tx_metrics, None);

                Err(e)
            }
            Ok(a) => Ok((tx, a)),
        }
    }

    pub(crate) fn alter_table_access(&self, tx: &mut MutTx, name: &str, access: StAccess) -> Result<(), DBError> {
        Ok(self.inner.alter_table_access_mut_tx(tx, name, access)?)
    }

    pub(crate) fn alter_table_row_type(
        &self,
        tx: &mut MutTx,
        table_id: TableId,
        column_schemas: Vec<ColumnSchema>,
    ) -> Result<(), DBError> {
        Ok(self.inner.alter_table_row_type_mut_tx(tx, table_id, column_schemas)?)
    }

    pub(crate) fn add_columns_to_table(
        &self,
        tx: &mut MutTx,
        table_id: TableId,
        column_schemas: Vec<ColumnSchema>,
        default_values: Vec<AlgebraicValue>,
    ) -> Result<TableId, DBError> {
        Ok(self
            .inner
            .add_columns_to_table_mut_tx(tx, table_id, column_schemas, default_values)?)
    }

    /// Reports the `TxMetrics`s passed.
    ///
    /// Should only be called after the tx lock has been fully released.
    pub(crate) fn report_tx_metrics(
        &self,
        reducer: String,
        tx_data: Option<Arc<TxData>>,
        metrics_for_writer: Option<TxMetrics>,
        metrics_for_reader: Option<TxMetrics>,
    ) {
        if let Some(recorder) = &self.metrics_recorder_queue {
            recorder.send_metrics(
                reducer,
                metrics_for_writer,
                metrics_for_reader,
                tx_data,
                self.exec_counter_map(),
            );
        }
    }
}

impl RelationalDB {
    pub fn create_table(&self, tx: &mut MutTx, schema: TableSchema) -> Result<TableId, DBError> {
        Ok(self.inner.create_table_mut_tx(tx, schema)?)
    }

    pub fn drop_table(&self, tx: &mut MutTx, table_id: TableId) -> Result<(), DBError> {
        let table_name = self
            .table_name_from_id_mut(tx, table_id)?
            .map(|name| name.to_string())
            .unwrap_or_default();
        Ok(self.inner.drop_table_mut_tx(tx, table_id).map(|_| {
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&self.database_identity, &table_id.into(), &table_name)
                .set(0)
        })?)
    }

    pub fn create_view(
        &self,
        tx: &mut MutTx,
        module_def: &ModuleDef,
        view_def: &ViewDef,
    ) -> Result<(ViewId, TableId), DBError> {
        Ok(tx.create_view(module_def, view_def)?)
    }

    pub fn drop_view(&self, tx: &mut MutTx, view_id: ViewId) -> Result<(), DBError> {
        Ok(tx.drop_view(view_id)?)
    }

    pub fn create_table_for_test_with_the_works(
        &self,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        indexes: &[ColList],
        unique_constraints: &[ColList],
        access: StAccess,
    ) -> Result<TableId, DBError> {
        let mut module_def_builder = RawModuleDefV9Builder::new();

        let mut table_builder = module_def_builder
            .build_table_with_new_type_for_tests(name, ProductType::from_iter(schema.iter().cloned()), true)
            .with_access(access.into());

        for columns in indexes {
            table_builder = table_builder.with_index(btree(columns.clone()), "accessor_name_doesnt_matter");
        }
        for columns in unique_constraints {
            table_builder = table_builder.with_unique_constraint(columns.clone());
        }
        table_builder.finish();
        let module_def: ModuleDef = module_def_builder.finish().try_into()?;

        let table: &TableDef = module_def.table(name).expect("table not found");

        // Recursively sets all IDs to `SENTINEL`.
        let schema = TableSchema::from_module_def(&module_def, table, (), TableId::SENTINEL);

        //TODO: Change this to `Workload::ForTest` once `#[cfg(bench)]` is stabilized.
        self.with_auto_commit(Workload::Internal, |tx| self.create_table(tx, schema))
    }

    pub fn create_table_for_test_with_access(
        &self,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        indexes: &[ColId],
        access: StAccess,
    ) -> Result<TableId, DBError> {
        let indexes: Vec<ColList> = indexes.iter().map(|col_id| (*col_id).into()).collect();
        self.create_table_for_test_with_the_works(name, schema, &indexes[..], &[], access)
    }

    pub fn create_table_for_test(
        &self,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        indexes: &[ColId],
    ) -> Result<TableId, DBError> {
        self.create_table_for_test_with_access(name, schema, indexes, StAccess::Public)
    }

    pub fn create_table_for_test_multi_column(
        &self,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        idx_cols: ColList,
    ) -> Result<TableId, DBError> {
        self.create_table_for_test_with_the_works(name, schema, &[idx_cols], &[], StAccess::Public)
    }

    pub fn create_table_for_test_mix_indexes(
        &self,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        idx_cols_single: &[ColId],
        idx_cols_multi: ColList,
    ) -> Result<TableId, DBError> {
        let indexes: Vec<ColList> = idx_cols_single
            .iter()
            .map(|col_id| (*col_id).into())
            .chain(std::iter::once(idx_cols_multi))
            .collect();

        self.create_table_for_test_with_the_works(name, schema, &indexes[..], &[], StAccess::Public)
    }

    /// Rename a table.
    ///
    /// Sets the name of the table to `new_name` regardless of the previous value. This is a
    /// relatively cheap operation which only modifies the system tables.
    ///
    /// If the table is not found or is a system table, an error is returned.
    pub fn rename_table(&self, tx: &mut MutTx, table_id: TableId, new_name: &str) -> Result<(), DBError> {
        Ok(self.inner.rename_table_mut_tx(tx, table_id, new_name)?)
    }

    pub fn view_id_from_name_mut(&self, tx: &MutTx, view_name: &str) -> Result<Option<ViewId>, DBError> {
        Ok(self.inner.view_id_from_name_mut_tx(tx, view_name)?)
    }

    pub fn table_id_from_name_mut(&self, tx: &MutTx, table_name: &str) -> Result<Option<TableId>, DBError> {
        Ok(self.inner.table_id_from_name_mut_tx(tx, table_name)?)
    }

    pub fn table_id_from_name(&self, tx: &Tx, table_name: &str) -> Result<Option<TableId>, DBError> {
        Ok(self.inner.table_id_from_name_tx(tx, table_name)?)
    }

    pub fn table_id_exists(&self, tx: &Tx, table_id: &TableId) -> bool {
        self.inner.table_id_exists_tx(tx, table_id)
    }

    pub fn table_id_exists_mut(&self, tx: &MutTx, table_id: &TableId) -> bool {
        self.inner.table_id_exists_mut_tx(tx, table_id)
    }

    pub fn table_name_from_id<'a>(&'a self, tx: &'a Tx, table_id: TableId) -> Result<Option<Cow<'a, str>>, DBError> {
        Ok(self.inner.table_name_from_id_tx(tx, table_id)?)
    }

    pub fn table_name_from_id_mut<'a>(
        &'a self,
        tx: &'a MutTx,
        table_id: TableId,
    ) -> Result<Option<Cow<'a, str>>, DBError> {
        Ok(self.inner.table_name_from_id_mut_tx(tx, table_id)?)
    }

    pub fn index_id_from_name_mut(&self, tx: &MutTx, index_name: &str) -> Result<Option<IndexId>, DBError> {
        Ok(self.inner.index_id_from_name_mut_tx(tx, index_name)?)
    }

    pub fn table_row_count_mut(&self, tx: &MutTx, table_id: TableId) -> Option<u64> {
        // TODO(Centril): Go via MutTxDatastore trait instead.
        // Doing this for now to ship this quicker.
        tx.table_row_count(table_id)
    }

    /// Returns the constraints on the input `ColList`.
    /// Note that this is ORDER-SENSITIVE: the order of the columns in the input `ColList` matters.
    pub fn column_constraints(
        &self,
        tx: &mut MutTx,
        table_id: TableId,
        cols: &ColList,
    ) -> Result<Constraints, DBError> {
        let table = self.inner.schema_for_table_mut_tx(tx, table_id)?;

        let index = table.indexes.iter().find(|i| i.index_algorithm.columns() == *cols);
        let cols_set = ColSet::from(cols);
        let unique_constraint = table
            .constraints
            .iter()
            .find(|c| c.data.unique_columns() == Some(&cols_set));

        if index.is_some() {
            Ok(Constraints::from_is_unique(unique_constraint.is_some()))
        } else if unique_constraint.is_some() {
            Ok(Constraints::unique())
        } else {
            Ok(Constraints::unset())
        }
    }

    pub fn index_id_from_name(&self, tx: &MutTx, index_name: &str) -> Result<Option<IndexId>, DBError> {
        Ok(self.inner.index_id_from_name_mut_tx(tx, index_name)?)
    }

    pub fn sequence_id_from_name(&self, tx: &MutTx, sequence_name: &str) -> Result<Option<SequenceId>, DBError> {
        Ok(self.inner.sequence_id_from_name_mut_tx(tx, sequence_name)?)
    }

    pub fn constraint_id_from_name(&self, tx: &MutTx, constraint_name: &str) -> Result<Option<ConstraintId>, DBError> {
        Ok(self.inner.constraint_id_from_name(tx, constraint_name)?)
    }

    /// Adds the index into the [ST_INDEXES_NAME] table
    ///
    /// NOTE: It loads the data from the table into it before returning
    pub fn create_index(&self, tx: &mut MutTx, schema: IndexSchema, is_unique: bool) -> Result<IndexId, DBError> {
        Ok(self.inner.create_index_mut_tx(tx, schema, is_unique)?)
    }

    /// Removes the [`TableIndex`] from the database by their `index_id`
    pub fn drop_index(&self, tx: &mut MutTx, index_id: IndexId) -> Result<(), DBError> {
        Ok(self.inner.drop_index_mut_tx(tx, index_id)?)
    }

    pub fn create_row_level_security(
        &self,
        tx: &mut MutTx,
        row_level_security_schema: RowLevelSecuritySchema,
    ) -> Result<RawSql, DBError> {
        Ok(tx.create_row_level_security(row_level_security_schema)?)
    }

    pub fn drop_row_level_security(&self, tx: &mut MutTx, sql: RawSql) -> Result<(), DBError> {
        Ok(tx.drop_row_level_security(sql)?)
    }

    pub fn row_level_security_for_table_id_mut_tx(
        &self,
        tx: &mut MutTx,
        table_id: TableId,
    ) -> Result<Vec<RowLevelSecuritySchema>, DBError> {
        Ok(tx.row_level_security_for_table_id(table_id)?)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`.
    pub fn iter_mut<'a>(&'a self, tx: &'a MutTx, table_id: TableId) -> Result<IterMutTx<'a>, DBError> {
        Ok(self.inner.iter_mut_tx(tx, table_id)?)
    }

    pub fn iter<'a>(&'a self, tx: &'a Tx, table_id: TableId) -> Result<IterTx<'a>, DBError> {
        Ok(self.inner.iter_tx(tx, table_id)?)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `cols` matches `value`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn iter_by_col_eq_mut<'a, 'r>(
        &'a self,
        tx: &'a MutTx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<IterByColEqMutTx<'a, 'r>, DBError> {
        Ok(self.inner.iter_by_col_eq_mut_tx(tx, table_id.into(), cols, value)?)
    }

    pub fn iter_by_col_eq<'a, 'r>(
        &'a self,
        tx: &'a Tx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<IterByColEqTx<'a, 'r>, DBError> {
        Ok(self.inner.iter_by_col_eq_tx(tx, table_id.into(), cols, value)?)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `cols` matches what is within `range`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn iter_by_col_range_mut<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        tx: &'a MutTx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<IterByColRangeMutTx<'a, R>, DBError> {
        Ok(self.inner.iter_by_col_range_mut_tx(tx, table_id.into(), cols, range)?)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `cols` matches what is within `range`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        tx: &'a Tx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<IterByColRangeTx<'a, R>, DBError> {
        Ok(self.inner.iter_by_col_range_tx(tx, table_id.into(), cols, range)?)
    }

    pub fn index_scan_range<'a>(
        &'a self,
        tx: &'a MutTx,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<
        (
            TableId,
            Bound<AlgebraicValue>,
            Bound<AlgebraicValue>,
            impl Iterator<Item = RowRef<'a>>,
        ),
        DBError,
    > {
        Ok(tx.index_scan_range(index_id, prefix, prefix_elems, rstart, rend)?)
    }

    pub fn insert<'a>(
        &'a self,
        tx: &'a mut MutTx,
        table_id: TableId,
        row: &[u8],
    ) -> Result<(ColList, RowRef<'a>, InsertFlags), DBError> {
        Ok(self.inner.insert_mut_tx(tx, table_id, row)?)
    }

    pub fn update<'a>(
        &'a self,
        tx: &'a mut MutTx,
        table_id: TableId,
        index_id: IndexId,
        row: &[u8],
    ) -> Result<(ColList, RowRef<'a>, UpdateFlags), DBError> {
        Ok(self.inner.update_mut_tx(tx, table_id, index_id, row)?)
    }

    pub fn delete(&self, tx: &mut MutTx, table_id: TableId, row_ids: impl IntoIterator<Item = RowPointer>) -> u32 {
        self.inner.delete_mut_tx(tx, table_id, row_ids)
    }

    pub fn delete_by_rel<R: IntoIterator<Item = ProductValue>>(
        &self,
        tx: &mut MutTx,
        table_id: TableId,
        relation: R,
    ) -> u32 {
        self.inner.delete_by_rel_mut_tx(tx, table_id, relation)
    }

    /// Clear all rows from a table without dropping it.
    pub fn clear_table(&self, tx: &mut MutTx, table_id: TableId) -> Result<usize, DBError> {
        let rows_deleted = tx.clear_table(table_id)?;
        Ok(rows_deleted)
    }

    pub fn create_sequence(&self, tx: &mut MutTx, sequence_schema: SequenceSchema) -> Result<SequenceId, DBError> {
        Ok(self.inner.create_sequence_mut_tx(tx, sequence_schema)?)
    }

    ///Removes the [Sequence] from database instance
    pub fn drop_sequence(&self, tx: &mut MutTx, seq_id: SequenceId) -> Result<(), DBError> {
        Ok(self.inner.drop_sequence_mut_tx(tx, seq_id)?)
    }

    ///Removes the [Constraints] from database instance
    pub fn drop_constraint(&self, tx: &mut MutTx, constraint_id: ConstraintId) -> Result<(), DBError> {
        Ok(self.inner.drop_constraint_mut_tx(tx, constraint_id)?)
    }

    /// Reports the metrics for `reducer`, using counters provided by `db`.
    pub fn report_mut_tx_metrics(&self, reducer: String, metrics: TxMetrics, tx_data: Option<TxData>) {
        self.report_tx_metrics(reducer, tx_data.map(Arc::new), Some(metrics), None);
    }

    /// Reports subscription metrics for `reducer`, using counters provided by `db`.
    pub fn report_read_tx_metrics(&self, reducer: String, metrics: TxMetrics) {
        self.report_tx_metrics(reducer, None, None, Some(metrics));
    }

    /// Read the value of [ST_VARNAME_ROW_LIMIT] from `st_var`
    pub(crate) fn row_limit(&self, tx: &Tx) -> Result<Option<u64>, DBError> {
        let data = self.read_var(tx, StVarName::RowLimit);

        if let Some(StVarValue::U64(limit)) = data? {
            return Ok(Some(limit));
        }
        Ok(None)
    }

    /// Read the value of [ST_VARNAME_SLOW_QRY] from `st_var`
    pub(crate) fn query_limit(&self, tx: &Tx) -> Result<Option<u64>, DBError> {
        if let Some(StVarValue::U64(ms)) = self.read_var(tx, StVarName::SlowQryThreshold)? {
            return Ok(Some(ms));
        }
        Ok(None)
    }

    /// Read the value of [ST_VARNAME_SLOW_SUB] from `st_var`
    #[allow(dead_code)]
    pub(crate) fn sub_limit(&self, tx: &Tx) -> Result<Option<u64>, DBError> {
        if let Some(StVarValue::U64(ms)) = self.read_var(tx, StVarName::SlowSubThreshold)? {
            return Ok(Some(ms));
        }
        Ok(None)
    }

    /// Read the value of [ST_VARNAME_SLOW_INC] from `st_var`
    #[allow(dead_code)]
    pub(crate) fn incr_limit(&self, tx: &Tx) -> Result<Option<u64>, DBError> {
        if let Some(StVarValue::U64(ms)) = self.read_var(tx, StVarName::SlowIncThreshold)? {
            return Ok(Some(ms));
        }
        Ok(None)
    }

    /// Read the value of a system variable from `st_var`
    pub(crate) fn read_var(&self, tx: &Tx, name: StVarName) -> Result<Option<StVarValue>, DBError> {
        if let Some(row_ref) = self
            .iter_by_col_eq(tx, ST_VAR_ID, StVarFields::Name.col_id(), &name.into())?
            .next()
        {
            return Ok(Some(StVarRow::try_from(row_ref)?.value));
        }
        Ok(None)
    }

    /// Update the value of a system variable in `st_var`
    pub(crate) fn write_var(&self, tx: &mut MutTx, name: StVarName, literal: &str) -> Result<(), DBError> {
        let value = Self::parse_var(name, literal)?;
        if let Some(row_ref) = self
            .iter_by_col_eq_mut(tx, ST_VAR_ID, StVarFields::Name.col_id(), &name.into())?
            .next()
        {
            self.delete(tx, ST_VAR_ID, [row_ref.pointer()]);
        }
        tx.insert_via_serialize_bsatn(ST_VAR_ID, &StVarRow { name, value })?;
        Ok(())
    }

    /// Parse the literal representation of a system variable
    fn parse_var(name: StVarName, literal: &str) -> Result<StVarValue, DBError> {
        StVarValue::try_from_primitive(parse::parse(literal, &name.type_of())?).map_err(|v| {
            ErrorVm::Type(ErrorType::Parse {
                value: literal.to_string(),
                ty: fmt_algebraic_type(&name.type_of()).to_string(),
                err: format!("error parsing value: {v:?}"),
            })
            .into()
        })
    }
}

#[allow(unused)]
#[derive(Clone)]
struct LockFile {
    path: Arc<Path>,
    lock: Arc<File>,
}

impl LockFile {
    pub fn lock(root: &ReplicaDir) -> Result<Self, DBError> {
        root.create()?;
        let path = root.0.join("db.lock");
        let lock = File::create(&path)?;
        lock.try_lock_exclusive()
            .map_err(|e| DatabaseError::DatabasedOpened(root.0.clone(), e.into()))?;

        Ok(Self {
            path: path.into(),
            lock: lock.into(),
        })
    }
}

impl fmt::Debug for LockFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LockFile").field("path", &self.path).finish()
    }
}

fn apply_history<H>(datastore: &Locking, database_identity: Identity, history: H) -> Result<(), DBError>
where
    H: durability::History<TxData = Txdata>,
{
    log::info!("[{database_identity}] DATABASE: applying transaction history...");

    // TODO: Revisit once we actually replay history suffixes, ie. starting
    // from an offset larger than the history's min offset.
    // TODO: We may want to require that a `tokio::runtime::Handle` is
    // always supplied when constructing a `RelationalDB`. This would allow
    // to spawn a timer task here which just prints the progress periodically
    // in case the history is finite but very long.
    let (_, max_tx_offset) = history.tx_range_hint();
    let mut last_logged_percentage = 0;
    let progress = |tx_offset: u64| {
        if let Some(max_tx_offset) = max_tx_offset {
            let percentage = f64::floor((tx_offset as f64 / max_tx_offset as f64) * 100.0) as i32;
            if percentage > last_logged_percentage && percentage % 10 == 0 {
                log::info!("[{database_identity}] Loaded {percentage}% ({tx_offset}/{max_tx_offset})");
                last_logged_percentage = percentage;
            }
        // Print _something_ even if we don't know what's still ahead.
        } else if tx_offset.is_multiple_of(10_000) {
            log::info!("[{database_identity}] Loading transaction {tx_offset}");
        }
    };

    let time_before = std::time::Instant::now();

    let mut replay = datastore.replay(progress);
    let start_tx_offset = replay.next_tx_offset();
    history
        .fold_transactions_from(start_tx_offset, &mut replay)
        .map_err(anyhow::Error::from)?;

    let time_elapsed = time_before.elapsed();
    WORKER_METRICS
        .replay_commitlog_time_seconds
        .with_label_values(&database_identity)
        .set(time_elapsed.as_secs_f64());

    let end_tx_offset = replay.next_tx_offset();
    WORKER_METRICS
        .replay_commitlog_num_commits
        .with_label_values(&database_identity)
        .set((end_tx_offset - start_tx_offset) as _);

    log::info!("[{database_identity}] DATABASE: applied transaction history");
    datastore.rebuild_state_after_replay()?;
    log::info!("[{database_identity}] DATABASE: rebuilt state after replay");

    Ok(())
}

pub type LocalDurability = Arc<durability::Local<ProductValue>>;
/// Initialize local durability with the default parameters.
///
/// Also returned is a [`DiskSizeFn`] as required by [`RelationalDB::open`].
///
/// Note that this operation can be expensive, as it needs to traverse a suffix
/// of the commitlog.
pub async fn local_durability(
    commitlog_dir: CommitLogDir,
    snapshot_worker: Option<&SnapshotWorker>,
) -> io::Result<(LocalDurability, DiskSizeFn)> {
    let rt = tokio::runtime::Handle::current();
    // TODO: Should this better be spawn_blocking?
    let on_new_segment = snapshot_worker.map(|snapshot_worker| {
        let snapshot_worker = snapshot_worker.clone();
        Arc::new(move || {
            // Ignore errors: we don't want our durability to die and start throwing away queued TXes
            // just because the snapshot worker shut down.
            snapshot_worker.request_snapshot_ignore_closed();
        }) as Arc<OnNewSegmentFn>
    });
    let local = spawn_rayon(move || {
        durability::Local::open(
            commitlog_dir,
            rt,
            durability::local::Options {
                commitlog: commitlog::Options {
                    max_records_in_commit: 1.try_into().unwrap(),
                    ..Default::default()
                },
                ..Default::default()
            },
            // Give the durability a handle to request a new snapshot run,
            // which it will send down whenever we rotate commitlog segments.
            on_new_segment,
        )
    })
    .await
    .map(Arc::new)?;
    let disk_size_fn = Arc::new({
        let durability = local.clone();
        move || durability.size_on_disk()
    });

    Ok((local, disk_size_fn))
}

/// Watches snapshot creation events and compresses all commitlog segments older
/// than the snapshot.
///
/// Suitable **only** for non-replicated databases.
pub async fn snapshot_watching_commitlog_compressor(
    mut snapshot_rx: watch::Receiver<u64>,
    mut clog_tx: Option<tokio::sync::mpsc::Sender<u64>>,
    mut snap_tx: Option<tokio::sync::mpsc::Sender<u64>>,
    durability: LocalDurability,
) {
    let mut prev_snapshot_offset = *snapshot_rx.borrow_and_update();
    while snapshot_rx.changed().await.is_ok() {
        let snapshot_offset = *snapshot_rx.borrow_and_update();
        let durability = durability.clone();

        if let Some(snap_tx) = &mut snap_tx {
            if let Err(err) = snap_tx.try_send(snapshot_offset) {
                tracing::warn!("failed to send offset {snapshot_offset} after snapshot creation: {err}");
            }
        }

        let res: io::Result<_> = asyncify(move || {
            let segment_offsets = durability.existing_segment_offsets()?;
            let start_idx = segment_offsets
                .binary_search(&prev_snapshot_offset)
                // if the snapshot is in the middle of a segment, we want to round down.
                // [0, 2].binary_search(1) will return Err(1), so we subtract 1.
                .unwrap_or_else(|i| i.saturating_sub(1));
            let segment_offsets = &segment_offsets[start_idx..];
            let end_idx = segment_offsets
                .binary_search(&snapshot_offset)
                .unwrap_or_else(|i| i.saturating_sub(1));
            // in this case, segment_offsets[end_idx] is the segment that contains the snapshot,
            // which we don't want to compress, so an exclusive range is correct.
            let segment_offsets = &segment_offsets[..end_idx];
            durability.compress_segments(segment_offsets)?;
            let n = segment_offsets.len();
            let last_compressed_segment = if n > 0 { Some(segment_offsets[n - 1]) } else { None };
            Ok(last_compressed_segment)
        })
        .await;

        let last_compressed_segment = match res {
            Ok(opt_offset) => opt_offset,
            Err(err) => {
                tracing::warn!("failed to compress segments: {err}");
                continue;
            }
        };
        prev_snapshot_offset = snapshot_offset;

        if let Some((clog_tx, last_compressed_segment)) = clog_tx.as_mut().zip(last_compressed_segment) {
            if let Err(err) = clog_tx.try_send(last_compressed_segment) {
                tracing::warn!("failed to send offset {last_compressed_segment} after compression: {err}");
            }
        }
    }
}

/// Open a [`SnapshotRepository`] at `db_path/snapshots`,
/// configured to store snapshots of the database `database_identity`/`replica_id`.
pub fn open_snapshot_repo(
    path: SnapshotsPath,
    database_identity: Identity,
    replica_id: u64,
) -> Result<Arc<SnapshotRepository>, Box<SnapshotError>> {
    path.create().map_err(SnapshotError::from)?;
    SnapshotRepository::open(path, database_identity, replica_id)
        .map(Arc::new)
        .map_err(Box::new)
}

fn default_row_count_fn(db: Identity) -> RowCountFn {
    Arc::new(move |table_id, table_name| {
        DB_METRICS
            .rdb_num_table_rows
            .with_label_values(&db, &table_id.into(), table_name)
            .get()
    })
}

#[cfg(any(test, feature = "test"))]
pub mod tests_utils {
    use crate::db::snapshot;
    use crate::db::snapshot::SnapshotWorker;

    use super::*;
    use core::ops::Deref;
    use durability::EmptyHistory;
    use spacetimedb_datastore::locking_tx_datastore::MutTxId;
    use spacetimedb_datastore::locking_tx_datastore::TxId;
    use spacetimedb_fs_utils::compression::CompressType;
    use spacetimedb_lib::{bsatn::to_vec, ser::Serialize};
    use spacetimedb_paths::server::SnapshotDirPath;
    use spacetimedb_paths::FromPathUnchecked;
    use tempfile::TempDir;

    pub enum TestDBDir {
        /// A directory that deletes itself when dropped.
        Temp(TempReplicaDir),
        /// A directory that will not delete itself when dropped.
        Persistent(ReplicaDir),
    }
    impl Deref for TestDBDir {
        type Target = ReplicaDir;
        fn deref(&self) -> &Self::Target {
            match self {
                TestDBDir::Temp(dir) => &dir.0,
                TestDBDir::Persistent(dir) => dir,
            }
        }
    }

    impl From<TempReplicaDir> for TestDBDir {
        fn from(dir: TempReplicaDir) -> Self {
            Self::Temp(dir)
        }
    }

    /// A [`RelationalDB`] in a temporary directory.
    ///
    /// When dropped, any resources including the temporary directory will be
    /// removed.
    ///
    /// To ensure all data is flushed to disk when using the durable variant
    /// constructed via [`Self::durable`], [`Self::close`] or [`Self::reopen`]
    /// must be used.
    ///
    /// To keep the temporary directory, use [`Self::reopen`] or [`Self::into_parts`].
    ///
    /// [`TestDB`] is deref-coercible into [`RelationalDB`], which is dubious
    /// but convenient.
    pub struct TestDB {
        pub db: RelationalDB,

        // nb: drop order is declaration order
        durable: Option<DurableState>,
        dir: TestDBDir,

        /// Whether to construct a snapshot repository when restarting with [`Self::reopen`].
        want_snapshot_repo: bool,
    }

    pub struct TempReplicaDir(ReplicaDir);
    impl TempReplicaDir {
        pub fn new() -> io::Result<Self> {
            let dir = TempDir::with_prefix("stdb_test")?;
            Ok(Self(ReplicaDir::from_path_unchecked(dir.keep())))
        }
    }
    impl Deref for TempReplicaDir {
        type Target = ReplicaDir;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl Drop for TempReplicaDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct DurableState {
        handle: Arc<durability::Local<ProductValue>>,
        rt: tokio::runtime::Runtime,
    }

    impl TestDB {
        pub const DATABASE_IDENTITY: Identity = Identity::ZERO;
        // pub const DATABASE_IDENTITY: Identity = Identity::ZERO;
        pub const OWNER: Identity = Identity::ZERO;

        /// Create a [`TestDB`] which does not store data on disk.
        pub fn in_memory() -> Result<Self, DBError> {
            let dir = TempReplicaDir::new()?;
            let db = Self::in_memory_internal(&dir)?;
            Ok(Self {
                db,

                durable: None,
                dir: dir.into(),
                want_snapshot_repo: false,
            })
        }

        /// Create a [`TestDB`] which stores data in a local commitlog.
        ///
        /// Note that flushing the log is an asynchronous process. [`Self::reopen`]
        /// ensures all data has been flushed to disk before re-opening the
        /// database.
        pub fn durable() -> Result<Self, DBError> {
            let dir = TempReplicaDir::new()?;
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
            // Enter the runtime so that `Self::durable_internal` can spawn a `SnapshotWorker`.
            let _rt = rt.enter();
            let (db, handle) = Self::durable_internal(&dir, rt.handle().clone(), true)?;
            let durable = DurableState { handle, rt };

            Ok(Self {
                db,
                durable: Some(durable),
                dir: dir.into(),
                want_snapshot_repo: true,
            })
        }

        pub fn durable_without_snapshot_repo() -> Result<Self, DBError> {
            let dir = TempReplicaDir::new()?;
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
            // Enter the runtime so that `Self::durable_internal` can spawn a `SnapshotWorker`.
            let _rt = rt.enter();
            let (db, handle) = Self::durable_internal(&dir, rt.handle().clone(), false)?;
            let durable = DurableState { handle, rt };

            Ok(Self {
                db,
                durable: Some(durable),
                dir: dir.into(),
                want_snapshot_repo: false,
            })
        }

        pub fn open_existing_durable(
            root: &ReplicaDir,
            rt: tokio::runtime::Handle,
            replica_id: u64,
            db_identity: Identity,
            owner_identity: Identity,
            want_snapshot_repo: bool,
        ) -> Result<(RelationalDB, Arc<durability::Local<ProductValue>>), DBError> {
            let snapshots = want_snapshot_repo
                .then(|| {
                    open_snapshot_repo(root.snapshots(), db_identity, replica_id)
                        .map(|repo| SnapshotWorker::new(repo, snapshot::Compression::Disabled))
                })
                .transpose()?;

            let (local, disk_size_fn) = rt.block_on(local_durability(root.commit_log(), snapshots.as_ref()))?;
            let history = local.clone();

            let persistence = Persistence {
                durability: local.clone(),
                disk_size: disk_size_fn,
                snapshots,
            };

            let (db, _) = RelationalDB::open(
                root,
                db_identity,
                owner_identity,
                history,
                Some(persistence),
                None,
                PagePool::new_for_test(),
            )?;
            let db = db.with_row_count(Self::row_count_fn());
            Ok((db, local))
        }
        /// Create a [`TestDB`] which stores data in a local commitlog,
        /// initialized with pre-existing data from `history`.
        ///
        /// [`TestHistory::from_txes`] is an easy-ish way to construct a non-empty [`History`].
        ///
        /// `expected_num_clients` is the expected size of the `connected_clients` return
        /// from [`RelationalDB::open`] after replaying `history`.
        /// Opening with an empty history, or one that does not insert into `st_client`,
        /// should result in this number being 0.
        pub fn in_memory_with_history(
            history: impl durability::History<TxData = Txdata>,
            expected_num_clients: usize,
        ) -> Result<Self, DBError> {
            let dir = TempReplicaDir::new()?;
            let db = Self::open_db(&dir, history, None, None, expected_num_clients)?;
            Ok(Self {
                db,
                durable: None,
                dir: dir.into(),
                want_snapshot_repo: false,
            })
        }

        /// Re-open the database, after ensuring that all data has been flushed
        /// to disk (if the database was created via [`Self::durable`]).
        pub fn reopen(self) -> Result<Self, DBError> {
            drop(self.db);

            if let Some(DurableState { handle, rt }) = self.durable {
                let handle =
                    Arc::into_inner(handle).expect("`drop(self.db)` should have dropped all references to durability");
                rt.block_on(handle.close())?;

                // Enter the runtime so that `Self::durable_internal` can spawn a `SnapshotWorker`.
                let _rt = rt.enter();
                let (db, handle) = Self::durable_internal(&self.dir, rt.handle().clone(), self.want_snapshot_repo)?;
                let durable = DurableState { handle, rt };

                Ok(Self {
                    db,
                    durable: Some(durable),
                    ..self
                })
            } else {
                let db = Self::in_memory_internal(&self.dir)?;
                Ok(Self { db, ..self })
            }
        }

        /// Close the database, flushing outstanding data to disk (if the
        /// database was created via [`Self::durable`].
        ///
        /// Note that the data is no longer accessible once this method returns,
        /// because the temporary directory has been dropped. The method is
        /// provided mainly for cases where measuring the flush overhead is
        /// desired.
        pub fn close(self) -> Result<(), DBError> {
            drop(self.db);
            if let Some(DurableState { handle, rt }) = self.durable {
                let handle =
                    Arc::into_inner(handle).expect("`drop(self.db)` should have dropped all references to durability");
                rt.block_on(handle.close())?;
            }

            Ok(())
        }

        pub fn with_row_count(self, row_count: RowCountFn) -> Self {
            Self {
                db: self.db.with_row_count(row_count),
                ..self
            }
        }

        /// The root path of the (temporary) database directory.
        pub fn path(&self) -> &ReplicaDir {
            &self.dir
        }

        /// Handle to the tokio runtime, available if [`Self::durable`] was used
        /// to create the [`TestDB`].
        pub fn runtime(&self) -> Option<&tokio::runtime::Handle> {
            self.durable.as_ref().map(|ds| ds.rt.handle())
        }

        /// Deconstruct `self` into its constituents.
        #[allow(unused)]
        pub fn into_parts(
            self,
        ) -> (
            RelationalDB,
            Option<Arc<durability::Local<ProductValue>>>,
            Option<tokio::runtime::Runtime>,
            TestDBDir,
        ) {
            let Self { db, durable, dir, .. } = self;
            let (durability, rt) = durable
                .map(|DurableState { handle, rt }| (Some(handle), Some(rt)))
                .unwrap_or((None, None));
            (db, durability, rt, dir)
        }

        fn in_memory_internal(root: &ReplicaDir) -> Result<RelationalDB, DBError> {
            Self::open_db(root, EmptyHistory::new(), None, None, 0)
        }

        fn durable_internal(
            root: &ReplicaDir,
            rt: tokio::runtime::Handle,
            want_snapshot_repo: bool,
        ) -> Result<(RelationalDB, Arc<durability::Local<ProductValue>>), DBError> {
            let snapshots = want_snapshot_repo
                .then(|| {
                    open_snapshot_repo(root.snapshots(), Identity::ZERO, 0)
                        .map(|repo| SnapshotWorker::new(repo, snapshot::Compression::Disabled))
                })
                .transpose()?;
            let (local, disk_size_fn) = rt.block_on(local_durability(root.commit_log(), snapshots.as_ref()))?;
            let history = local.clone();
            let persistence = Persistence {
                durability: local.clone(),
                disk_size: disk_size_fn,
                snapshots,
            };
            let db = Self::open_db(root, history, Some(persistence), None, 0)?;

            Ok((db, local))
        }

        pub fn open_db(
            root: &ReplicaDir,
            history: impl durability::History<TxData = Txdata>,
            persistence: Option<Persistence>,
            metrics_recorder_queue: Option<MetricsRecorderQueue>,
            expected_num_clients: usize,
        ) -> Result<RelationalDB, DBError> {
            let (db, connected_clients) = RelationalDB::open(
                root,
                Self::DATABASE_IDENTITY,
                Self::OWNER,
                history,
                persistence,
                metrics_recorder_queue,
                PagePool::new_for_test(),
            )?;
            assert_eq!(connected_clients.len(), expected_num_clients);
            let db = db.with_row_count(Self::row_count_fn());
            db.with_auto_commit(Workload::Internal, |tx| {
                db.set_initialized(tx, HostType::Wasm, Program::empty())
            })?;
            Ok(db)
        }

        // NOTE: This is important to make compiler tests work.
        fn row_count_fn() -> RowCountFn {
            Arc::new(|_, _| i64::MAX)
        }

        pub fn take_snapshot(&self, repo: &SnapshotRepository) -> Result<Option<SnapshotDirPath>, DBError> {
            Ok(self.inner.take_snapshot(repo)?)
        }
    }

    impl Deref for TestDB {
        type Target = RelationalDB;

        fn deref(&self) -> &Self::Target {
            &self.db
        }
    }

    pub fn with_read_only<T>(db: &RelationalDB, f: impl FnOnce(&mut Tx) -> T) -> T {
        db.with_read_only(Workload::ForTests, f)
    }

    pub fn with_auto_commit<A, E: From<DBError>>(
        db: &RelationalDB,
        f: impl FnOnce(&mut MutTx) -> Result<A, E>,
    ) -> Result<A, E> {
        db.with_auto_commit(Workload::ForTests, f)
    }

    pub fn begin_tx(db: &RelationalDB) -> TxId {
        db.begin_tx(Workload::ForTests)
    }

    pub fn begin_mut_tx(db: &RelationalDB) -> MutTxId {
        db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests)
    }

    pub fn insert<'a, T: Serialize>(
        db: &'a RelationalDB,
        tx: &'a mut MutTx,
        table_id: TableId,
        row: &T,
    ) -> Result<(AlgebraicValue, RowRef<'a>), DBError> {
        let (gen_cols, row_ref, _) = db.insert(tx, table_id, &to_vec(row).unwrap())?;
        let gen_cols = row_ref.project(&gen_cols).unwrap();
        Ok((gen_cols, row_ref))
    }

    /// An in-memory commitlog used for tests that want to replay a known history.
    pub struct TestHistory(commitlog::commitlog::Generic<commitlog::repo::Memory, Txdata>);

    impl durability::History for TestHistory {
        type TxData = Txdata;

        fn fold_transactions_from<D>(&self, offset: TxOffset, decoder: D) -> Result<(), D::Error>
        where
            D: commitlog::Decoder,
            D::Error: From<commitlog::error::Traversal>,
        {
            self.0.fold_transactions_from(offset, decoder)
        }

        fn transactions_from<'a, D>(
            &self,
            offset: TxOffset,
            decoder: &'a D,
        ) -> impl Iterator<Item = Result<commitlog::Transaction<Self::TxData>, D::Error>>
        where
            D: commitlog::Decoder<Record = Self::TxData>,
            D::Error: From<commitlog::error::Traversal>,
            Self::TxData: 'a,
        {
            self.0.transactions_from(offset, decoder)
        }

        fn tx_range_hint(&self) -> (TxOffset, Option<TxOffset>) {
            let min = self.0.min_committed_offset().unwrap_or_default();
            let max = self.0.max_committed_offset();

            (min, max)
        }
    }

    impl TestHistory {
        pub fn from_txes(txes: impl IntoIterator<Item = Txdata>) -> Self {
            let mut log = commitlog::tests::helpers::mem_log::<Txdata>(32);
            commitlog::tests::helpers::fill_log_with(&mut log, txes);
            Self(log)
        }
    }

    pub fn make_snapshot(
        dir: SnapshotsPath,
        identity: Identity,
        replica: u64,
        compress: CompressType,
        delete_if_exists: bool,
    ) -> (SnapshotsPath, SnapshotRepository) {
        let path = dir.0.join(format!("{replica}_{compress:?}"));
        if delete_if_exists && path.exists() {
            std::fs::remove_dir_all(&path).unwrap();
        }
        let dir = SnapshotsPath::from_path_unchecked(path);
        dir.create().unwrap();
        let snapshot = SnapshotRepository::open(dir.clone(), identity, replica).unwrap();

        (dir, snapshot)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_macros)]

    use std::cell::RefCell;
    use std::fs::OpenOptions;
    use std::path::PathBuf;
    use std::rc::Rc;

    use super::tests_utils::begin_mut_tx;
    use super::*;
    use crate::db::relational_db::tests_utils::{
        begin_tx, insert, make_snapshot, with_auto_commit, with_read_only, TestDB,
    };
    use anyhow::bail;
    use bytes::Bytes;
    use commitlog::payload::txdata;
    use commitlog::Commitlog;
    use durability::EmptyHistory;
    use pretty_assertions::{assert_eq, assert_matches};
    use spacetimedb_data_structures::map::IntMap;
    use spacetimedb_datastore::error::{DatastoreError, IndexError};
    use spacetimedb_datastore::execution_context::ReducerContext;
    use spacetimedb_datastore::system_tables::{
        system_tables, StConstraintRow, StIndexRow, StSequenceRow, StTableRow, ST_CONSTRAINT_ID, ST_INDEX_ID,
        ST_SEQUENCE_ID, ST_TABLE_ID,
    };
    use spacetimedb_fs_utils::compression::CompressType;
    use spacetimedb_lib::db::raw_def::v9::{btree, RawTableDefBuilder};
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::Identity;
    use spacetimedb_lib::Timestamp;
    use spacetimedb_paths::FromPathUnchecked;
    use spacetimedb_sats::buffer::BufReader;
    use spacetimedb_sats::product;
    use spacetimedb_schema::schema::RowLevelSecuritySchema;
    use spacetimedb_snapshot::CompressionStats;
    #[cfg(unix)]
    use spacetimedb_snapshot::Snapshot;
    use spacetimedb_table::read_column::ReadColumn;
    use spacetimedb_table::table::RowRef;
    use tempfile::TempDir;
    use tests::tests_utils::TestHistory;

    fn my_table(col_type: AlgebraicType) -> TableSchema {
        table("MyTable", ProductType::from([("my_col", col_type)]), |builder| builder)
    }

    fn table(
        name: &str,
        columns: ProductType,
        f: impl FnOnce(RawTableDefBuilder<'_>) -> RawTableDefBuilder,
    ) -> TableSchema {
        let mut builder = RawModuleDefV9Builder::new();
        f(builder.build_table_with_new_type(name, columns, true));
        let raw = builder.finish();
        let def: ModuleDef = raw.try_into().expect("table validation failed");
        let table = def.table(name).expect("table not found");
        TableSchema::from_module_def(&def, table, (), TableId::SENTINEL)
    }

    fn table_auto_inc() -> TableSchema {
        table(
            "MyTable",
            ProductType::from([("my_col", AlgebraicType::I64)]),
            |builder| {
                builder
                    .with_primary_key(0)
                    .with_column_sequence(0)
                    .with_unique_constraint(0)
                    .with_index_no_accessor_name(btree(0))
            },
        )
    }

    fn table_indexed(is_unique: bool) -> TableSchema {
        table(
            "MyTable",
            ProductType::from([("my_col", AlgebraicType::I64), ("other_col", AlgebraicType::I64)]),
            |builder| {
                let builder = builder.with_index_no_accessor_name(btree(0));

                if is_unique {
                    builder.with_unique_constraint(col_list![0])
                } else {
                    builder
                }
            },
        )
    }

    #[test]
    fn test() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        stdb.commit_tx(tx)?;

        Ok(())
    }

    #[test]
    fn test_system_variables() {
        let db = TestDB::durable().expect("failed to create db");
        let _ = with_auto_commit(&db, |tx| db.write_var(tx, StVarName::RowLimit, "5"));
        assert_eq!(
            5,
            with_read_only(&db, |tx| db.row_limit(tx))
                .expect("failed to read from st_var")
                .expect("row_limit does not exist")
        );
    }

    #[test]
    fn test_open_twice() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        stdb.commit_tx(tx)?;

        match RelationalDB::open(
            stdb.path(),
            Identity::ZERO,
            Identity::ZERO,
            EmptyHistory::new(),
            None,
            None,
            PagePool::new_for_test(),
        ) {
            Ok(_) => {
                panic!("Allowed to open database twice")
            }
            Err(e) => match e {
                DBError::Database(DatabaseError::DatabasedOpened(_, _)) => {}
                err => {
                    panic!("Failed with error {err}")
                }
            },
        }

        Ok(())
    }

    #[test]
    fn test_table_name() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        let t_id = stdb.table_id_from_name_mut(&tx, "MyTable")?;
        assert_eq!(t_id, Some(table_id));
        Ok(())
    }

    #[test]
    fn test_column_name() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        let table_id = stdb.table_id_from_name_mut(&tx, "MyTable")?.unwrap();
        let schema = stdb.schema_for_table_mut(&tx, table_id)?;
        let col = schema.columns().iter().find(|x| &*x.col_name == "my_col").unwrap();
        assert_eq!(col.col_pos, 0.into());
        Ok(())
    }

    #[test]
    fn test_create_table_pre_commit() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        let schema = my_table(AlgebraicType::I32);
        stdb.create_table(&mut tx, schema.clone())?;
        let result = stdb.create_table(&mut tx, schema);
        result.expect_err("create_table should error when called twice");
        Ok(())
    }

    fn read_first_col<T: ReadColumn>(row: RowRef<'_>) -> T {
        row.read_col(0).unwrap()
    }

    fn collect_sorted<T: ReadColumn + Ord>(stdb: &RelationalDB, tx: &MutTx, table_id: TableId) -> ResultTest<Vec<T>> {
        let mut rows = stdb.iter_mut(tx, table_id)?.map(read_first_col).collect::<Vec<T>>();
        rows.sort();
        Ok(rows)
    }

    fn collect_from_sorted<T: ReadColumn + Into<AlgebraicValue> + Ord>(
        stdb: &RelationalDB,
        tx: &MutTx,
        table_id: TableId,
        from: T,
    ) -> ResultTest<Vec<T>> {
        let from: AlgebraicValue = from.into();
        let mut rows = stdb
            .iter_by_col_range_mut(tx, table_id, 0, from..)?
            .map(read_first_col)
            .collect::<Vec<T>>();
        rows.sort();
        Ok(rows)
    }

    fn insert_three_i32s(stdb: &RelationalDB, tx: &mut MutTx, table_id: TableId) -> ResultTest<()> {
        for v in [-1, 0, 1] {
            insert(stdb, tx, table_id, &product![v])?;
        }
        Ok(())
    }

    #[test]
    fn test_pre_commit() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;

        insert_three_i32s(&stdb, &mut tx, table_id)?;
        assert_eq!(collect_sorted::<i32>(&stdb, &tx, table_id)?, vec![-1, 0, 1]);
        Ok(())
    }

    #[test]
    fn test_post_commit() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);

        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;

        insert_three_i32s(&stdb, &mut tx, table_id)?;
        stdb.commit_tx(tx)?;

        let tx = begin_mut_tx(&stdb);
        assert_eq!(collect_sorted::<i32>(&stdb, &tx, table_id)?, vec![-1, 0, 1]);
        Ok(())
    }

    #[test]
    fn test_filter_range_pre_commit() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);

        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        insert_three_i32s(&stdb, &mut tx, table_id)?;
        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i32)?, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn test_filter_range_post_commit() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);

        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;

        insert_three_i32s(&stdb, &mut tx, table_id)?;
        stdb.commit_tx(tx)?;

        let tx = begin_mut_tx(&stdb);
        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i32)?, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn test_create_table_rollback() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);

        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        let _ = stdb.rollback_mut_tx(tx);

        let tx = begin_mut_tx(&stdb);
        let result = stdb.table_id_from_name_mut(&tx, "MyTable")?;
        assert!(
            result.is_none(),
            "Table should not exist, so table_id_from_name should return none"
        );

        let result = stdb.table_name_from_id_mut(&tx, table_id)?;
        assert!(
            result.is_none(),
            "Table should not exist, so table_name_from_id_mut should return none",
        );
        Ok(())
    }

    #[test]
    fn test_rollback() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);

        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        stdb.commit_tx(tx)?;

        let mut tx = begin_mut_tx(&stdb);
        insert_three_i32s(&stdb, &mut tx, table_id)?;
        let _ = stdb.rollback_mut_tx(tx);

        let tx = begin_mut_tx(&stdb);
        assert_eq!(collect_sorted::<i32>(&stdb, &tx, table_id)?, Vec::<i32>::new());
        Ok(())
    }

    #[test]
    fn test_auto_inc() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        let schema = table_auto_inc();
        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "MyTable_my_col_seq")?;
        assert!(sequence.is_some(), "Sequence not created");

        insert(&stdb, &mut tx, table_id, &product![0i64])?;
        insert(&stdb, &mut tx, table_id, &product![0i64])?;

        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![1, 2]);
        Ok(())
    }

    #[test]
    fn test_auto_inc_disable() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        let schema = table_auto_inc();
        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "MyTable_my_col_seq")?;
        assert!(sequence.is_some(), "Sequence not created");

        insert(&stdb, &mut tx, table_id, &product![5i64])?;
        insert(&stdb, &mut tx, table_id, &product![6i64])?;

        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![5, 6]);
        Ok(())
    }

    #[test]
    fn test_auto_inc_reload() -> ResultTest<()> {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Trace)
            .format_timestamp(None)
            .is_test(true)
            .try_init();

        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        let schema = table_auto_inc();

        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "MyTable_my_col_seq")?;
        assert!(sequence.is_some(), "Sequence not created");

        insert(&stdb, &mut tx, table_id, &product![0i64])?;
        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![1]);

        stdb.commit_tx(tx)?;

        let stdb = stdb.reopen()?;

        let mut tx = begin_mut_tx(&stdb);
        insert(&stdb, &mut tx, table_id, &product![0i64]).unwrap();

        // Check the second row start after `SEQUENCE_PREALLOCATION_AMOUNT`
        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![1, 4097]);
        stdb.commit_tx(tx)?;

        let stdb = stdb.reopen()?;
        let mut tx = begin_mut_tx(&stdb);
        insert(&stdb, &mut tx, table_id, &product![0i64]).unwrap();
        // The next value will have a gap because we preallocate, but asserting the specific number
        // seems brittle.
        assert!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?[2] > 4097);
        Ok(())
    }

    #[test]
    fn test_indexed() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        let schema = table_indexed(false);

        let table_id = stdb.create_table(&mut tx, schema)?;

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx_btree")?.is_some(),
            "Index not created"
        );

        insert(&stdb, &mut tx, table_id, &product![1i64, 1i64])?;
        insert(&stdb, &mut tx, table_id, &product![1i64, 1i64])?;

        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![1]);
        Ok(())
    }

    #[test]
    fn test_row_count() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        let schema = my_table(AlgebraicType::I64);
        let table_id = stdb.create_table(&mut tx, schema)?;
        insert(&stdb, &mut tx, table_id, &product![1i64])?;
        insert(&stdb, &mut tx, table_id, &product![2i64])?;
        stdb.commit_tx(tx)?;

        let stdb = stdb.reopen()?;
        let tx = begin_tx(&stdb);
        assert_eq!(tx.table_row_count(table_id).unwrap(), 2);
        Ok(())
    }

    // Because we don't create `rls` when first creating the database, check we pass the bootstrap
    #[test]
    fn test_row_level_reopen() -> ResultTest<()> {
        let stdb = TestDB::durable()?;
        let mut tx = begin_mut_tx(&stdb);

        let schema = my_table(AlgebraicType::I64);
        let table_id = stdb.create_table(&mut tx, schema)?;

        let rls = RowLevelSecuritySchema {
            sql: "SELECT * FROM bar".into(),
            table_id,
        };

        tx.create_row_level_security(rls)?;
        stdb.commit_tx(tx)?;

        let stdb = stdb.reopen()?;
        let tx = begin_mut_tx(&stdb);

        assert_eq!(
            tx.row_level_security_for_table_id(table_id)?,
            vec![RowLevelSecuritySchema {
                sql: "SELECT * FROM bar".into(),
                table_id,
            }]
        );

        Ok(())
    }

    #[test]
    fn test_unique() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);

        let schema = table_indexed(true);
        let table_id = stdb.create_table(&mut tx, schema).expect("stdb.create_table failed");

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx_btree")
                .expect("index_id_from_name failed")
                .is_some(),
            "Index not created"
        );

        insert(&stdb, &mut tx, table_id, &product![1i64, 0i64]).expect("stdb.insert failed");
        match insert(&stdb, &mut tx, table_id, &product![1i64, 1i64]) {
            Ok(_) => panic!("Allow to insert duplicate row"),
            Err(DBError::Datastore(DatastoreError::Index(IndexError::UniqueConstraintViolation { .. }))) => {}
            Err(err) => panic!("Expected error `UniqueConstraintViolation`, got {err}"),
        }

        Ok(())
    }

    #[test]
    fn test_identity() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);
        let schema = table(
            "MyTable",
            ProductType::from([("my_col", AlgebraicType::I64)]),
            |builder| {
                builder
                    .with_column_sequence(0)
                    .with_unique_constraint(0)
                    .with_index_no_accessor_name(btree(0))
            },
        );

        let table_id = stdb.create_table(&mut tx, schema)?;

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx_btree")?.is_some(),
            "Index not created"
        );

        let sequence = stdb.sequence_id_from_name(&tx, "MyTable_my_col_seq")?;
        assert!(sequence.is_some(), "Sequence not created");

        insert(&stdb, &mut tx, table_id, &product![0i64])?;
        insert(&stdb, &mut tx, table_id, &product![0i64])?;

        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![1, 2]);
        Ok(())
    }

    #[test]
    fn test_cascade_drop_table() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);

        let schema = table(
            "MyTable",
            ProductType::from([
                ("col1", AlgebraicType::I64),
                ("col2", AlgebraicType::I64),
                ("col3", AlgebraicType::I64),
                ("col4", AlgebraicType::I64),
            ]),
            |builder| {
                builder
                    .with_index_no_accessor_name(btree(0))
                    .with_index_no_accessor_name(btree(1))
                    .with_index_no_accessor_name(btree(2))
                    .with_index_no_accessor_name(btree(3))
                    .with_unique_constraint(0)
                    .with_unique_constraint(1)
                    .with_unique_constraint(3)
                    .with_column_sequence(0)
            },
        );

        let table_id = stdb.create_table(&mut tx, schema)?;

        let indexes = stdb
            .iter_mut(&tx, ST_INDEX_ID)?
            .map(|x| StIndexRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(indexes.len(), 4, "Wrong number of indexes: {:#?}", indexes);

        let sequences = stdb
            .iter_mut(&tx, ST_SEQUENCE_ID)?
            .map(|x| StSequenceRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(sequences.len(), 1, "Wrong number of sequences");

        let constraints = stdb
            .iter_mut(&tx, ST_CONSTRAINT_ID)?
            .map(|x| StConstraintRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(constraints.len(), 3, "Wrong number of constraints");

        stdb.drop_table(&mut tx, table_id)?;

        let indexes = stdb
            .iter_mut(&tx, ST_INDEX_ID)?
            .map(|x| StIndexRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(indexes.len(), 0, "Wrong number of indexes DROP");

        let sequences = stdb
            .iter_mut(&tx, ST_SEQUENCE_ID)?
            .map(|x| StSequenceRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(sequences.len(), 0, "Wrong number of sequences DROP");

        let constraints = stdb
            .iter_mut(&tx, ST_CONSTRAINT_ID)?
            .map(|x| StConstraintRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(constraints.len(), 0, "Wrong number of constraints DROP");

        Ok(())
    }

    #[test]
    fn test_rename_table() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = begin_mut_tx(&stdb);

        let table_id = stdb.create_table(&mut tx, table_indexed(true))?;
        stdb.rename_table(&mut tx, table_id, "YourTable")?;
        let table_name = stdb.table_name_from_id_mut(&tx, table_id)?;

        assert_eq!(Some("YourTable"), table_name.as_ref().map(Cow::as_ref));
        // Also make sure we've removed the old ST_TABLES_ID row
        let mut n = 0;
        for row in stdb.iter_mut(&tx, ST_TABLE_ID)? {
            let table = StTableRow::try_from(row)?;
            if table.table_id == table_id {
                n += 1;
            }
        }
        assert_eq!(1, n);

        Ok(())
    }

    #[test]
    fn test_multi_column_index() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let columns = ProductType::from([
            ("a", AlgebraicType::U64),
            ("b", AlgebraicType::U64),
            ("c", AlgebraicType::U64),
        ]);

        let schema = table("t", columns, |builder| {
            builder.with_index(btree([0, 1]), "accessor_name_doesnt_matter")
        });

        let mut tx = begin_mut_tx(&stdb);
        let table_id = stdb.create_table(&mut tx, schema)?;

        insert(&stdb, &mut tx, table_id, &product![0u64, 0u64, 1u64])?;
        insert(&stdb, &mut tx, table_id, &product![0u64, 1u64, 2u64])?;
        insert(&stdb, &mut tx, table_id, &product![1u64, 2u64, 2u64])?;

        let cols = col_list![0, 1];
        let value = product![0u64, 1u64].into();

        let IterByColEqMutTx::Index(mut iter) = stdb.iter_by_col_eq_mut(&tx, table_id, cols, &value)? else {
            panic!("expected index iterator");
        };

        let Some(row) = iter.next() else {
            panic!("expected non-empty iterator");
        };

        assert_eq!(row.to_product_value(), product![0u64, 1u64, 2u64]);

        // iter should only return a single row, so this count should now be 0.
        assert_eq!(iter.count(), 0);
        Ok(())
    }

    #[test]
    /// Test that iteration yields each row only once
    /// in the edge case where a row is committed and has been deleted and re-inserted within the iterating TX.
    fn test_insert_delete_insert_iter() {
        let stdb = TestDB::durable().expect("failed to create TestDB");

        let mut initial_tx = begin_mut_tx(&stdb);
        let schema = my_table(AlgebraicType::I32);

        let table_id = stdb.create_table(&mut initial_tx, schema).expect("create_table failed");

        stdb.commit_tx(initial_tx).expect("Commit initial_tx failed");

        // Insert a row and commit it, so the row is in the committed_state.
        let mut insert_tx = begin_mut_tx(&stdb);
        insert(&stdb, &mut insert_tx, table_id, &product!(AlgebraicValue::I32(0))).expect("Insert insert_tx failed");
        stdb.commit_tx(insert_tx).expect("Commit insert_tx failed");

        let mut delete_insert_tx = begin_mut_tx(&stdb);
        // Delete the row, so it's in the `delete_tables` of `delete_insert_tx`.
        assert_eq!(
            stdb.delete_by_rel(&mut delete_insert_tx, table_id, [product!(AlgebraicValue::I32(0))]),
            1
        );

        // Insert the row again, so that depending on the datastore internals,
        // it may now be only in the committed_state,
        // or in all three of the committed_state, delete_tables and insert_tables.
        insert(
            &stdb,
            &mut delete_insert_tx,
            table_id,
            &product!(AlgebraicValue::I32(0)),
        )
        .expect("Insert delete_insert_tx failed");

        // Iterate over the table and assert that we see the committed-deleted-inserted row only once.
        assert_eq!(
            &stdb
                .iter_mut(&delete_insert_tx, table_id)
                .expect("iter delete_insert_tx failed")
                .map(|row_ref| row_ref.to_product_value())
                .collect::<Vec<_>>(),
            &[product!(AlgebraicValue::I32(0))],
        );

        let _ = stdb.rollback_mut_tx(delete_insert_tx);
    }

    #[test]
    fn test_tx_inputs_are_in_the_commitlog() {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Trace)
            .format_timestamp(None)
            .is_test(true)
            .try_init();

        let stdb = TestDB::durable().expect("failed to create TestDB");

        let timestamp = Timestamp::now();
        let ctx = ReducerContext {
            name: "abstract_concrete_proxy_factory_impl".into(),
            caller_identity: Identity::__dummy(),
            caller_connection_id: ConnectionId::ZERO,
            timestamp,
            arg_bsatn: Bytes::new(),
        };

        let row_ty = ProductType::from([("le_boeuf", AlgebraicType::I32)]);
        let schema = table("test_table", row_ty.clone(), |builder| builder);

        // Create an empty transaction
        {
            let tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::Reducer(ctx.clone()));
            stdb.commit_tx(tx).expect("failed to commit empty transaction");
        }

        // Create an empty transaction pretending to be an
        // `__identity_connected__` call.
        {
            let tx = stdb.begin_mut_tx(
                IsolationLevel::Serializable,
                Workload::Reducer(ReducerContext {
                    name: "__identity_connected__".into(),
                    caller_identity: Identity::__dummy(),
                    caller_connection_id: ConnectionId::ZERO,
                    timestamp,
                    arg_bsatn: Bytes::new(),
                }),
            );
            stdb.commit_tx(tx)
                .expect("failed to commit empty __identity_connected__ transaction");
        }

        // Create a non-empty transaction including reducer info
        let table_id = {
            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::Reducer(ctx));
            let table_id = stdb.create_table(&mut tx, schema).expect("failed to create table");
            insert(&stdb, &mut tx, table_id, &product!(AlgebraicValue::I32(0))).expect("failed to insert row");
            stdb.commit_tx(tx).expect("failed to commit tx");

            table_id
        };

        // Create a non-empty transaction without reducer info, as it would be
        // created by a mutable SQL transaction
        {
            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::Sql);
            insert(&stdb, &mut tx, table_id, &product!(AlgebraicValue::I32(-42))).expect("failed to insert row");
            stdb.commit_tx(tx).expect("failed to commit tx");
        }

        // `txdata::Visitor` which only collects `txdata::Inputs`.
        struct Inputs {
            // The inputs collected during traversal of the log.
            inputs: Vec<txdata::Inputs>,
            // The number of transactions seen during traversal of the log.
            num_txs: usize,
            // System tables, needed to be able to consume transaction records.
            sys: IntMap<TableId, ProductType>,
            // The table created above, needed to be able to consume transaction
            // records.
            row_ty: ProductType,
        }

        impl txdata::Visitor for Inputs {
            type Row = ();
            type Error = anyhow::Error;

            fn visit_insert<'a, R: BufReader<'a>>(
                &mut self,
                table_id: TableId,
                reader: &mut R,
            ) -> Result<Self::Row, Self::Error> {
                let ty = self.sys.get(&table_id).unwrap_or(&self.row_ty);
                let row = ProductValue::decode(ty, reader)?;
                log::debug!("insert: {table_id} {row:?}");
                Ok(())
            }

            fn visit_delete<'a, R: BufReader<'a>>(
                &mut self,
                table_id: TableId,
                reader: &mut R,
            ) -> Result<Self::Row, Self::Error> {
                // Allow specifically deletes from `st_sequence`,
                // since the transactions in this test will allocate sequence values.
                if table_id != ST_SEQUENCE_ID {
                    bail!("unexpected delete for table: {table_id}")
                }
                let ty = self.sys.get(&table_id).unwrap();
                let row = ProductValue::decode(ty, reader)?;
                log::debug!("delete: {table_id} {row:?}");
                Ok(())
            }

            fn skip_row<'a, R: BufReader<'a>>(
                &mut self,
                table_id: TableId,
                _reader: &mut R,
            ) -> Result<(), Self::Error> {
                bail!("unexpected skip for table: {table_id}")
            }

            fn visit_inputs(&mut self, inputs: &txdata::Inputs) -> Result<(), Self::Error> {
                log::debug!("visit_inputs: {inputs:?}");
                self.inputs.push(inputs.clone());
                Ok(())
            }

            fn visit_tx_start(&mut self, offset: u64) -> Result<(), Self::Error> {
                log::debug!("tx start: {offset}");
                self.num_txs += 1;
                Ok(())
            }

            fn visit_tx_end(&mut self) -> Result<(), Self::Error> {
                log::debug!("tx end");
                Ok(())
            }
        }

        struct Decoder(Rc<RefCell<Inputs>>);

        impl spacetimedb_commitlog::Decoder for Decoder {
            type Record = txdata::Txdata<()>;
            type Error = txdata::DecoderError<anyhow::Error>;

            #[inline]
            fn decode_record<'a, R: BufReader<'a>>(
                &self,
                version: u8,
                tx_offset: u64,
                reader: &mut R,
            ) -> Result<Self::Record, Self::Error> {
                txdata::decode_record_fn(&mut *self.0.borrow_mut(), version, tx_offset, reader)
            }

            fn skip_record<'a, R: BufReader<'a>>(
                &self,
                version: u8,
                _tx_offset: u64,
                reader: &mut R,
            ) -> Result<(), Self::Error> {
                txdata::skip_record_fn(&mut *self.0.borrow_mut(), version, reader)
            }
        }

        let (db, durablity, rt, dir) = stdb.into_parts();
        // Free reference to durability.
        drop(db);
        // Ensure everything is flushed to disk.
        rt.expect("Durable TestDB must have a runtime")
            .block_on(
                Arc::into_inner(durablity.expect("Durable TestDB must have a durability"))
                    .expect("failed to unwrap Arc")
                    .close(),
            )
            .expect("failed to close local durabilility");

        // Re-open commitlog and collect inputs.
        let inputs = Rc::new(RefCell::new(Inputs {
            inputs: Vec::new(),
            num_txs: 0,
            sys: system_tables()
                .into_iter()
                .map(|schema| (schema.table_id, schema.into_row_type()))
                .collect(),
            row_ty,
        }));
        {
            let clog =
                Commitlog::<()>::open(dir.commit_log(), Default::default(), None).expect("failed to open commitlog");
            let decoder = Decoder(Rc::clone(&inputs));
            clog.fold_transactions(decoder).unwrap();
        }
        // Just a safeguard so we don't drop the temp dir before this point.
        drop(dir);

        let inputs = Rc::into_inner(inputs).unwrap().into_inner();
        log::debug!("collected inputs: {:?}", inputs.inputs);

        // We should've seen four transactions:
        //
        // - the internal tx which initializes `st_module`
        // - three non-empty transactions here
        //
        // The empty transaction should've been ignored.
        assert_eq!(inputs.num_txs, 4);
        // Two of the transactions should yield inputs.
        assert_eq!(inputs.inputs.len(), 2);

        // Also assert that we got what we put in.
        for (i, input) in inputs.inputs.into_iter().enumerate() {
            let ReducerContext {
                name: reducer_name,
                caller_identity,
                caller_connection_id,
                timestamp: reducer_timestamp,
                arg_bsatn,
            } = ReducerContext::try_from(&input).unwrap();
            if i == 0 {
                assert_eq!(reducer_name, "__identity_connected__");
            } else {
                assert_eq!(reducer_name, "abstract_concrete_proxy_factory_impl");
            }
            assert!(
                arg_bsatn.is_empty(),
                "expected args to be exhausted because nullary args were given"
            );
            assert_eq!(caller_identity, Identity::ZERO);
            assert_eq!(caller_connection_id, ConnectionId::ZERO);
            assert_eq!(reducer_timestamp, timestamp);
        }
    }

    /// This tests that we are able to correctly replay mutations to system tables,
    /// in this case specifically `st_client`.
    ///
    /// [SpacetimeDB PR #2161](https://github.com/clockworklabs/SpacetimeDB/pull/2161)
    /// fixed a bug where replaying deletes to `st_client` would fail due to an unpopulated index.
    #[test]
    fn replay_delete_from_st_client() {
        use spacetimedb_datastore::system_tables::{StClientRow, ST_CLIENT_ID};

        let row_0 = StClientRow {
            identity: Identity::ZERO.into(),
            connection_id: ConnectionId::ZERO.into(),
        };
        let row_1 = StClientRow {
            identity: Identity::ZERO.into(),
            connection_id: ConnectionId::from_u128(1).into(),
        };

        let history = TestHistory::from_txes([
            // TX 0: insert row 0
            Txdata {
                inputs: None,
                outputs: None,
                mutations: Some(txdata::Mutations {
                    inserts: Box::new([txdata::Ops {
                        table_id: ST_CLIENT_ID,
                        rowdata: Arc::new([row_0.into()]),
                    }]),
                    deletes: Box::new([]),
                    truncates: Box::new([]),
                }),
            },
            // TX 1: delete row 0
            Txdata {
                inputs: None,
                outputs: None,
                mutations: Some(txdata::Mutations {
                    inserts: Box::new([]),
                    deletes: Box::new([txdata::Ops {
                        table_id: ST_CLIENT_ID,
                        rowdata: Arc::new([row_0.into()]),
                    }]),
                    truncates: Box::new([]),
                }),
            },
            // TX 2: insert row 1
            Txdata {
                inputs: None,
                outputs: None,
                mutations: Some(txdata::Mutations {
                    inserts: Box::new([txdata::Ops {
                        table_id: ST_CLIENT_ID,
                        rowdata: Arc::new([row_1.into()]),
                    }]),
                    deletes: Box::new([]),
                    truncates: Box::new([]),
                }),
            },
        ]);

        // We expect 1 client, since we left `row_1` in there.
        let stdb = TestDB::in_memory_with_history(history, /* expected_num_clients: */ 1).unwrap();

        let read_tx = begin_tx(&stdb);

        // Read all of st_client, assert that there's only one row, and that said row is `row_1`.
        let present_rows: Vec<StClientRow> = stdb
            .iter(&read_tx, ST_CLIENT_ID)
            .unwrap()
            .map(|row_ref| row_ref.try_into().unwrap())
            .collect();
        assert_eq!(present_rows.len(), 1);
        assert_eq!(present_rows[0], row_1);

        let _ = stdb.release_tx(read_tx);
    }

    // Verify that we can compress snapshots and hardlink them,
    // except for the last one, which should be uncompressed.
    // Then, verify that we can read the compressed snapshot.
    //
    // NOTE: `snapshot_watching_compressor` is what filter out the last snapshot
    #[test]
    fn compress_snapshot_test() -> ResultTest<()> {
        let stdb = TestDB::in_memory()?;

        let mut tx = begin_mut_tx(&stdb);
        let schema = my_table(AlgebraicType::I32);
        let table_id = stdb.create_table(&mut tx, schema)?;
        for v in 0..3 {
            insert(&stdb, &mut tx, table_id, &product![v])?;
        }
        stdb.commit_tx(tx)?;

        let root = stdb.path().snapshots();
        let (dir, repo) = make_snapshot(root.clone(), Identity::ZERO, 0, CompressType::None, true);
        stdb.take_snapshot(&repo)?;

        #[cfg(unix)]
        let total_objects = repo.size_on_disk()?.object_count;
        // Another snapshots that will hardlink part of the first one
        for i in 0..2 {
            let mut tx = begin_mut_tx(&stdb);
            for v in 0..(10 + i) {
                insert(&stdb, &mut tx, table_id, &product![v])?;
            }
            stdb.commit_tx(tx)?;
            stdb.take_snapshot(&repo)?;
        }

        let size_compress_off = repo.size_on_disk()?;
        assert!(
            size_compress_off.total_size > 0,
            "Snapshot size should be greater than 0"
        );
        let mut offsets = repo.all_snapshots()?.collect::<Vec<_>>();
        offsets.sort();
        assert_eq!(&offsets, &[1, 2, 3]);
        // Simulate we take except the last snapshot
        let last_compress = 2;
        let mut stats = CompressionStats::default();
        repo.compress_snapshots(&mut stats, ..3)?;
        assert_eq!(stats.compressed(), 2);
        let size_compress_on = repo.size_on_disk()?;
        assert!(size_compress_on.total_size < size_compress_off.total_size);
        // Verify we hard-linked the second snapshot
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let snapshot_dir = dir.snapshot_dir(last_compress);
            let mut hard_linked_on = 0;
            let mut hard_linked_off = 0;

            let (snapshot, compress) = Snapshot::read_from_file(&snapshot_dir.snapshot_file(last_compress))?;
            assert!(compress.is_compressed());
            let repo = SnapshotRepository::object_repo(&snapshot_dir)?;
            for (_, path) in snapshot.files(&repo) {
                match path.metadata()?.nlink() {
                    0 => hard_linked_off += 1,
                    _ => hard_linked_on += 1,
                }
            }
            assert_eq!(hard_linked_on, total_objects);
            assert_eq!(hard_linked_off, 0);
        }

        // Sanity check that we can read the snapshot after compression
        let repo = open_snapshot_repo(dir, Identity::ZERO, 0)?;
        RelationalDB::restore_from_snapshot_or_bootstrap(
            Identity::ZERO,
            Some(&repo),
            Some(last_compress),
            0,
            PagePool::new_for_test(),
        )?;

        Ok(())
    }

    // For test compression into an existing database.
    // Must supply the path to the database and the identity of the replica using the `ENV`:
    // - `SNAPSHOT` the path to the database, like `/tmp/db/replicas/.../8/database`
    // - `IDENTITY` the identity in hex format
    #[tokio::test]
    #[ignore]
    async fn read_existing() -> ResultTest<()> {
        let path_db = PathBuf::from(std::env::var("SNAPSHOT").expect("SNAPSHOT must be set to a valid path"));
        let identity =
            Identity::from_hex(std::env::var("IDENTITY").expect("IDENTITY must be set to a valid hex identity"))?;
        let path = ReplicaDir::from_path_unchecked(path_db);

        let repo = open_snapshot_repo(path.snapshots(), Identity::ZERO, 0)?;
        assert!(
            repo.size_on_disk()?.total_size > 0,
            "Snapshot size should be greater than 0"
        );

        let last = repo.latest_snapshot()?;
        let stdb =
            RelationalDB::restore_from_snapshot_or_bootstrap(identity, Some(&repo), last, 0, PagePool::new_for_test())?;

        let out = TempDir::with_prefix("snapshot_test")?;
        let dir = SnapshotsPath::from_path_unchecked(out.path());

        let (_, repo) = make_snapshot(dir.clone(), Identity::ZERO, 0, CompressType::zstd(), false);

        stdb.take_snapshot(&repo)?;
        let size = repo.size_on_disk()?;
        assert!(size.total_size > 0, "Snapshot size should be greater than 0");

        Ok(())
    }

    #[test]
    fn tries_older_snapshots() -> ResultTest<()> {
        let stdb = TestDB::in_memory()?;
        stdb.path().snapshots().create()?;
        let repo = SnapshotRepository::open(stdb.path().snapshots(), stdb.database_identity(), 85)?;

        stdb.take_snapshot(&repo)?.expect("failed to take snapshot");
        {
            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
            let schema = my_table(AlgebraicType::I32);
            let table_id = stdb.create_table(&mut tx, schema)?;
            for v in 0..3 {
                insert(&stdb, &mut tx, table_id, &product![v])?;
            }
            stdb.commit_tx(tx)?;
        }
        stdb.take_snapshot(&repo)?.expect("failed to take snapshot");

        let try_restore = |durable_tx_offset, min_commitlog_offset| {
            RelationalDB::restore_from_snapshot_or_bootstrap(
                stdb.database_identity(),
                Some(&repo),
                Some(durable_tx_offset),
                min_commitlog_offset,
                PagePool::new_for_test(),
            )
        };

        try_restore(1, 0)?;
        // We can restore from the previous snapshot
        // if the snapshot file is corrupted
        repo.snapshot_dir_path(1)
            .snapshot_file(1)
            .open_file(OpenOptions::new().write(true))?
            .set_len(1)?;
        try_restore(1, 0)?;
        // Also if it's gone
        std::fs::remove_file(repo.snapshot_dir_path(1).snapshot_file(1))?;
        try_restore(1, 0)?;
        // But not if the commitlog starts after the previous snapshot
        assert_matches!(
            try_restore(1, 2).map(drop),
            Err(RestoreSnapshotError::NoConnectedSnapshot { .. })
        );

        Ok(())
    }

    #[test]
    /// Test that we can create a table after replaying a durable database
    /// without a snapshot.
    ///
    /// Regression test for
    /// [SpacetimeDB issue #2758](https://github.com/clockworklabs/SpacetimeDB/issues/2758).
    /// Before [the fix](https://github.com/clockworklabs/SpacetimeDB/pull/2760),
    /// this would fail because the sequence allocations for system table sequences,
    /// including the one on `st_table.table_id`,
    /// were not correctly reinitialized in memory after replaying a commitlog
    /// into a database that had been [`Locking::bootstrap`]ped,
    /// as opposed to restored from a snapshot.
    fn repro_2758_create_table_after_replay_without_snapshot() {
        // Create a new database which has an on-disk commitlog but no snapshots.
        let stdb = TestDB::durable_without_snapshot_repo().expect("failed to create TestDB");

        // Begin a transaction, create a table, then commit.
        let mut tx = begin_mut_tx(&stdb);
        let product_type = ProductType::from([("col_0", AlgebraicType::I32)]);
        let schema = table("table_0", product_type.clone(), |builder| builder);
        let table_0_id = stdb.create_table(&mut tx, schema).unwrap();
        stdb.commit_tx(tx).unwrap();

        // At this point, the sequence on `st_table.table_id` will have allocated once and advanced once,
        // i.e. have `allocated = 8193`, `value = 4198`.

        // Reopen the database. This will replay all the way from the commitlog,
        // since we have no snapshot repository.
        let stdb = stdb.reopen().unwrap();

        // Begin a transaction, create another table, then commit.
        let mut tx = begin_mut_tx(&stdb);
        let other_schema = table("table_1", product_type.clone(), |builder| builder);
        // Before the fix to issue #2758,
        // this next call to `create_table` would fail with:
        // ```
        // called `Result::unwrap()` on an `Err` value: Index(UniqueConstraintViolation(UniqueConstraintViolation { constraint_name: "st_table_table_id_idx_btree", table_name: "st_table", cols: ["table_id"], value: U32(4097) }))
        // ```
        let table_1_id = stdb.create_table(&mut tx, other_schema).unwrap();
        stdb.commit_tx(tx).unwrap();

        // Quick sanity check: we actually got different table IDs,
        // we didn't just fail to detect a unique constraint violation.
        assert!(table_1_id > table_0_id);
    }

    use crate::error::DBError;
    use fs_extra::dir::{copy, CopyOptions};
    use itertools::Itertools;

    /// Make a temp dir with the contents of the given directory.
    fn copy_fixture_dir(src: &PathBuf) -> TempDir {
        // Create a temporary directory
        let tempdir = TempDir::new().expect("create temp dir");

        // Copy everything from src into tempdir
        let options = CopyOptions {
            overwrite: true,
            copy_inside: true, // copy the contents, not the directory itself
            ..Default::default()
        };
        copy(src, tempdir.path(), &options).expect("copy fixtures");

        tempdir
    }

    /// Load a v1.2 database, verify that the expected tables are present, and we can add a table.
    fn load_1_2_data(use_snapshot: bool) -> ResultTest<()> {
        let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/v1.2/replicas");
        let tempdir = copy_fixture_dir(&data_dir);

        let dir = ReplicaDir::from_path_unchecked(tempdir.path().join("replicas/22000001"));

        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
        // Enter the runtime so that `Self::durable_internal` can spawn a `SnapshotWorker`.
        let _rt = rt.enter();

        let db_identity = Identity::from_hex("c2006c1c2ede44b44e9835dfe00e3e1edc3bc67024b504cb6a3b8491db5d98a3")?;
        let owner_identity = Identity::from_hex("c2001c551217385bdeba5f1e1b5b19a28910852a94ce43428508e7a098760cea")?;
        let (db, _durability_handle) = TestDB::open_existing_durable(
            &dir,
            rt.handle().clone(),
            22000001,
            db_identity,
            owner_identity,
            use_snapshot,
        )?;
        let mut tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let schemas = db.get_all_tables_mut(&tx)?;
        let user_table_names: Vec<String> = schemas
            .iter()
            .filter(|s| s.table_id.0 > 4000)
            .sorted_by_key(|s| s.table_id.0)
            .map(|s| s.table_name.clone().to_string())
            .collect();
        let expected_table_names = vec!["message", "user"];
        assert_eq!(user_table_names, expected_table_names);

        let table_id = db.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        db.commit_tx(tx)?;

        let tx = db.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let t_id = db.table_id_from_name_mut(&tx, "MyTable")?.unwrap();
        assert_eq!(table_id, t_id);
        assert_eq!("MyTable", db.table_name_from_id_mut(&tx, t_id)?.unwrap());
        db.commit_tx(tx)?;
        let tx = db.begin_tx(Workload::ForTests);
        let user_table_names: Vec<String> = db
            .get_all_tables(&tx)?
            .iter()
            .filter(|s| s.table_id.0 > 4000)
            .sorted_by_key(|s| s.table_id.0)
            .map(|s| s.table_name.clone().to_string())
            .collect();
        let expected_table_names = vec!["message", "user", "MyTable"];
        assert_eq!(user_table_names, expected_table_names);

        Ok(())
    }

    #[test]
    fn load_1_2_quickstart_from_snapshot_test() -> ResultTest<()> {
        load_1_2_data(true)
    }

    #[test]
    fn load_1_2_quickstart_without_snapshot_test() -> ResultTest<()> {
        load_1_2_data(false)
    }

    /// This tests adding a new system table st_connection_credentials, which was not in 1.2.
    /// We ensure that we can open data from a version without it, write to the table,
    /// and then reopen the database and read from the table.
    fn load_1_2_data_and_migrate(use_snapshot: bool) -> ResultTest<()> {
        let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/v1.2/replicas");
        let tempdir = copy_fixture_dir(&data_dir);

        let dir = ReplicaDir::from_path_unchecked(tempdir.path().join("replicas/22000001"));

        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
        // Enter the runtime so that `Self::durable_internal` can spawn a `SnapshotWorker`.
        let _rt = rt.enter();

        let db_identity = Identity::from_hex("c2006c1c2ede44b44e9835dfe00e3e1edc3bc67024b504cb6a3b8491db5d98a3")?;
        let owner_identity = Identity::from_hex("c2001c551217385bdeba5f1e1b5b19a28910852a94ce43428508e7a098760cea")?;
        let (db, durability_handle) = TestDB::open_existing_durable(
            &dir,
            rt.handle().clone(),
            22000001,
            db_identity,
            owner_identity,
            use_snapshot,
        )?;
        let schemas = db.with_read_only(Workload::ForTests, |tx| db.get_all_tables(tx))?;
        let user_table_names: Vec<String> = schemas
            .iter()
            .filter(|s| s.table_id.0 > 4000)
            .sorted_by_key(|s| s.table_id.0)
            .map(|s| s.table_name.clone().to_string())
            .collect();
        let expected_table_names = vec!["message", "user"];
        // We have this assertion just to double check that we aren't accidentally opening an empty
        // directory.
        assert_eq!(user_table_names, expected_table_names);

        db.with_auto_commit(Workload::ForTests, |tx| {
            tx.insert_st_client(Identity::ZERO, ConnectionId::ZERO, "invalid_jwt")
                .unwrap();
            Ok::<(), DBError>(())
        })?;
        // Now we are going to shut it down and reopen it, to ensure that the new table can be
        // read from disk.
        drop(db);
        let handle = Arc::into_inner(durability_handle).expect("Durability handle should be dropped by db");
        rt.block_on(handle.close()).expect("Failed to close durability handle");

        let (db, _) = TestDB::open_existing_durable(
            &dir,
            rt.handle().clone(),
            22000001,
            db_identity,
            owner_identity,
            use_snapshot,
        )?;

        db.with_read_only(Workload::ForTests, |tx| {
            let jwt = tx.get_jwt_payload(ConnectionId::ZERO).unwrap().unwrap();
            assert_eq!(jwt, "invalid_jwt");
            Ok::<(), DBError>(())
        })?;

        Ok(())
    }

    #[test]
    fn load_1_2_data_and_migrate_with_snapshot() -> ResultTest<()> {
        load_1_2_data_and_migrate(true)
    }

    #[test]
    fn load_1_2_data_and_migrate_without_snapshot() -> ResultTest<()> {
        load_1_2_data_and_migrate(false)
    }
}
