use super::datastore::locking_tx_datastore::committed_state::CommittedState;
use super::datastore::traits::{
    IsolationLevel, MutProgrammable, MutTx as _, MutTxDatastore, Programmable, RowTypeForTable, Tx as _, TxDatastore,
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
use crate::config::DatabaseConfig;
use crate::error::{DBError, DatabaseError, TableError};
use crate::execution_context::ExecutionContext;
use crate::util::slow::SlowQueryConfig;
use durability::TxOffset;
use fs2::FileExt;
use futures::channel::mpsc;
use futures::StreamExt;
use parking_lot::RwLock;
use spacetimedb_commitlog as commitlog;
use spacetimedb_durability::{self as durability, Durability};
use spacetimedb_lib::address::Address;
use spacetimedb_lib::Identity;
use spacetimedb_primitives::*;
use spacetimedb_sats::db::auth::{StAccess, StTableType};
use spacetimedb_sats::db::def::{ColumnDef, IndexDef, SequenceDef, TableDef, TableSchema};
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductType, ProductValue};
use spacetimedb_snapshot::SnapshotRepository;
use spacetimedb_table::indexes::RowPointer;
use spacetimedb_vm::errors::ErrorVm;
use std::borrow::Cow;
use std::collections::HashSet;
use std::fs::{create_dir_all, File};
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
type DiskSizeFn = Arc<dyn Fn() -> io::Result<u64> + Send + Sync>;

pub type Txdata = commitlog::payload::Txdata<ProductValue>;

/// Clients for which a connect reducer call was found in the [`History`], but
/// no corresponding disconnect.
pub type ConnectedClients = HashSet<(Identity, Address)>;

#[derive(Clone)]
pub struct RelationalDB {
    // TODO(cloutiertyler): This should not be public
    pub(crate) inner: Locking,
    durability: Option<Arc<dyn Durability<TxData = Txdata>>>,
    snapshots: Option<Arc<SnapshotWorker>>,
    address: Address,

    row_count_fn: RowCountFn,
    /// Function to determine the durable size on disk. `Some` if `durability`
    /// is `Some`, `None` otherwise.
    disk_size_fn: Option<DiskSizeFn>,

    config: Arc<RwLock<DatabaseConfig>>,

    // DO NOT ADD FIELDS AFTER THIS.
    // By default, fields are dropped in declaration order.
    // We want to release the file lock last.
    _lock: Arc<File>,
}

struct SnapshotWorker {
    _handle: tokio::task::JoinHandle<()>,
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

