use super::datastore::locking_tx_datastore::committed_state::CommittedState;
use super::datastore::locking_tx_datastore::state_view::StateView as _;
use super::datastore::system_tables::ST_MODULE_ID;
use super::datastore::traits::{
    IsolationLevel, Metadata, MutTx as _, MutTxDatastore, Program, RowTypeForTable, Tx as _, TxDatastore,
};
use super::datastore::{
    locking_tx_datastore::{
        datastore::Locking,
        state_view::{Iter, IterByColEq, IterByColRange},
    },
    traits::TxData,
};
use super::db_metrics::DB_METRICS;
use super::relational_operators::Relation;
use crate::db::datastore::system_tables::{StModuleRow, WASM_MODULE};
use crate::error::{DBError, DatabaseError, TableError};
use crate::execution_context::ExecutionContext;
use crate::messages::control_db::HostType;
use crate::util::spawn_rayon;
use anyhow::anyhow;
use fs2::FileExt;
use futures::channel::mpsc;
use futures::StreamExt;
use parking_lot::RwLock;
use spacetimedb_commitlog as commitlog;
use spacetimedb_durability::{self as durability, Durability, TxOffset};
use spacetimedb_lib::address::Address;
use spacetimedb_lib::db::auth::{StAccess, StTableType};
use spacetimedb_lib::db::raw_def::{RawColumnDefV8, RawIndexDefV8, RawSequenceDefV8, RawTableDefV8};
use spacetimedb_lib::Identity;
use spacetimedb_primitives::*;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductType, ProductValue};
use spacetimedb_schema::schema::TableSchema;
use spacetimedb_snapshot::{SnapshotError, SnapshotRepository};
use spacetimedb_table::indexes::RowPointer;
use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::ops::RangeBounds;
use std::path::Path;
use std::sync::Arc;

pub type MutTx = <Locking as super::datastore::traits::MutTx>::MutTx;
pub type Tx = <Locking as super::datastore::traits::Tx>::Tx;

type RowCountFn = Arc<dyn Fn(TableId, &str) -> i64 + Send + Sync>;

/// A function to determine the size on disk of the durable state of the
/// local database instance. This is used for metrics and energy accounting
/// purposes.
///
/// It is not part of the [`Durability`] trait because it must report disk
/// usage of the local instance only, even if exclusively remote durability is
/// configured or the database is in follower state.
pub type DiskSizeFn = Arc<dyn Fn() -> io::Result<u64> + Send + Sync>;

pub type Txdata = commitlog::payload::Txdata<ProductValue>;

/// The set of clients considered connected to the database.
///
/// A client is considered connected if there exists a corresponding row in the
/// `st_clients` system table.
///
/// If rows exist in `st_clients` upon [`RelationalDB::open`], the database was
/// not shut down gracefully. Such "dangling" clients should be removed by
/// calling [`crate::host::ModuleHost::call_identity_connected_disconnected`]
/// for each entry in [`ConnectedClients`].
pub type ConnectedClients = HashSet<(Identity, Address)>;

#[derive(Clone)]
pub struct RelationalDB {
    address: Address,
    owner_identity: Identity,

    inner: Locking,
    durability: Option<Arc<dyn Durability<TxData = Txdata>>>,
    snapshot_worker: Option<Arc<SnapshotWorker>>,

    row_count_fn: RowCountFn,
    /// Function to determine the durable size on disk.
    /// `Some` if `durability` is `Some`, `None` otherwise.
    disk_size_fn: Option<DiskSizeFn>,

    // DO NOT ADD FIELDS AFTER THIS.
    // By default, fields are dropped in declaration order.
    // We want to release the file lock last.
    _lock: LockFile,
}

struct SnapshotWorker {
    _handle: tokio::task::JoinHandle<()>,
    /// Send end of the [`Self::snapshot_loop`]'s `trigger` receiver.
    ///
    /// Send a message along this queue to request that the `snapshot_loop` asynchronously capture a snapshot.
    request_snapshot: mpsc::UnboundedSender<()>,
}

impl SnapshotWorker {
    fn new(committed_state: Arc<RwLock<CommittedState>>, repo: Arc<SnapshotRepository>) -> Self {
        let (request_snapshot, trigger) = mpsc::unbounded();
        let handle = tokio::spawn(Self::snapshot_loop(trigger, committed_state, repo));
        SnapshotWorker {
            _handle: handle,
            request_snapshot,
        }
    }

    /// The snapshot loop takes a snapshot after each `trigger` message received.
    async fn snapshot_loop(
        mut trigger: mpsc::UnboundedReceiver<()>,
        committed_state: Arc<RwLock<CommittedState>>,
        repo: Arc<SnapshotRepository>,
    ) {
        while let Some(()) = trigger.next().await {
            let committed_state = committed_state.clone();
            let repo = repo.clone();
            tokio::task::spawn_blocking(move || Self::take_snapshot(&committed_state, &repo))
                .await
                .unwrap();
        }
    }

    fn take_snapshot(committed_state: &RwLock<CommittedState>, snapshot_repo: &SnapshotRepository) {
        let mut committed_state = committed_state.write();
        let Some(tx_offset) = committed_state.next_tx_offset.checked_sub(1) else {
            log::info!("SnapshotWorker::take_snapshot: refusing to take snapshot at tx_offset -1");
            return;
        };
        log::info!(
            "Capturing snapshot of database {:?} at TX offset {}",
            snapshot_repo.database_address(),
            tx_offset,
        );

        let start_time = std::time::Instant::now();

        let CommittedState {
            ref mut tables,
            ref blob_store,
            ..
        } = *committed_state;

        if let Err(e) = snapshot_repo.create_snapshot(tables.values_mut(), blob_store, tx_offset) {
            log::error!(
                "Error capturing snapshot of database {:?}: {e:?}",
                snapshot_repo.database_address()
            );
        } else {
            log::info!(
                "Captured snapshot of database {:?} at TX offset {} in {:?}",
                snapshot_repo.database_address(),
                tx_offset,
                start_time.elapsed()
            );
        }
    }
}

/// Perform a snapshot every `SNAPSHOT_FREQUENCY` transactions.
// TODO(config): Allow DBs to specify how frequently to snapshot.
// TODO(bikeshedding): Snapshot based on number of bytes written to commitlog, not tx offsets.
const SNAPSHOT_FREQUENCY: u64 = 1_000_000;

impl std::fmt::Debug for RelationalDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelationalDB").field("address", &self.address).finish()
    }
}