    fn take_snapshot(committed_state: &RwLock<CommittedState>, snapshots: &SnapshotRepository) {
        let mut committed_state = committed_state.write();
        let Some(tx_offset) = committed_state.next_tx_offset.checked_sub(1) else {
            log::info!("SnapshotWorker::take_snapshot: refusing to take snapshot at tx_offset -1");
            return;
        };
        log::info!(
            "Capturing snapshot of database {:?} at TX offset {}",
            snapshots.database_address(),
            tx_offset,
        );

        let start_time = std::time::Instant::now();

        let CommittedState {
            ref mut tables,
            ref blob_store,
            ..
        } = *committed_state;

        if let Err(e) = snapshots.create_snapshot(tables.values_mut(), blob_store, tx_offset) {
            log::error!(
                "Error capturing snapshot of database {:?}: {e:?}",
                snapshots.database_address()
            );
        } else {
            log::info!(
                "Captured snapshot of database {:?} at TX offset {} in {:?}",
                snapshots.database_address(),
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
    /// Open a database with local durability.
    ///
    /// This is a convenience constructor which initializes the database with
    /// the [`spacetimedb_durability::Local`] durability implementation.
    ///
    /// The commitlog is expected to be located in the `clog` directory relative
    /// to `root`. If there already exists data in the log, it is replayed into
    /// the database's state via [`Self::apply`].
    ///
    /// The [`tokio::runtime::Handle`] is used to spawn background tasks which
    /// take care of flushing and syncing the log.
    ///
    /// Alongside `Self`, the set of clients who were connected as of the most
    /// recent transaction is returned as a [`ConnectedClients`].
    /// `__disconnect__` should be called for each entry.
    pub fn local(
        root: impl AsRef<Path>,
        rt: tokio::runtime::Handle,
        address: Address,
        instance_id: u64,
    ) -> Result<(Self, ConnectedClients), DBError> {
        let log_dir = root.as_ref().join("clog");
        create_dir_all(&log_dir)?;
        let durability = durability::Local::open(
            log_dir,
            rt,
            durability::local::Options {
                commitlog: commitlog::Options {
                    max_records_in_commit: 1.try_into().unwrap(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .map(Arc::new)?;
        let disk_size_fn = Arc::new({
            let durability = durability.clone();
            move || durability.size_on_disk()
        });

        let snapshot_dir = root.as_ref().join("snapshots");
        create_dir_all(&snapshot_dir)?;
        let snapshots = SnapshotRepository::open(snapshot_dir, address, instance_id).map(Arc::new)?;

        let start = std::time::Instant::now();
        let res =
            Self::open(root, address, Some((durability.clone(), disk_size_fn)), Some(snapshots))?.apply(durability);
        log::info!("[{address}] DATABASE: opened local in {:?}", start.elapsed());

        res
    }

    /// Open a database with root directory `root` and the provided [`Durability`]
    /// implementation.
    ///
    /// Note that this **does not** replay existing state, [`Self::apply`] must
    /// be called explicitly if this is desired.
    pub fn open(
        root: impl AsRef<Path>,
        address: Address,
        durability: Option<(Arc<dyn Durability<TxData = Txdata>>, DiskSizeFn)>,
        snapshots: Option<Arc<SnapshotRepository>>,
    ) -> Result<Self, DBError> {
        create_dir_all(&root)?;

        let lock = File::create(root.as_ref().join("db.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|e| DatabaseError::DatabasedOpened(root.as_ref().to_path_buf(), e.into()))?;

        let (durability, disk_size_fn) = durability.map(|(a, b)| (Some(a), Some(b))).unwrap_or_default();
        // Check the latest durable TX and restore from a snapshot no newer than it,
        // so that you drop TXes which were committed but not durable before the restart.
        // TODO: delete or mark as invalid snapshots newer than this.
        let durable_tx_offset = durability
            .as_deref()
            .and_then(|durability| durability.durable_tx_offset());
        log::info!("[{address}] DATABASE: durable_tx_offset is {durable_tx_offset:?}");
        let inner = Self::restore_from_snapshot_or_bootstrap(address, snapshots.as_deref(), durable_tx_offset)?;

        let row_count_fn: RowCountFn = Arc::new(move |table_id, table_name| {
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&address, &table_id.into(), table_name)
                .get()
        });

        let snapshot_worker =
            snapshots.map(|repo| Arc::new(SnapshotWorker::new(inner.committed_state.clone(), repo.clone())));

        Ok(Self {
            inner,
            durability,
            snapshots: snapshot_worker,
            address,

            row_count_fn,
            disk_size_fn,

            _lock: Arc::new(lock),
            config: Arc::new(RwLock::new(DatabaseConfig::new(SlowQueryConfig::with_defaults(), None))),
        })
    }

    fn restore_from_snapshot_or_bootstrap(
        address: Address,
        snapshots: Option<&SnapshotRepository>,
        durable_tx_offset: Option<TxOffset>,
    ) -> Result<Locking, DBError> {
        if let Some(snapshots) = snapshots {
            if let Some(durable_tx_offset) = durable_tx_offset {
                // Don't restore from a snapshot newer than the `durable_tx_offset`,
                // so that you drop TXes which were committed but not durable before the restart.
                // TODO: delete or mark as invalid snapshots newer than this.
                if let Some(tx_offset) = snapshots.latest_snapshot_older_than(durable_tx_offset)? {
                    log::info!("[{address}] DATABASE: restoring snapshot of tx_offset {tx_offset}");
                    let start = std::time::Instant::now();
                    let snapshot = snapshots.read_snapshot(tx_offset)?;
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

    /// Replay ("fold") the provided [`spacetimedb_durability::History`] onto
    /// the database state.
    ///
    /// Consumes `self` in order to ensure exclusive access, and to prevent use
    /// of the database in case of an incomplete replay.
    /// This restriction may be lifted in the future to allow for "live" followers.
    ///
    /// Alongside `Self`, the set of clients who were connected as of the most
    /// recent transaction is returned as a [`ConnectedClients`].
    /// `__disconnect__` should be called for each entry.
    pub fn apply<T>(self, history: T) -> Result<(Self, ConnectedClients), DBError>
    where
        T: durability::History<TxData = Txdata>,
    {
        log::info!("[{}] DATABASE: applying transaction history...", self.address);

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
                    log::info!(
                        "[{}] Loaded {}% ({}/{})",
                        self.address,
                        percentage,
                        tx_offset,
                        max_tx_offset
                    );
                    last_logged_percentage = percentage;
                }
            // Print _something_ even if we don't know what's still ahead.
            } else if tx_offset % 10_000 == 0 {
                log::info!("[{}] Loading transaction {}", self.address, tx_offset);
            }
        };

        let mut replay = self.inner.replay(progress);
        let start = replay.next_tx_offset();

        history
            .fold_transactions_from(start, &mut replay)
            .map_err(anyhow::Error::from)?;
        log::info!("[{}] DATABASE: applied transaction history", self.address);
        self.inner.rebuild_state_after_replay()?;
        log::info!("[{}] DATABASE: rebuilt state after replay", self.address);
        let connected_clients = replay.into_connected_clients();

        Ok((self, connected_clients))
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
    fn maybe_do_snapshot(&self, tx_data: &TxData) {
        if let Some(snapshot_worker) = &self.snapshots {
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
    pub fn create_table<T: Into<TableDef>>(&self, tx: &mut MutTx, schema: T) -> Result<TableId, DBError> {
        self.inner.create_table_mut_tx(tx, schema.into())
    }

    fn col_def_for_test(schema: &[(&str, AlgebraicType)]) -> Vec<ColumnDef> {
        schema
            .iter()
            .cloned()
            .map(|(col_name, col_type)| ColumnDef {
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
            .map(|(col_id, index_name)| IndexDef::btree(index_name.into(), col_id, false))
            .collect();

        let schema = TableDef::new(name.into(), Self::col_def_for_test(schema))
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
        let schema = TableDef::new(name.into(), Self::col_def_for_test(schema))
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
            .map(|(col_id, index_name)| IndexDef::btree(index_name.into(), col_id, false))
            .collect();

        let schema = TableDef::new(name.into(), Self::col_def_for_test(schema))
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

    pub fn column_constraints(
        &self,
        tx: &mut MutTx,
        table_id: TableId,
        cols: &ColList,
    ) -> Result<Constraints, DBError> {
        let table = self.inner.schema_for_table_mut_tx(tx, table_id)?;

        let unique_index = table.indexes.iter().find(|x| &x.columns == cols).map(|x| x.is_unique);
        let attr = Constraints::unset();

        if let Some(is_unique) = unique_index {
            attr.push(if is_unique {
                Constraints::unique()
            } else {
                Constraints::indexed()
            });
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
    pub fn create_index(&self, tx: &mut MutTx, table_id: TableId, index: IndexDef) -> Result<IndexId, DBError> {
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
        seq: SequenceDef,
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

    /// Retrieve the [`Hash`] of the program (SpacetimeDB module) currently
    /// associated with the database.
    ///
    /// A `None` result indicates that the database is not fully initialized
    /// yet.
    pub fn program_hash(&self, tx: &Tx) -> Result<Option<Hash>, DBError> {
        self.inner.program_hash(tx)
    }

    /// Update the [`Hash`] of the program (SpacetimeDB module) currently
    /// associated with the database.
    ///
    /// The operation runs within the transactional context `tx`.
    ///
    /// The fencing token `fence` must be greater than in any previous
    /// invocations of this method, and is typically obtained from a locking
    /// service.
    ///
    /// The method **MUST** be called within the transaction context which
    /// ensures that any lifecycle reducers (`init`, `update`) are invoked. That
    /// is, an impl of [`crate::host::ModuleInstance`].
    pub(crate) fn set_program_hash(&self, tx: &mut MutTx, fence: u128, hash: Hash) -> Result<(), DBError> {
        self.inner.set_program_hash(tx, fence, hash)
    }

    /// Set a runtime configurations setting of the database
    pub(crate) fn set_config(&self, key: &str, value: AlgebraicValue) -> Result<(), ErrorVm> {
        self.config.write().set_config(key, value)
    }
    /// Read the runtime configurations settings of the database
    pub(crate) fn read_config(&self) -> DatabaseConfig {
        *self.config.read()
    }
}

#[cfg(any(test, feature = "test"))]
pub mod tests_utils {
    use super::*;
    use core::ops::Deref;
    use tempfile::TempDir;

    /// Run `f` in a context with access to a Tokio runtime.
    ///
    /// Required for any test that does [`TestDB::durable`],
    /// which requires a Tokio runtime to spawn a snapshot worker.
    pub fn with_tokio<Res>(f: impl FnOnce() -> Res) -> Res {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .build()
            .unwrap();
        let _handle = runtime.enter();
        f()
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
        tmp_dir: TempDir,
    }

    struct DurableState {
        handle: Arc<durability::Local<ProductValue>>,
        rt: tokio::runtime::Runtime,
    }

    impl TestDB {
        pub const ADDRESS: Address = Address::zero();

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
        ///
        /// Requires access to a Tokio runtime in order to spawn a snapshot worker.
        /// See [`with_tokio`].
        pub fn durable() -> Result<Self, DBError> {
            let dir = TempDir::with_prefix("stdb_test")?;
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
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
            Ok(RelationalDB::open(root, Self::ADDRESS, None, None)?.with_row_count(Self::row_count_fn()))
        }

        fn durable_internal(
            root: &Path,
            rt: tokio::runtime::Handle,
        ) -> Result<(RelationalDB, Arc<durability::Local<ProductValue>>), DBError> {
            let log_dir = root.join("clog");
            create_dir_all(&log_dir)?;

            let handle = durability::Local::open(
                log_dir,
                rt,
                durability::local::Options {
                    commitlog: commitlog::Options {
                        max_records_in_commit: 1.try_into().unwrap(),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .map(Arc::new)?;
            let disk_size_fn = Arc::new({
                let handle = handle.clone();
                move || handle.size_on_disk()
            });

            let snapshot_dir = root.join("snapshots");
            create_dir_all(&snapshot_dir)?;
            let snapshots = SnapshotRepository::open(snapshot_dir, Self::ADDRESS, 0).map(Arc::new)?;

            let (db, connected_clients) = {
                let db = RelationalDB::open(
                    root,
                    Self::ADDRESS,
                    Some((handle.clone(), disk_size_fn)),
                    Some(snapshots),
                )?;
                db.apply(handle.clone())?
            };
            // TODO: Should we be able to handle the non-empty case?
            // `RelationalDB` cannot exist on its own then.
            debug_assert!(connected_clients.is_empty());

            Ok((db.with_row_count(Self::row_count_fn()), handle))
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
        system_tables, StConstraintRow, StIndexRow, StSequenceRow, StTableRow, ST_CONSTRAINTS_ID, ST_INDEXES_ID,
        ST_SEQUENCES_ID, ST_TABLES_ID,
    };
    use crate::db::relational_db::tests_utils::{with_tokio, TestDB};
    use crate::error::IndexError;
    use crate::execution_context::ReducerContext;
    use crate::host::Timestamp;
    use anyhow::bail;
    use bytes::Bytes;
    use commitlog::payload::txdata;
    use commitlog::Commitlog;
    use spacetimedb_data_structures::map::IntMap;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::Identity;
    use spacetimedb_sats::bsatn;
    use spacetimedb_sats::buffer::BufReader;
    use spacetimedb_sats::db::def::{ColumnDef, ConstraintDef};
    use spacetimedb_sats::product;
    use spacetimedb_table::read_column::ReadColumn;
    use spacetimedb_table::table::RowRef;

    fn column(name: &str, ty: AlgebraicType) -> ColumnDef {
        ColumnDef {
            col_name: name.into(),
            col_type: ty,
        }
    }

    fn index(name: &str, cols: &[u32]) -> IndexDef {
        IndexDef::btree(
            name.into(),
            cols.iter()
                .copied()
                .map(ColId)
                .collect::<ColListBuilder>()
                .build()
                .unwrap(),
            false,
        )
    }

    fn table(name: &str, columns: Vec<ColumnDef>, indexes: Vec<IndexDef>, constraints: Vec<ConstraintDef>) -> TableDef {
        TableDef::new(name.into(), columns)
            .with_indexes(indexes)
            .with_constraints(constraints)
    }

    fn my_table(col_type: AlgebraicType) -> TableDef {
        TableDef::new(
            "MyTable".into(),
            vec![ColumnDef {
                col_name: "my_col".into(),
                col_type,
            }],
        )
    }

    #[test]
    fn test() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
            stdb.commit_tx(&ExecutionContext::default(), tx)?;

            Ok(())
        })
    }

    #[test]
    fn test_open_twice() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
            stdb.commit_tx(&ExecutionContext::default(), tx)?;

            match RelationalDB::local(stdb.path(), stdb.runtime().unwrap().clone(), Address::zero(), 0) {
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
        })
    }

    #[test]
    fn test_table_name() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
            let t_id = stdb.table_id_from_name_mut(&tx, "MyTable")?;
            assert_eq!(t_id, Some(table_id));
            Ok(())
        })
    }

    #[test]
    fn test_column_name() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
            let table_id = stdb.table_id_from_name_mut(&tx, "MyTable")?.unwrap();
            let schema = stdb.schema_for_table_mut(&tx, table_id)?;
            let col = schema.columns().iter().find(|x| &*x.col_name == "my_col").unwrap();
            assert_eq!(col.col_pos, 0.into());
            Ok(())
        })
    }

    #[test]
    fn test_create_table_pre_commit() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            let schema = my_table(AlgebraicType::I32);
            stdb.create_table(&mut tx, schema.clone())?;
            let result = stdb.create_table(&mut tx, schema);
            result.expect_err("create_table should error when called twice");
            Ok(())
        })
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
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;

            insert_three_i32s(&stdb, &mut tx, table_id)?;
            assert_eq!(collect_sorted::<i32>(&stdb, &tx, table_id)?, vec![-1, 0, 1]);
            Ok(())
        })
    }

    #[test]
    fn test_post_commit() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);

            let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;

            insert_three_i32s(&stdb, &mut tx, table_id)?;
            stdb.commit_tx(&ExecutionContext::default(), tx)?;

            let tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            assert_eq!(collect_sorted::<i32>(&stdb, &tx, table_id)?, vec![-1, 0, 1]);
            Ok(())
        })
    }

    #[test]
    fn test_filter_range_pre_commit() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);

            let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;
            insert_three_i32s(&stdb, &mut tx, table_id)?;
            assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i32)?, vec![0, 1]);
            Ok(())
        })
    }

    #[test]
    fn test_filter_range_post_commit() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);

            let table_id = stdb.create_table(&mut tx, my_table(AlgebraicType::I32))?;

            insert_three_i32s(&stdb, &mut tx, table_id)?;
            stdb.commit_tx(&ExecutionContext::default(), tx)?;

            let tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            assert_eq!(collect_from_sorted(&stdb, &tx, table_id, 0i32)?, vec![0, 1]);
            Ok(())
        })
    }

    #[test]
    fn test_create_table_rollback() -> ResultTest<()> {
        with_tokio(|| {
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
        })
    }

    #[test]
    fn test_rollback() -> ResultTest<()> {
        with_tokio(|| {
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
        })
    }

    fn table_auto_inc() -> TableDef {
        my_table(AlgebraicType::I64).with_column_constraint(Constraints::primary_key_auto(), 0)
    }

    #[test]
    fn test_auto_inc() -> ResultTest<()> {
        with_tokio(|| {
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
        })
    }

    #[test]
    fn test_auto_inc_disable() -> ResultTest<()> {
        with_tokio(|| {
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
        })
    }

    fn table_indexed(is_unique: bool) -> TableDef {
        my_table(AlgebraicType::I64).with_indexes(vec![IndexDef::btree("MyTable_my_col_idx".into(), 0, is_unique)])
    }

    #[test]
    fn test_auto_inc_reload() -> ResultTest<()> {
        with_tokio(|| {
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
        })
    }

    #[test]
    fn test_indexed() -> ResultTest<()> {
        with_tokio(|| {
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
        })
    }

    #[test]
    fn test_row_count() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            let schema = my_table(AlgebraicType::I64);
            let table_id = stdb.create_table(&mut tx, schema)?;
            stdb.insert(&mut tx, table_id, product![1i64])?;
            stdb.insert(&mut tx, table_id, product![2i64])?;
            stdb.commit_tx(&ExecutionContext::default(), tx)?;

            let stdb = stdb.reopen()?;
            let tx = stdb.begin_tx();
            assert_eq!(tx.get_row_count(table_id).unwrap(), 2);
            Ok(())
        })
    }

    #[test]
    fn test_unique() -> ResultTest<()> {
        with_tokio(|| {
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
        })
    }

    #[test]
    fn test_identity() -> ResultTest<()> {
        with_tokio(|| {
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
        })
    }

    #[test]
    fn test_cascade_drop_table() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            let schema = TableDef::new(
                "MyTable".into(),
                ["col1", "col2", "col3", "col4"]
                    .map(|c| ColumnDef {
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
                .map(|(name, unique)| IndexDef::btree(name.into(), 0, unique))
                .into(),
            )
            .with_sequences(vec![SequenceDef::for_column("MyTable", "col1", 0.into())])
            .with_constraints(vec![ConstraintDef::for_column(
                "MyTable",
                "col2",
                Constraints::indexed(),
                1,
            )]);

            let ctx = ExecutionContext::default();
            let table_id = stdb.create_table(&mut tx, schema)?;

            let indexes = stdb
                .iter_mut(&ctx, &tx, ST_INDEXES_ID)?
                .map(|x| StIndexRow::try_from(x).unwrap())
                .filter(|x| x.table_id == table_id)
                .collect::<Vec<_>>();
            assert_eq!(indexes.len(), 4, "Wrong number of indexes");

            let sequences = stdb
                .iter_mut(&ctx, &tx, ST_SEQUENCES_ID)?
                .map(|x| StSequenceRow::try_from(x).unwrap())
                .filter(|x| x.table_id == table_id)
                .collect::<Vec<_>>();
            assert_eq!(sequences.len(), 1, "Wrong number of sequences");

            let constraints = stdb
                .iter_mut(&ctx, &tx, ST_CONSTRAINTS_ID)?
                .map(|x| StConstraintRow::try_from(x).unwrap())
                .filter(|x| x.table_id == table_id)
                .collect::<Vec<_>>();
            assert_eq!(constraints.len(), 4, "Wrong number of constraints");

            stdb.drop_table(&ctx, &mut tx, table_id)?;

            let indexes = stdb
                .iter_mut(&ctx, &tx, ST_INDEXES_ID)?
                .map(|x| StIndexRow::try_from(x).unwrap())
                .filter(|x| x.table_id == table_id)
                .collect::<Vec<_>>();
            assert_eq!(indexes.len(), 0, "Wrong number of indexes DROP");

            let sequences = stdb
                .iter_mut(&ctx, &tx, ST_SEQUENCES_ID)?
                .map(|x| StSequenceRow::try_from(x).unwrap())
                .filter(|x| x.table_id == table_id)
                .collect::<Vec<_>>();
            assert_eq!(sequences.len(), 0, "Wrong number of sequences DROP");

            let constraints = stdb
                .iter_mut(&ctx, &tx, ST_CONSTRAINTS_ID)?
                .map(|x| StConstraintRow::try_from(x).unwrap())
                .filter(|x| x.table_id == table_id)
                .collect::<Vec<_>>();
            assert_eq!(constraints.len(), 0, "Wrong number of constraints DROP");

            Ok(())
        })
    }

    #[test]
    fn test_rename_table() -> ResultTest<()> {
        with_tokio(|| {
            let stdb = TestDB::durable()?;

            let mut tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            let ctx = ExecutionContext::default();

            let table_id = stdb.create_table(&mut tx, table_indexed(true))?;
            stdb.rename_table(&mut tx, table_id, "YourTable")?;
            let table_name = stdb.table_name_from_id_mut(&ctx, &tx, table_id)?;

            assert_eq!(Some("YourTable"), table_name.as_ref().map(Cow::as_ref));
            // Also make sure we've removed the old ST_TABLES_ID row
            let mut n = 0;
            for row in stdb.iter_mut(&ctx, &tx, ST_TABLES_ID)? {
                let table = StTableRow::try_from(row)?;
                if table.table_id == table_id {
                    n += 1;
                }
            }
            assert_eq!(1, n);

            Ok(())
        })
    }

    #[test]
    fn test_multi_column_index() -> ResultTest<()> {
        with_tokio(|| {
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
        })
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
        with_tokio(|| {
            let stdb = TestDB::durable().expect("failed to create TestDB");
            let ctx = ExecutionContext::default();

            let mut initial_tx = stdb.begin_mut_tx(IsolationLevel::Serializable);
            let schema = TableDef::from_product("test_table", ProductType::from_iter([("my_col", AlgebraicType::I32)]));
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
        })
    }

    #[test]
    fn test_tx_inputs_are_in_the_commitlog() {
        with_tokio(|| {
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
            let schema = TableDef::from_product("test_table", row_ty.clone());

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
                stdb.commit_tx(&ExecutionContext::sql(stdb.address(), Default::default()), tx)
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
                    _reader: &mut R,
                ) -> Result<Self::Row, Self::Error> {
                    bail!("unexpected delete for table: {table_id}")
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
                let clog = Commitlog::<()>::open(dir.path().join("clog"), Default::default())
                    .expect("failed to open commitlog");
                let decoder = Decoder(Rc::clone(&inputs));
                clog.fold_transactions(decoder).unwrap();
            }
            // Just a safeguard so we don't drop the temp dir before this point.
            drop(dir);

            let inputs = Rc::into_inner(inputs).unwrap().into_inner();
            log::debug!("collected inputs: {:?}", inputs.inputs);

            // We should've seen three transactions --
            // the empty one should've been ignored/
            assert_eq!(inputs.num_txs, 3);
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
        })
    }
}