impl RelationalDB {
    fn new(
        lock: LockFile,
        address: Address,
        owner_identity: Identity,
        inner: Locking,
        durability: Option<(Arc<dyn Durability<TxData = Txdata>>, DiskSizeFn)>,
        snapshot_repo: Option<Arc<SnapshotRepository>>,
    ) -> Self {
        let (durability, disk_size_fn) = durability.unzip();
        let snapshot_worker =
            snapshot_repo.map(|repo| Arc::new(SnapshotWorker::new(inner.committed_state.clone(), repo.clone())));
        Self {
            inner,
            durability,
            snapshot_worker,

            address,
            owner_identity,

            row_count_fn: default_row_count_fn(address),
            disk_size_fn,

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
    /// - `address`
    ///
    ///   The [`Address`] of the database.
    ///
    ///   An error is returned if the database already exists, but has a
    ///   different address.
    ///   If it is a new database, the address is stored in the database's
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
    /// # Return values
    ///
    /// Alongside `Self`, [`ConnectedClients`] is returned, which is the set of
    /// clients considered connected at the given snapshot and `history`.
    ///
    /// If [`ConnectedClients`] is non-empty, the database did not shut down
    /// gracefully. The caller is responsible for disconnecting the clients.
    ///
    /// [ModuleHost]: crate::host::module_host::ModuleHost
    pub fn open(
        root: &Path,
        address: Address,
        owner_identity: Identity,
        history: impl durability::History<TxData = Txdata>,
        durability: Option<(Arc<dyn Durability<TxData = Txdata>>, DiskSizeFn)>,
        snapshot_repo: Option<Arc<SnapshotRepository>>,
    ) -> Result<(Self, ConnectedClients), DBError> {
        log::trace!("[{}] DATABASE: OPEN", address);

        let lock = LockFile::lock(root)?;

        // Check the latest durable TX and restore from a snapshot no newer than it,
        // so that you drop TXes which were committed but not durable before the restart.
        // TODO: delete or mark as invalid snapshots newer than this.
        let durable_tx_offset = durability
            .as_ref()
            .map(|pair| pair.0.clone())
            .as_deref()
            .and_then(|durability| durability.durable_tx_offset());

        log::info!("[{address}] DATABASE: durable_tx_offset is {durable_tx_offset:?}");
        let inner = Self::restore_from_snapshot_or_bootstrap(address, snapshot_repo.as_deref(), durable_tx_offset)?;

        apply_history(&inner, address, history)?;
        let db = Self::new(lock, address, owner_identity, inner, durability, snapshot_repo);

        if let Some(meta) = db.metadata()? {
            if meta.database_address != address {
                return Err(anyhow!("mismatched database address: {} != {}", meta.database_address, address).into());
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

    /// Mark the database as initialized with the given module parameters.
    ///
    /// Records the database's address, owner and module parameters in the
    /// system tables. The transactional context is supplied by the caller.
    ///
    /// It is an error to call this method on an alread-initialized database.
    ///
    /// See [`Self::open`] for further information.
    pub fn set_initialized(&self, tx: &mut MutTx, host_type: HostType, program: Program) -> Result<(), DBError> {
        log::trace!(
            "[{}] DATABASE: set initialized owner={} program_hash={}",
            self.address,
            self.owner_identity,
            program.hash
        );

        // Probably a bug: the database is already initialized.
        // Ignore if it would be a no-op.
        if let Some(meta) = self.inner.metadata_mut_tx(tx)? {
            if program.hash == meta.program_hash
                && self.address == meta.database_address
                && self.owner_identity == meta.owner_identity
            {
                return Ok(());
            }
            return Err(anyhow!("database {} already initialized", self.address).into());
        }
        let row = StModuleRow {
            database_address: self.address,
            owner_identity: self.owner_identity,

            program_kind: match host_type {
                HostType::Wasm => WASM_MODULE,
            },
            program_hash: program.hash,
            program_bytes: program.bytes,
        };
        self.insert(tx, ST_MODULE_ID, row.into()).map(drop)
    }

    /// Obtain the [`Metadata`] of this database.
    ///
    /// `None` if the database is not yet fully initialized.
    pub fn metadata(&self) -> Result<Option<Metadata>, DBError> {
        let ctx = ExecutionContext::internal(self.address);
        self.with_read_only(&ctx, |tx| self.inner.metadata(&ctx, tx))
    }

    /// Obtain the module associated with this database.
    ///
    /// `None` if the database is not yet fully initialized.
    /// Note that a `Some` result may yield an empty slice.
    pub fn program(&self) -> Result<Option<Program>, DBError> {
        let ctx = ExecutionContext::internal(self.address);
        self.with_read_only(&ctx, |tx| self.inner.program(&ctx, tx))
    }

    /// Read the set of clients currently connected to the database.
    pub fn connected_clients(&self) -> Result<ConnectedClients, DBError> {
        let ctx = ExecutionContext::internal(self.address);
        self.with_read_only(&ctx, |tx| self.inner.connected_clients(&ctx, tx)?.collect())
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
        let program_kind = match host_type {
            HostType::Wasm => WASM_MODULE,
        };
        self.inner.update_program(tx, program_kind, program)
    }

    fn restore_from_snapshot_or_bootstrap(
        address: Address,
        snapshot_repo: Option<&SnapshotRepository>,
        durable_tx_offset: Option<TxOffset>,
    ) -> Result<Locking, DBError> {
        if let Some(snapshot_repo) = snapshot_repo {
            if let Some(durable_tx_offset) = durable_tx_offset {
                // Don't restore from a snapshot newer than the `durable_tx_offset`,
                // so that you drop TXes which were committed but not durable before the restart.
                if let Some(tx_offset) = snapshot_repo.latest_snapshot_older_than(durable_tx_offset)? {
                    // Mark any newer snapshots as invalid, as the new history will diverge from their state.
                    snapshot_repo.invalidate_newer_snapshots(durable_tx_offset)?;
                    log::info!("[{address}] DATABASE: restoring snapshot of tx_offset {tx_offset}");
                    let start = std::time::Instant::now();
                    let snapshot = snapshot_repo.read_snapshot(tx_offset)?;
                    log::info!(
                        "[{address}] DATABASE: read snapshot of tx_offset {tx_offset} in {:?}",
                        start.elapsed(),
                    );
                    if snapshot.database_address != address {
                        // TODO: return a proper typed error
                        return Err(anyhow::anyhow!(
                            "Snapshot has incorrect database_address: expected {address} but found {}",
                            snapshot.database_address,
                        )
                        .into());
                    }
                    let start = std::time::Instant::now();
                    let res = Locking::restore_from_snapshot(snapshot);
                    log::info!(
                        "[{address}] DATABASE: restored from snapshot of tx_offset {tx_offset} in {:?}",
                        start.elapsed(),
                    );
                    return res;
                }
            }
            log::info!("[{address}] DATABASE: no snapshot on disk");
        }

        Locking::bootstrap(address)
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
        apply_history(&self.inner, self.address, history)?;
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

    /// Returns the address for this database
    pub fn address(&self) -> Address {
        self.address
    }

    /// The number of bytes on disk occupied by the durability layer.
    ///
    /// If this is an in-memory instance, `Ok(0)` is returned.
    pub fn size_on_disk(&self) -> io::Result<u64> {
        self.disk_size_fn.as_ref().map_or(Ok(0), |f| f())
    }

    pub fn encode_row(row: &ProductValue, bytes: &mut Vec<u8>) {
        // TODO: large file storage of the row elements
        row.encode(bytes);
    }

    pub fn schema_for_table_mut(&self, tx: &MutTx, table_id: TableId) -> Result<Arc<TableSchema>, DBError> {
        self.inner.schema_for_table_mut_tx(tx, table_id)
    }

    pub fn schema_for_table(&self, tx: &Tx, table_id: TableId) -> Result<Arc<TableSchema>, DBError> {
        self.inner.schema_for_table_tx(tx, table_id)
    }

    pub fn row_schema_for_table<'tx>(
        &self,
        tx: &'tx MutTx,
        table_id: TableId,
    ) -> Result<RowTypeForTable<'tx>, DBError> {
        self.inner.row_type_for_table_mut_tx(tx, table_id)
    }

    pub fn get_all_tables_mut(&self, tx: &MutTx) -> Result<Vec<Arc<TableSchema>>, DBError> {
        self.inner
            .get_all_tables_mut_tx(&ExecutionContext::internal(self.address), tx)
    }

    pub fn get_all_tables(&self, tx: &Tx) -> Result<Vec<Arc<TableSchema>>, DBError> {
        self.inner
            .get_all_tables_tx(&ExecutionContext::internal(self.address), tx)
    }

    pub fn is_scheduled_table(
        &self,
        ctx: &ExecutionContext,
        tx: &mut MutTx,
        table_id: TableId,
    ) -> Result<bool, DBError> {
        tx.schema_for_table(ctx, table_id)
            .map(|schema| schema.scheduled.is_some())
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
                return Err(TableError::ColumnNotFound(col_id).into());
            }
            Ok(col_idx)
        };
        let row_ty = &*self.row_schema_for_table(tx, table_id)?;
        let col_idx = check_bounds(row_ty)?;
        let col_ty = &row_ty.elements[col_idx].algebraic_type;
        Ok(AlgebraicValue::decode(col_ty, &mut &*bytes)?)
    }

    /// Begin a transaction.
    ///
    /// **Note**: this call **must** be paired with [`Self::rollback_mut_tx`] or
    /// [`Self::commit_tx`], otherwise the database will be left in an invalid
    /// state. See also [`Self::with_auto_commit`].
    #[tracing::instrument(skip_all)]
    pub fn begin_mut_tx(&self, isolation_level: IsolationLevel) -> MutTx {
        log::trace!("BEGIN MUT TX");
        let r = self.inner.begin_mut_tx(isolation_level);
        log::trace!("ACQUIRED MUT TX");
        r
    }

    #[tracing::instrument(skip_all)]
    pub fn begin_tx(&self) -> Tx {
        log::trace!("BEGIN TX");
        let r = self.inner.begin_tx();
        log::trace!("ACQUIRED TX");
        r
    }

    #[tracing::instrument(skip_all)]
    pub fn rollback_mut_tx(&self, ctx: &ExecutionContext, tx: MutTx) {
        log::trace!("ROLLBACK MUT TX");
        self.inner.rollback_mut_tx(ctx, tx)
    }

    #[tracing::instrument(skip_all)]
    pub fn rollback_mut_tx_downgrade(&self, ctx: &ExecutionContext, tx: MutTx) -> Tx {
        log::trace!("ROLLBACK MUT TX");
        self.inner.rollback_mut_tx_downgrade(ctx, tx)
    }

    #[tracing::instrument(skip_all)]
    pub fn release_tx(&self, ctx: &ExecutionContext, tx: Tx) {
        log::trace!("RELEASE TX");
        self.inner.release_tx(ctx, tx)
    }

    #[tracing::instrument(skip_all)]
    pub fn commit_tx(&self, ctx: &ExecutionContext, tx: MutTx) -> Result<Option<TxData>, DBError> {
        log::trace!("COMMIT MUT TX");

        // TODO: Never returns `None` -- should it?
        let Some(tx_data) = self.inner.commit_mut_tx(ctx, tx)? else {
            return Ok(None);
        };

        self.maybe_do_snapshot(&tx_data);

        if let Some(durability) = &self.durability {
            Self::do_durability(&**durability, ctx, &tx_data)
        }

        Ok(Some(tx_data))
    }

    #[tracing::instrument(skip_all)]
    pub fn commit_tx_downgrade(&self, ctx: &ExecutionContext, tx: MutTx) -> Result<Option<(TxData, Tx)>, DBError> {
        log::trace!("COMMIT MUT TX");

        let Some((tx_data, tx)) = self.inner.commit_mut_tx_downgrade(ctx, tx)? else {
            return Ok(None);
        };

        self.maybe_do_snapshot(&tx_data);

        if let Some(durability) = &self.durability {
            Self::do_durability(&**durability, ctx, &tx_data)
        }

        Ok(Some((tx_data, tx)))
    }

    /// If `(tx_data, ctx)` should be appended to the commitlog, do so.
    ///
    /// Note that by this stage,
    /// [`crate::db::datastore::locking_tx_datastore::committed_state::tx_consumes_offset`]
    /// has already decided based on the reducer and operations whether the transaction should be appended;
    /// this method is responsible only for reading its decision out of the `tx_data`
    /// and calling `durability.append_tx`.
    fn do_durability(durability: &dyn Durability<TxData = Txdata>, ctx: &ExecutionContext, tx_data: &TxData) {
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
            let deletes: Box<_> = tx_data
                .deletes()
                .map(|(table_id, rowdata)| Ops {
                    table_id: *table_id,
                    rowdata: rowdata.clone(),
                })
                .collect();

            let inputs = ctx.reducer_context().map(|rcx| rcx.into());

            let txdata = Txdata {
                inputs,
                outputs: None,
                mutations: Some(Mutations {
                    inserts,
                    deletes,
                    truncates: [].into(),
                }),
            };

            log::trace!("append {txdata:?}");
            // TODO: Should measure queuing time + actual write
            durability.append_tx(txdata);
        } else {
            debug_assert!(tx_data.inserts().all(|(_, inserted_rows)| inserted_rows.is_empty()));
            debug_assert!(tx_data.deletes().all(|(_, deleted_rows)| deleted_rows.is_empty()));
            debug_assert!(!matches!(
                ctx.reducer_context().map(|rcx| rcx.name.strip_prefix("__identity_")),
                Some(Some("connected__" | "disconnected__"))
            ));
        }
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
                    snapshot_worker.request_snapshot.unbounded_send(()).unwrap();
                }
            }
        }
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
    pub fn with_auto_commit<F, A, E>(&self, ctx: &ExecutionContext, f: F) -> Result<A, E>
    where
        F: FnOnce(&mut MutTx) -> Result<A, E>,
        E: From<DBError>,
    {
        let mut tx = self.begin_mut_tx(IsolationLevel::Serializable);
        let res = f(&mut tx);
        self.finish_tx(ctx, tx, res)
    }

    /// Run a fallible function in a transaction, rolling it back if the
    /// function returns `Err`.
    ///
    /// Similar in purpose to [`Self::with_auto_commit`], but returns the
    /// [`MutTx`] alongside the `Ok` result of the function `F` without
    /// committing the transaction.
    pub fn with_auto_rollback<F, A, E>(&self, ctx: &ExecutionContext, mut tx: MutTx, f: F) -> Result<(MutTx, A), E>
    where
        F: FnOnce(&mut MutTx) -> Result<A, E>,
    {
        let res = f(&mut tx);
        self.rollback_on_err(ctx, tx, res)
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
    pub fn with_read_only<F, T>(&self, ctx: &ExecutionContext, f: F) -> T
    where
        F: FnOnce(&mut Tx) -> T,
    {
        let mut tx = self.begin_tx();
        let res = f(&mut tx);
        self.release_tx(ctx, tx);
        res
    }

    /// Perform the transactional logic for the `tx` according to the `res`
    pub fn finish_tx<A, E>(&self, ctx: &ExecutionContext, tx: MutTx, res: Result<A, E>) -> Result<A, E>
    where
        E: From<DBError>,
    {
        if res.is_err() {
            self.rollback_mut_tx(ctx, tx);
        } else {
            match self.commit_tx(ctx, tx).map_err(E::from)? {
                Some(_) => (),
                None => panic!("TODO: retry?"),
            }
        }
        res
    }

    /// Roll back transaction `tx` if `res` is `Err`, otherwise return it
    /// alongside the `Ok` value.
    pub fn rollback_on_err<A, E>(&self, ctx: &ExecutionContext, tx: MutTx, res: Result<A, E>) -> Result<(MutTx, A), E> {
        match res {
            Err(e) => {
                self.rollback_mut_tx(ctx, tx);
                Err(e)
            }
            Ok(a) => Ok((tx, a)),
        }
    }

    pub(crate) fn alter_table_access(&self, tx: &mut MutTx, name: Box<str>, access: StAccess) -> Result<(), DBError> {
        self.inner.alter_table_access_mut_tx(tx, name, access)
    }
}

impl RelationalDB {
    pub fn create_table<T: Into<RawTableDefV8>>(&self, tx: &mut MutTx, schema: T) -> Result<TableId, DBError> {
        self.inner.create_table_mut_tx(tx, schema.into())
    }

    fn col_def_for_test(schema: &[(&str, AlgebraicType)]) -> Vec<RawColumnDefV8> {
        schema
            .iter()
            .cloned()
            .map(|(col_name, col_type)| RawColumnDefV8 {
                col_name: col_name.into(),
                col_type,
            })
            .collect()
    }

    pub fn create_table_for_test_with_access(
        &self,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        indexes: &[(ColId, &str)],
        access: StAccess,
    ) -> Result<TableId, DBError> {
        let indexes = indexes
            .iter()
            .copied()
            .map(|(col_id, index_name)| RawIndexDefV8::btree(index_name.into(), col_id, false))
            .collect();

        let schema = RawTableDefV8::new(name.into(), Self::col_def_for_test(schema))
            .with_indexes(indexes)
            .with_type(StTableType::User)
            .with_access(access);

        self.with_auto_commit(&ExecutionContext::default(), |tx| self.create_table(tx, schema))
    }

    pub fn create_table_for_test(
        &self,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        indexes: &[(ColId, &str)],
    ) -> Result<TableId, DBError> {
        self.create_table_for_test_with_access(name, schema, indexes, StAccess::Public)
    }

    pub fn create_table_for_test_multi_column(
        &self,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        idx_cols: ColList,
    ) -> Result<TableId, DBError> {
        let schema = RawTableDefV8::new(name.into(), Self::col_def_for_test(schema))
            .with_column_index(idx_cols, false)
            .with_type(StTableType::User)
            .with_access(StAccess::Public);

        self.with_auto_commit(&ExecutionContext::default(), |tx| self.create_table(tx, schema))
    }

    pub fn create_table_for_test_mix_indexes(
        &self,
        name: &str,
        schema: &[(&str, AlgebraicType)],
        idx_cols_single: &[(ColId, &str)],
        idx_cols_multi: ColList,
    ) -> Result<TableId, DBError> {
        let idx_cols_single = idx_cols_single
            .iter()
            .copied()
            .map(|(col_id, index_name)| RawIndexDefV8::btree(index_name.into(), col_id, false))
            .collect();

        let schema = RawTableDefV8::new(name.into(), Self::col_def_for_test(schema))
            .with_indexes(idx_cols_single)
            .with_column_index(idx_cols_multi, false)
            .with_type(StTableType::User)
            .with_access(StAccess::Public);

        self.with_auto_commit(&ExecutionContext::default(), |tx| self.create_table(tx, schema))
    }

    pub fn drop_table(&self, ctx: &ExecutionContext, tx: &mut MutTx, table_id: TableId) -> Result<(), DBError> {
        let table_name = self
            .table_name_from_id_mut(ctx, tx, table_id)?
            .map(|name| name.to_string())
            .unwrap_or_default();
        self.inner.drop_table_mut_tx(tx, table_id).map(|_| {
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&self.address, &table_id.into(), &table_name)
                .set(0)
        })
    }

    /// Rename a table.
    ///
    /// Sets the name of the table to `new_name` regardless of the previous value. This is a
    /// relatively cheap operation which only modifies the system tables.
    ///
    /// If the table is not found or is a system table, an error is returned.
    pub fn rename_table(&self, tx: &mut MutTx, table_id: TableId, new_name: &str) -> Result<(), DBError> {
        self.inner.rename_table_mut_tx(tx, table_id, new_name)
    }

    pub fn table_id_from_name_mut(&self, tx: &MutTx, table_name: &str) -> Result<Option<TableId>, DBError> {
        self.inner.table_id_from_name_mut_tx(tx, table_name)
    }

    pub fn table_id_from_name(&self, tx: &Tx, table_name: &str) -> Result<Option<TableId>, DBError> {
        self.inner.table_id_from_name_tx(tx, table_name)
    }

    pub fn table_id_exists(&self, tx: &Tx, table_id: &TableId) -> bool {
        self.inner.table_id_exists_tx(tx, table_id)
    }

    pub fn table_id_exists_mut(&self, tx: &MutTx, table_id: &TableId) -> bool {
        self.inner.table_id_exists_mut_tx(tx, table_id)
    }

    pub fn table_name_from_id<'a>(&'a self, tx: &'a Tx, table_id: TableId) -> Result<Option<Cow<'a, str>>, DBError> {
        self.inner.table_name_from_id_tx(tx, table_id)
    }

    pub fn table_name_from_id_mut<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a MutTx,
        table_id: TableId,
    ) -> Result<Option<Cow<'a, str>>, DBError> {
        self.inner.table_name_from_id_mut_tx(ctx, tx, table_id)
    }

    pub fn table_row_count_mut(&self, tx: &MutTx, table_id: TableId) -> Option<u64> {
        // TODO(Centril): Go via MutTxDatastore trait instead.
        // Doing this for now to ship this quicker.
        tx.table_row_count(table_id)
    }

    pub fn column_constraints(
        &self,
        tx: &mut MutTx,
        table_id: TableId,
        cols: &ColList,
    ) -> Result<Constraints, DBError> {
        let table = self.inner.schema_for_table_mut_tx(tx, table_id)?;

        let unique_index = table.indexes.iter().find(|x| &x.columns == cols).map(|x| x.is_unique);
        let mut attr = Constraints::unset();
        if let Some(is_unique) = unique_index {
            attr = attr.push(Constraints::from_is_unique(is_unique));
        }
        Ok(attr)
    }

    pub fn index_id_from_name(&self, tx: &MutTx, index_name: &str) -> Result<Option<IndexId>, DBError> {
        self.inner.index_id_from_name_mut_tx(tx, index_name)
    }

    pub fn sequence_id_from_name(&self, tx: &MutTx, sequence_name: &str) -> Result<Option<SequenceId>, DBError> {
        self.inner.sequence_id_from_name_mut_tx(tx, sequence_name)
    }

    pub fn constraint_id_from_name(&self, tx: &MutTx, constraint_name: &str) -> Result<Option<ConstraintId>, DBError> {
        self.inner.constraint_id_from_name(tx, constraint_name)
    }

    /// Adds the [index::BTreeIndex] into the [ST_INDEXES_NAME] table
    ///
    /// Returns the `index_id`
    ///
    /// NOTE: It loads the data from the table into it before returning
    pub fn create_index(&self, tx: &mut MutTx, table_id: TableId, index: RawIndexDefV8) -> Result<IndexId, DBError> {
        self.inner.create_index_mut_tx(tx, table_id, index)
    }

    /// Removes the [index::BTreeIndex] from the database by their `index_id`
    pub fn drop_index(&self, tx: &mut MutTx, index_id: IndexId) -> Result<(), DBError> {
        self.inner.drop_index_mut_tx(tx, index_id)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`.
    pub fn iter_mut<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a MutTx,
        table_id: TableId,
    ) -> Result<Iter<'a>, DBError> {
        self.inner.iter_mut_tx(ctx, tx, table_id)
    }

    pub fn iter<'a>(&'a self, ctx: &'a ExecutionContext, tx: &'a Tx, table_id: TableId) -> Result<Iter<'a>, DBError> {
        self.inner.iter_tx(ctx, tx, table_id)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `cols` matches `value`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn iter_by_col_eq_mut<'a, 'r>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a MutTx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<IterByColEq<'a, 'r>, DBError> {
        self.inner.iter_by_col_eq_mut_tx(ctx, tx, table_id.into(), cols, value)
    }

    pub fn iter_by_col_eq<'a, 'r>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Tx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<IterByColEq<'a, 'r>, DBError> {
        self.inner.iter_by_col_eq_tx(ctx, tx, table_id.into(), cols, value)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `cols` matches what is within `range`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn iter_by_col_range_mut<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a MutTx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<IterByColRange<'a, R>, DBError> {
        self.inner
            .iter_by_col_range_mut_tx(ctx, tx, table_id.into(), cols, range)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `cols` matches what is within `range`.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    pub fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Tx,
        table_id: impl Into<TableId>,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<IterByColRange<'a, R>, DBError> {
        self.inner.iter_by_col_range_tx(ctx, tx, table_id.into(), cols, range)
    }

    pub fn insert(&self, tx: &mut MutTx, table_id: TableId, row: ProductValue) -> Result<ProductValue, DBError> {
        self.inner.insert_mut_tx(tx, table_id, row)
    }

    pub fn insert_bytes_as_row(
        &self,
        tx: &mut MutTx,
        table_id: TableId,
        row_bytes: &[u8],
    ) -> Result<ProductValue, DBError> {
        let ty = self.inner.row_type_for_table_mut_tx(tx, table_id)?;
        let row = ProductValue::decode(&ty, &mut &row_bytes[..])?;
        self.insert(tx, table_id, row)
    }

    pub fn delete(&self, tx: &mut MutTx, table_id: TableId, row_ids: impl IntoIterator<Item = RowPointer>) -> u32 {
        self.inner.delete_mut_tx(tx, table_id, row_ids)
    }

    pub fn delete_by_rel<R: Relation>(&self, tx: &mut MutTx, table_id: TableId, relation: R) -> u32 {
        self.inner.delete_by_rel_mut_tx(tx, table_id, relation)
    }

    /// Clear all rows from a table without dropping it.
    pub fn clear_table(&self, tx: &mut MutTx, table_id: TableId) -> Result<(), DBError> {
        let relation = self
            .iter_mut(&ExecutionContext::internal(self.address), tx, table_id)?
            .map(|row_ref| row_ref.pointer())
            .collect::<Vec<_>>();
        self.delete(tx, table_id, relation);
        Ok(())
    }

    /// Generated the next value for the [SequenceId]
    pub fn next_sequence(&self, tx: &mut MutTx, seq_id: SequenceId) -> Result<i128, DBError> {
        self.inner.get_next_sequence_value_mut_tx(tx, seq_id)
    }

    /// Add a [Sequence] into the database instance, generates a stable [SequenceId] for it that will persist on restart.
    pub fn create_sequence(
        &mut self,
        tx: &mut MutTx,
        table_id: TableId,
        seq: RawSequenceDefV8,
    ) -> Result<SequenceId, DBError> {
        self.inner.create_sequence_mut_tx(tx, table_id, seq)
    }

    ///Removes the [Sequence] from database instance
    pub fn drop_sequence(&self, tx: &mut MutTx, seq_id: SequenceId) -> Result<(), DBError> {
        self.inner.drop_sequence_mut_tx(tx, seq_id)
    }

    ///Removes the [Constraints] from database instance
    pub fn drop_constraint(&self, tx: &mut MutTx, constraint_id: ConstraintId) -> Result<(), DBError> {
        self.inner.drop_constraint_mut_tx(tx, constraint_id)
    }
}

#[allow(unused)]
#[derive(Clone)]
struct LockFile {
    path: Arc<Path>,
    lock: Arc<File>,
}

impl LockFile {
    pub fn lock(root: impl AsRef<Path>) -> Result<Self, DBError> {
        fs::create_dir_all(&root)?;
        let path = root.as_ref().join("db.lock");
        let lock = File::create(&path)?;
        lock.try_lock_exclusive()
            .map_err(|e| DatabaseError::DatabasedOpened(root.as_ref().to_path_buf(), e.into()))?;

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

fn apply_history<H>(datastore: &Locking, address: Address, history: H) -> Result<(), DBError>
where
    H: durability::History<TxData = Txdata>,
{
    log::info!("[{}] DATABASE: applying transaction history...", address);

    // TODO: Revisit once we actually replay history suffixes, ie. starting
    // from an offset larger than the history's min offset.
    // TODO: We may want to require that a `tokio::runtime::Handle` is
    // always supplied when constructing a `RelationalDB`. This would allow
    // to spawn a timer task here which just prints the progress periodically
    // in case the history is finite but very long.
    let max_tx_offset = history.max_tx_offset();
    let mut last_logged_percentage = 0;
    let progress = |tx_offset: u64| {
        if let Some(max_tx_offset) = max_tx_offset {
            let percentage = f64::floor((tx_offset as f64 / max_tx_offset as f64) * 100.0) as i32;
            if percentage > last_logged_percentage && percentage % 10 == 0 {
                log::info!("[{}] Loaded {}% ({}/{})", address, percentage, tx_offset, max_tx_offset);
                last_logged_percentage = percentage;
            }
        // Print _something_ even if we don't know what's still ahead.
        } else if tx_offset % 10_000 == 0 {
            log::info!("[{}] Loading transaction {}", address, tx_offset);
        }
    };

    let mut replay = datastore.replay(progress);
    let start = replay.next_tx_offset();
    history
        .fold_transactions_from(start, &mut replay)
        .map_err(anyhow::Error::from)?;
    log::info!("[{}] DATABASE: applied transaction history", address);
    datastore.rebuild_state_after_replay()?;
    log::info!("[{}] DATABASE: rebuilt state after replay", address);

    Ok(())
}

/// Initialize local durability with the default parameters.
///
/// Also returned is a [`DiskSizeFn`] as required by [`RelationalDB::open`].
///
/// Note that this operation can be expensive, as it needs to traverse a suffix
/// of the commitlog.
pub async fn local_durability(db_path: &Path) -> io::Result<(Arc<durability::Local<ProductValue>>, DiskSizeFn)> {
    let commitlog_dir = db_path.join("clog");
    tokio::fs::create_dir_all(&commitlog_dir).await?;
    let rt = tokio::runtime::Handle::current();
    // TODO: Should this better be spawn_blocking?
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

/// Open a [`SnapshotRepository`] at `db_path/snapshots`,
/// configured to store snapshots of the database `database_address`/`database_instance_id`.
pub fn open_snapshot_repo(
    db_path: &Path,
    database_address: Address,
    database_instance_id: u64,
) -> Result<Arc<SnapshotRepository>, Box<SnapshotError>> {
    let snapshot_dir = db_path.join("snapshots");
    std::fs::create_dir_all(&snapshot_dir).map_err(|e| Box::new(SnapshotError::from(e)))?;
    SnapshotRepository::open(snapshot_dir, database_address, database_instance_id)
        .map(Arc::new)
        .map_err(Box::new)
}

fn default_row_count_fn(address: Address) -> RowCountFn {
    Arc::new(move |table_id, table_name| {
        DB_METRICS
            .rdb_num_table_rows
            .with_label_values(&address, &table_id.into(), table_name)
            .get()
    })
}

#[cfg(any(test, feature = "test"))]
pub mod tests_utils {
    use super::*;
    use core::ops::Deref;
    use durability::EmptyHistory;
    use tempfile::TempDir;

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
        tmp_dir: TempDir,
    }

    struct DurableState {
        handle: Arc<durability::Local<ProductValue>>,
        rt: tokio::runtime::Runtime,
    }

    impl TestDB {
        pub const ADDRESS: Address = Address::ZERO;
        pub const OWNER: Identity = Identity::ZERO;

        /// Create a [`TestDB`] which does not store data on disk.
        pub fn in_memory() -> Result<Self, DBError> {
            let dir = TempDir::with_prefix("stdb_test")?;
            let db = Self::in_memory_internal(dir.path())?;
            Ok(Self {
                db,

                durable: None,
                tmp_dir: dir,
            })
        }

        /// Create a [`TestDB`] which stores data in a local commitlog.
        ///
        /// Note that flushing the log is an asynchronous process. [`Self::reopen`]
        /// ensures all data has been flushed to disk before re-opening the
        /// database.
        pub fn durable() -> Result<Self, DBError> {
            let dir = TempDir::with_prefix("stdb_test")?;
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
            // Enter the runtime so that `Self::durable_internal` can spawn a `SnapshotWorker`.
            let _rt = rt.enter();
            let (db, handle) = Self::durable_internal(dir.path(), rt.handle().clone())?;
            let durable = DurableState { handle, rt };

            Ok(Self {
                db,

                durable: Some(durable),
                tmp_dir: dir,
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
                let (db, handle) = Self::durable_internal(self.tmp_dir.path(), rt.handle().clone())?;
                let durable = DurableState { handle, rt };

                Ok(Self {
                    db,
                    durable: Some(durable),
                    ..self
                })
            } else {
                let db = Self::in_memory_internal(self.tmp_dir.path())?;
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
        pub fn path(&self) -> &Path {
            self.tmp_dir.path()
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
            TempDir,
        ) {
            let Self { db, durable, tmp_dir } = self;
            let (durability, rt) = durable
                .map(|DurableState { handle, rt }| (Some(handle), Some(rt)))
                .unwrap_or((None, None));
            (db, durability, rt, tmp_dir)
        }

        fn in_memory_internal(root: &Path) -> Result<RelationalDB, DBError> {
            Self::open_db(root, EmptyHistory::new(), None, None)
        }

        fn durable_internal(
            root: &Path,
            rt: tokio::runtime::Handle,
        ) -> Result<(RelationalDB, Arc<durability::Local<ProductValue>>), DBError> {
            let (local, disk_size_fn) = rt.block_on(local_durability(root))?;
            let history = local.clone();
            let durability = local.clone() as Arc<dyn Durability<TxData = Txdata>>;
            let snapshot_repo = open_snapshot_repo(root, Address::default(), 0)?;
            let db = Self::open_db(root, history, Some((durability, disk_size_fn)), Some(snapshot_repo))?;

            Ok((db, local))
        }

        fn open_db(
            root: &Path,
            history: impl durability::History<TxData = Txdata>,
            durability: Option<(Arc<dyn Durability<TxData = Txdata>>, DiskSizeFn)>,
            snapshot_repo: Option<Arc<SnapshotRepository>>,
        ) -> Result<RelationalDB, DBError> {
            let (db, connected_clients) =
                RelationalDB::open(root, Self::ADDRESS, Self::OWNER, history, durability, snapshot_repo)?;
            debug_assert!(connected_clients.is_empty());
            let db = db.with_row_count(Self::row_count_fn());
            db.with_auto_commit(&ExecutionContext::internal(db.address()), |tx| {
                db.set_initialized(tx, HostType::Wasm, Program::empty())
            })?;
            Ok(db)
        }

        // NOTE: This is important to make compiler tests work.
        fn row_count_fn() -> RowCountFn {
            Arc::new(|_, _| i64::MAX)
        }
    }

    impl Deref for TestDB {
        type Target = RelationalDB;

        fn deref(&self) -> &Self::Target {
            &self.db
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_macros)]

    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::db::datastore::system_tables::{
        system_tables, StConstraintRow, StIndexRow, StSequenceRow, StTableRow, ST_CONSTRAINT_ID, ST_INDEX_ID,
        ST_SEQUENCE_ID, ST_TABLE_ID,
    };
    use crate::db::relational_db::tests_utils::TestDB;
    use crate::error::IndexError;
    use crate::execution_context::ReducerContext;
    use anyhow::bail;
    use bytes::Bytes;
    use commitlog::payload::txdata;
    use commitlog::Commitlog;
    use durability::EmptyHistory;
    use pretty_assertions::assert_eq;
    use spacetimedb_client_api_messages::timestamp::Timestamp;
    use spacetimedb_data_structures::map::IntMap;
    use spacetimedb_lib::db::raw_def::{RawColumnDefV8, RawConstraintDefV8};
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::Identity;
    use spacetimedb_sats::bsatn;
    use spacetimedb_sats::buffer::BufReader;
    use spacetimedb_sats::product;
    use spacetimedb_table::read_column::ReadColumn;
    use spacetimedb_table::table::RowRef;

    fn column(name: &str, ty: AlgebraicType) -> RawColumnDefV8 {
        RawColumnDefV8 {
            col_name: name.into(),
            col_type: ty,
        }
    }

    fn index(name: &str, cols: &[u16]) -> RawIndexDefV8 {
        RawIndexDefV8::btree(name.into(), cols.iter().copied().map(ColId).collect::<ColList>(), false)
    }

    fn table(
        name: &str,
        columns: Vec<RawColumnDefV8>,
        indexes: Vec<RawIndexDefV8>,
        constraints: Vec<RawConstraintDefV8>,
    ) -> RawTableDefV8 {
        RawTableDefV8::new(name.into(), columns)
            .with_indexes(indexes)
            .with_constraints(constraints)
    }

    fn my_table(col_type: AlgebraicType) -> RawTableDefV8 {
        RawTableDefV8::new(
            "MyTable".into(),
            vec![RawColumnDefV8 {
                col_name: "my_col".into(),
                col_type,
            }],
        )
    }

    #[test]
    fn test() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        stdb.commit_tx(&ExecutionContext::default(), tx)?;

        Ok(())
    }

    #[test]
    fn test_open_twice() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        stdb.commit_tx(&ExecutionContext::default(), tx)?;

        match RelationalDB::open(
            stdb.path(),
            Address::ZERO,
            Identity::ZERO,
            EmptyHistory::new(),
            None,
            None,
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

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        let t_id = stdb.table_id_from_name_mut(&tx, "MyTable")?;
        assert_eq!(t_id, Some(table_id));
        Ok(())
    }

    #[test]
    fn test_column_name() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
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

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
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
        let mut rows = stdb
            .iter_mut(&ExecutionContext::default(), tx, table_id)?
            .map(read_first_col)
            .collect::<Vec<T>>();
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
            .iter_by_col_range_mut(&ExecutionContext::default(), tx, table_id, 0, from..)?
            .map(read_first_col)
            .collect::<Vec<T>>();
        rows.sort();
        Ok(rows)
    }

    fn insert_three_i32s(stdb: &RelationalDB, tx: &mut MutTx, table_id: TableId) -> ResultTest<()> {
        for v in [-1, 0, 1] {
            stdb.insert(tx, table_id, product![v])?;
        }
        Ok(())
    }

    #[test]
    fn test_pre_commit() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;

        insert_three_i32s(&stdb, &mut tx, table_id)?;
        assert_eq!(collect_sorted::<i32>(&stdb, &tx, table_id)?, vec![-1, 0, 1]);
        Ok(())
    }

    #[test]
    fn test_post_commit() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);

        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;

        insert_three_i32s(&stdb, &mut tx, table_id)?;
        stdb.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        assert_eq!(collect_sorted::<i32>(&stdb, &tx, table_id)?, vec![-1, 0, 1]);
        Ok(())
    }

    #[test]
    fn test_filter_range_pre_commit() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);

        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        insert_three_i32s(&stdb, &mut tx, table_id)?;
        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i32)?, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn test_filter_range_post_commit() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);

        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;

        insert_three_i32s(&stdb, &mut tx, table_id)?;
        stdb.commit_tx(&ExecutionContext::default(), tx)?;

        let tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i32)?, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn test_create_table_rollback() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);

        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        stdb.rollback_mut_tx(&ExecutionContext::default(), tx);

        let tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let result = stdb.table_id_from_name_mut(&tx, "MyTable")?;
        assert!(
            result.is_none(),
            "Table should not exist, so table_id_from_name should return none"
        );

        let ctx = ExecutionContext::default();

        let result = stdb.table_name_from_id_mut(&ctx, &tx, table_id)?;
        assert!(
            result.is_none(),
            "Table should not exist, so table_name_from_id_mut should return none",
        );
        Ok(())
    }

    #[test]
    fn test_rollback() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();

        let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
        stdb.commit_tx(&ctx, tx)?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        insert_three_i32s(&stdb, &mut tx, table_id)?;
        stdb.rollback_mut_tx(&ctx, tx);

        let tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        assert_eq!(collect_sorted::<i32>(&stdb, &tx, table_id)?, Vec::<i32>::new());
        Ok(())
    }

    fn table_auto_inc() -> RawTableDefV8 {
        my_table(AlgebraicType::I64).with_column_constraint(Constraints::primary_key_auto(), 0)
    }

    #[test]
    fn test_auto_inc() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let schema = table_auto_inc();
        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "seq_MyTable_my_col_primary_key_auto")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![0i64])?;
        stdb.insert(&mut tx, table_id, product![0i64])?;

        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![1, 2]);
        Ok(())
    }

    #[test]
    fn test_auto_inc_disable() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let schema = table_auto_inc();
        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "seq_MyTable_my_col_primary_key_auto")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![5i64])?;
        stdb.insert(&mut tx, table_id, product![6i64])?;

        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![5, 6]);
        Ok(())
    }

    fn table_indexed(is_unique: bool) -> RawTableDefV8 {
        my_table(AlgebraicType::I64).with_indexes(vec![RawIndexDefV8::btree("MyTable_my_col_idx".into(), 0, is_unique)])
    }

    #[test]
    fn test_auto_inc_reload() -> ResultTest<()> {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Trace)
            .format_timestamp(None)
            .is_test(true)
            .try_init();

        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let schema = my_table(AlgebraicType::I64).with_column_sequence(0.into());

        let table_id = stdb.create_table(&mut tx, schema)?;

        let sequence = stdb.sequence_id_from_name(&tx, "seq_MyTable_my_col")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![0i64])?;
        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![1]);

        stdb.commit_tx(&ExecutionContext::default(), tx)?;

        let stdb = stdb.reopen()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        stdb.insert(&mut tx, table_id, product![0i64]).unwrap();

        // Check the second row start after `SEQUENCE_PREALLOCATION_AMOUNT`
        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![1, 4098]);
        Ok(())
    }

    #[test]
    fn test_indexed() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let schema = table_indexed(false);
        let table_id = stdb.create_table(&mut tx, schema)?;

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx")?.is_some(),
            "Index not created"
        );

        stdb.insert(&mut tx, table_id, product![1i64])?;
        stdb.insert(&mut tx, table_id, product![1i64])?;

        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![1]);
        Ok(())
    }

    #[test]
    fn test_row_count() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let schema = my_table(AlgebraicType::I64);
        let table_id = stdb.create_table(&mut tx, schema)?;
        stdb.insert(&mut tx, table_id, product![1i64])?;
        stdb.insert(&mut tx, table_id, product![2i64])?;
        stdb.commit_tx(&ExecutionContext::default(), tx)?;

        let stdb = stdb.reopen()?;
        let tx = stdb.begin_tx();
        assert_eq!(tx.table_row_count(table_id).unwrap(), 2);
        Ok(())
    }

    #[test]
    fn test_unique() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);

        let schema = table_indexed(true);
        let table_id = stdb.create_table(&mut tx, schema).expect("stdb.create_table failed");

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx")
                .expect("index_id_from_name failed")
                .is_some(),
            "Index not created"
        );

        stdb.insert(&mut tx, table_id, product![1i64])
            .expect("stdb.insert failed");
        match stdb.insert(&mut tx, table_id, product![1i64]) {
            Ok(_) => {
                panic!("Allow to insert duplicate row")
            }
            Err(DBError::Index(err)) => match err {
                IndexError::UniqueConstraintViolation { .. } => {}
                err => {
                    panic!("Expected error `UniqueConstraintViolation`, got {err}")
                }
            },
            err => {
                panic!("Expected error `UniqueConstraintViolation`, got {err:?}")
            }
        }

        Ok(())
    }

    #[test]
    fn test_identity() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let schema = table_indexed(true).with_column_constraint(Constraints::identity(), 0);

        let table_id = stdb.create_table(&mut tx, schema)?;

        assert!(
            stdb.index_id_from_name(&tx, "MyTable_my_col_idx")?.is_some(),
            "Index not created"
        );

        let sequence = stdb.sequence_id_from_name(&tx, "seq_MyTable_my_col_identity")?;
        assert!(sequence.is_some(), "Sequence not created");

        stdb.insert(&mut tx, table_id, product![0i64])?;
        stdb.insert(&mut tx, table_id, product![0i64])?;

        assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i64)?, vec![1, 2]);
        Ok(())
    }

    #[test]
    fn test_cascade_drop_table() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let schema = RawTableDefV8::new(
            "MyTable".into(),
            ["col1", "col2", "col3", "col4"]
                .map(|c| RawColumnDefV8 {
                    col_name: c.into(),
                    col_type: AlgebraicType::I64,
                })
                .into(),
        )
        .with_indexes(
            [
                ("MyTable_col1_idx", true),
                ("MyTable_col3_idx", false),
                ("MyTable_col4_idx", true),
            ]
            .map(|(name, unique)| RawIndexDefV8::btree(name.into(), 0, unique))
            .into(),
        )
        .with_sequences(vec![RawSequenceDefV8::for_column("MyTable", "col1", 0.into())])
        .with_constraints(vec![RawConstraintDefV8::for_column(
            "MyTable",
            "col2",
            Constraints::indexed(),
            1,
        )]);

        let ctx = ExecutionContext::default();
        let table_id = stdb.create_table(&mut tx, schema)?;

        let indexes = stdb
            .iter_mut(&ctx, &tx, ST_INDEX_ID)?
            .map(|x| StIndexRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(indexes.len(), 4, "Wrong number of indexes");

        let sequences = stdb
            .iter_mut(&ctx, &tx, ST_SEQUENCE_ID)?
            .map(|x| StSequenceRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(sequences.len(), 1, "Wrong number of sequences");

        let constraints = stdb
            .iter_mut(&ctx, &tx, ST_CONSTRAINT_ID)?
            .map(|x| StConstraintRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(constraints.len(), 4, "Wrong number of constraints");

        stdb.drop_table(&ctx, &mut tx, table_id)?;

        let indexes = stdb
            .iter_mut(&ctx, &tx, ST_INDEX_ID)?
            .map(|x| StIndexRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(indexes.len(), 0, "Wrong number of indexes DROP");

        let sequences = stdb
            .iter_mut(&ctx, &tx, ST_SEQUENCE_ID)?
            .map(|x| StSequenceRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(sequences.len(), 0, "Wrong number of sequences DROP");

        let constraints = stdb
            .iter_mut(&ctx, &tx, ST_CONSTRAINT_ID)?
            .map(|x| StConstraintRow::try_from(x).unwrap())
            .filter(|x| x.table_id == table_id)
            .collect::<Vec<_>>();
        assert_eq!(constraints.len(), 0, "Wrong number of constraints DROP");

        Ok(())
    }

    #[test]
    fn test_rename_table() -> ResultTest<()> {
        let stdb = TestDB::durable()?;

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();

        let table_id = stdb.create_table(&mut tx, table_indexed(true))?;
        stdb.rename_table(&mut tx, table_id, "YourTable")?;
        let table_name = stdb.table_name_from_id_mut(&ctx, &tx, table_id)?;

        assert_eq!(Some("YourTable"), table_name.as_ref().map(Cow::as_ref));
        // Also make sure we've removed the old ST_TABLES_ID row
        let mut n = 0;
        for row in stdb.iter_mut(&ctx, &tx, ST_TABLE_ID)? {
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

        let columns = ["a", "b", "c"].map(|n| column(n, AlgebraicType::U64)).into();

        let indexes = vec![index("0", &[0, 1])];
        let schema = table("t", columns, indexes, vec![]);

        let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let table_id = stdb.create_table(&mut tx, schema)?;

        stdb.insert(&mut tx, table_id, product![0u64, 0u64, 1u64])?;
        stdb.insert(&mut tx, table_id, product![0u64, 1u64, 2u64])?;
        stdb.insert(&mut tx, table_id, product![1u64, 2u64, 2u64])?;

        let cols = col_list![0, 1];
        let value = product![0u64, 1u64].into();

        let ctx = ExecutionContext::default();

        let IterByColEq::Index(mut iter) = stdb.iter_by_col_eq_mut(&ctx, &tx, table_id, cols, &value)? else {
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

    // #[test]
    // fn test_rename_column() -> ResultTest<()> {
    //     let (mut stdb, _tmp_dir) = make_test_db()?;

    //     let mut tx_ = stdb.begin_mut_tx(IsolationLevel::Serializable);
    //     let (tx, stdb) = tx_.get();

    //     let schema = &[("col1", AlgebraicType::U64, ColumnIndexAttribute::Identity)];
    //     let table_id = stdb.create_table(tx, "MyTable", ProductTypeMeta::from_iter(&schema[..1]))?;
    //     let column_id = stdb.column_id_from_name(tx, table_id, "col1")?.unwrap();
    //     stdb.rename_column(tx, table_id, column_id, "id")?;

    //     assert_eq!(Some(column_id), stdb.column_id_from_name(tx, table_id, "id")?);
    //     assert_eq!(None, stdb.column_id_from_name(tx, table_id, "col1")?);

    //     Ok(())
    // }

    #[test]
    /// Test that iteration yields each row only once
    /// in the edge case where a row is committed and has been deleted and re-inserted within the iterating TX.
    fn test_insert_delete_insert_iter() {
        let stdb = TestDB::durable().expect("failed to create TestDB");
        let ctx = ExecutionContext::default();

        let mut initial_tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        let schema =
            RawTableDefV8::from_product("test_table", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
        let table_id = stdb.create_table(&mut initial_tx, schema).expect("create_table failed");

        stdb.commit_tx(&ctx, initial_tx).expect("Commit initial_tx failed");

        // Insert a row and commit it, so the row is in the committed_state.
        let mut insert_tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        stdb.insert(&mut insert_tx, table_id, product!(AlgebraicValue::I32(0)))
            .expect("Insert insert_tx failed");
        stdb.commit_tx(&ctx, insert_tx).expect("Commit insert_tx failed");

        let mut delete_insert_tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
        // Delete the row, so it's in the `delete_tables` of `delete_insert_tx`.
        assert_eq!(
            stdb.delete_by_rel(&mut delete_insert_tx, table_id, [product!(AlgebraicValue::I32(0))]),
            1
        );

        // Insert the row again, so that depending on the datastore internals,
        // it may now be only in the committed_state,
        // or in all three of the committed_state, delete_tables and insert_tables.
        stdb.insert(&mut delete_insert_tx, table_id, product!(AlgebraicValue::I32(0)))
            .expect("Insert delete_insert_tx failed");

        // Iterate over the table and assert that we see the committed-deleted-inserted row only once.
        assert_eq!(
            &stdb
                .iter_mut(&ctx, &delete_insert_tx, table_id)
                .expect("iter delete_insert_tx failed")
                .map(|row_ref| row_ref.to_product_value())
                .collect::<Vec<_>>(),
            &[product!(AlgebraicValue::I32(0))],
        );

        stdb.rollback_mut_tx(&ctx, delete_insert_tx);
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
        let ctx = ExecutionContext::reducer(
            stdb.address(),
            ReducerContext {
                name: "abstract_concrete_proxy_factory_impl".into(),
                caller_identity: Identity::__dummy(),
                caller_address: Address::__DUMMY,
                timestamp,
                arg_bsatn: Bytes::new(),
            },
        );

        let row_ty = ProductType::from([("le_boeuf", AlgebraicType::I32)]);
        let schema = RawTableDefV8::from_product("test_table", row_ty.clone());

        // Create an empty transaction
        {
            let tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            stdb.commit_tx(&ctx, tx).expect("failed to commit empty transaction");
        }

        // Create an empty transaction pretending to be an
        // `__identity_connected__` call.
        {
            let tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            stdb.commit_tx(
                &ExecutionContext::reducer(
                    stdb.address(),
                    ReducerContext {
                        name: "__identity_connected__".into(),
                        caller_identity: Identity::__dummy(),
                        caller_address: Address::__DUMMY,
                        timestamp,
                        arg_bsatn: Bytes::new(),
                    },
                ),
                tx,
            )
            .expect("failed to commit empty __identity_connected__ transaction");
        }

        // Create a non-empty transaction including reducer info
        let table_id = {
            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            let table_id = stdb.create_table(&mut tx, schema).expect("failed to create table");
            stdb.insert(&mut tx, table_id, product!(AlgebraicValue::I32(0)))
                .expect("failed to insert row");
            stdb.commit_tx(&ctx, tx).expect("failed to commit tx");

            table_id
        };

        // Create a non-empty transaction without reducer info, as it would be
        // created by a mutable SQL transaction
        {
            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            stdb.insert(&mut tx, table_id, product!(AlgebraicValue::I32(-42)))
                .expect("failed to insert row");
            stdb.commit_tx(&ExecutionContext::sql(stdb.address()), tx)
                .expect("failed to commit tx");
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
                Commitlog::<()>::open(dir.path().join("clog"), Default::default()).expect("failed to open commitlog");
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
            let reducer_name = input.reducer_name.as_str();
            if i == 0 {
                assert_eq!(reducer_name, "__identity_connected__");
            } else {
                assert_eq!(reducer_name, "abstract_concrete_proxy_factory_impl");
            }
            let mut args = input.reducer_args.as_ref();
            let identity: Identity =
                bsatn::from_reader(&mut args).expect("failed to decode caller identity from reducer args");
            let address: Address =
                bsatn::from_reader(&mut args).expect("failed to decode caller address from reducer args");
            let timestamp1: Timestamp =
                bsatn::from_reader(&mut args).expect("failed to decode timestamp from reducer args");
            assert!(
                args.is_empty(),
                "expected args to be exhausted because nullary args were given"
            );
            assert_eq!(identity, Identity::ZERO);
            assert_eq!(address, Address::ZERO);
            assert_eq!(timestamp1, timestamp);
        }
    }
}
