use super::{
    committed_state::CommittedState,
    mut_tx::MutTxId,
    sequence::SequencesState,
    state_view::{IterByColRangeTx, StateView},
    tx::TxId,
    tx_state::TxState,
};
use crate::execution_context::Workload;
use crate::{
    db::datastore::{
        locking_tx_datastore::state_view::{IterByColRangeMutTx, IterMutTx, IterTx},
        traits::{InsertFlags, UpdateFlags},
    },
    subscription::record_exec_metrics,
};
use crate::{
    db::{
        datastore::{
            system_tables::{
                read_bytes_from_col, read_hash_from_col, read_identity_from_col, system_table_schema, ModuleKind,
                StClientRow, StModuleFields, StModuleRow, StTableFields, ST_CLIENT_ID, ST_MODULE_ID, ST_TABLE_ID,
            },
            traits::{
                DataRow, IsolationLevel, Metadata, MutTx, MutTxDatastore, Program, RowTypeForTable, Tx, TxData,
                TxDatastore,
            },
        },
        db_metrics::DB_METRICS,
    },
    error::{DBError, TableError},
    execution_context::ExecutionContext,
};
use anyhow::{anyhow, Context};
use core::{cell::RefCell, ops::RangeBounds};
use parking_lot::{Mutex, RwLock};
use spacetimedb_commitlog::payload::{txdata, Txdata};
use spacetimedb_durability::TxOffset;
use spacetimedb_lib::{db::auth::StAccess, metrics::ExecutionMetrics};
use spacetimedb_lib::{ConnectionId, Identity};
use spacetimedb_paths::server::SnapshotDirPath;
use spacetimedb_primitives::{ColList, ConstraintId, IndexId, SequenceId, TableId};
use spacetimedb_sats::{bsatn, buffer::BufReader, AlgebraicValue, ProductValue};
use spacetimedb_schema::schema::{IndexSchema, SequenceSchema, TableSchema};
use spacetimedb_snapshot::{ReconstructedSnapshot, SnapshotRepository};
use spacetimedb_table::{
    indexes::RowPointer,
    table::{RowRef, Table},
    MemoryUsage,
};
use std::borrow::Cow;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, DBError>;

/// Struct contains various database states, each protected by
/// their own lock. To avoid deadlocks, it is crucial to acquire these locks
/// in a consistent order throughout the application.
///
/// Lock Acquisition Order:
/// 1. `memory`
/// 2. `committed_state`
/// 3. `sequence_state`
///
/// All locking mechanisms are encapsulated within the struct through local methods.
#[derive(Clone)]
pub struct Locking {
    /// The state of the database up to the point of the last committed transaction.
    pub(crate) committed_state: Arc<RwLock<CommittedState>>,
    /// The state of sequence generation in this database.
    sequence_state: Arc<Mutex<SequencesState>>,
    /// The identity of this database.
    pub(crate) database_identity: Identity,
}

impl MemoryUsage for Locking {
    fn heap_usage(&self) -> usize {
        let Self {
            committed_state,
            sequence_state,
            database_identity,
        } = self;
        std::mem::size_of_val(&**committed_state)
            + committed_state.read().heap_usage()
            + std::mem::size_of_val(&**sequence_state)
            + sequence_state.lock().heap_usage()
            + database_identity.heap_usage()
    }
}

impl Locking {
    pub fn new(database_identity: Identity) -> Self {
        Self {
            committed_state: <_>::default(),
            sequence_state: <_>::default(),
            database_identity,
        }
    }

    /// IMPORTANT! This the most delicate function in the entire codebase.
    /// DO NOT CHANGE UNLESS YOU KNOW WHAT YOU'RE DOING!!!
    pub fn bootstrap(database_identity: Identity) -> Result<Self> {
        log::trace!("DATABASE: BOOTSTRAPPING SYSTEM TABLES...");

        // NOTE! The bootstrapping process does not take plan in a transaction.
        // This is intentional.
        let datastore = Self::new(database_identity);
        let mut commit_state = datastore.committed_state.write_arc();
        // TODO(cloutiertyler): One thing to consider in the future is, should
        // we persist the bootstrap transaction in the message log? My intuition
        // is no, because then if we change the schema of the system tables we
        // would need to migrate that data, whereas since the tables are defined
        // in the code we don't have that issue. We may have other issues though
        // for code that relies on the old schema...

        // Create the system tables and insert information about themselves into
        commit_state.bootstrap_system_tables(database_identity)?;
        // The database tables are now initialized with the correct data.
        // Now we have to build our in memory structures.
        commit_state.build_sequence_state(&mut datastore.sequence_state.lock())?;
        // We don't want to build indexes here; we'll build those later,
        // in `rebuild_state_after_replay`.
        // We actively do not want indexes to exist during replay,
        // as they break replaying TX 0.

        log::trace!("DATABASE:BOOTSTRAPPING SYSTEM TABLES DONE");
        Ok(datastore)
    }

    /// The purpose of this is to rebuild the state of the datastore
    /// after having inserted all of rows from the message log.
    /// This is necessary because, for example, inserting a row into `st_table`
    /// is not equivalent to calling `create_table`.
    /// There may eventually be better way to do this, but this will have to do for now.
    pub fn rebuild_state_after_replay(&self) -> Result<()> {
        let mut committed_state = self.committed_state.write_arc();
        let mut sequence_state = self.sequence_state.lock();
        // `build_missing_tables` must be called before indexes.
        // Honestly this should maybe just be one big procedure.
        // See John Carmack's philosophy on this.
        committed_state.reschema_tables()?;
        committed_state.build_missing_tables()?;
        committed_state.build_indexes()?;
        committed_state.build_sequence_state(&mut sequence_state)?;
        Ok(())
    }

    /// Obtain a [`spacetimedb_commitlog::Decoder`] suitable for replaying a
    /// [`spacetimedb_durability::History`] onto the currently committed state.
    ///
    /// The provided closure will be called for each transaction found in the
    /// history, the parameter is the transaction's offset. The closure is called
    /// _before_ the transaction is applied to the database state.
    pub fn replay<F: FnMut(u64)>(&self, progress: F) -> Replay<F> {
        Replay {
            database_identity: self.database_identity,
            committed_state: self.committed_state.clone(),
            progress: RefCell::new(progress),
        }
    }

    /// Construct a new [`Locking`] datastore containing the state stored in `snapshot`.
    ///
    /// - Construct all the tables referenced by `snapshot`, computing their schemas
    ///   either from known system table schemas or from `st_table` and friends.
    /// - Populate those tables with all rows in `snapshot`.
    /// - Construct a [`HashMapBlobStore`] containing all the large blobs referenced by `snapshot`,
    ///   with reference counts specified in `snapshot`.
    /// - Do [`CommittedState::reset_system_table_schemas`] to fix-up auto_inc IDs in the system tables,
    ///   to ensure those schemas match what [`Self::bootstrap`] would install.
    /// - Notably, **do not** construct indexes or sequences.
    ///   This should be done by [`Self::rebuild_state_after_replay`],
    ///   after replaying the suffix of the commitlog.
    pub fn restore_from_snapshot(snapshot: ReconstructedSnapshot) -> Result<Self> {
        let ReconstructedSnapshot {
            database_identity,
            tx_offset,
            blob_store,
            tables,
            ..
        } = snapshot;

        let datastore = Self::new(database_identity);
        let mut committed_state = datastore.committed_state.write_arc();
        committed_state.blob_store = blob_store;

        // Note that `tables` is a `BTreeMap`, and so iterates in increasing order.
        // This means that we will instantiate and populate the system tables before any user tables.
        for (table_id, pages) in tables {
            let schema = match system_table_schema(table_id) {
                Some(schema) => Arc::new(schema),
                // In this case, `schema_for_table` will never see a cached schema,
                // as the committed state is newly constructed and we have not accessed this schema yet.
                // As such, this call will compute and save the schema from `st_table` and friends.
                None => committed_state.schema_for_table(table_id)?,
            };
            let (table, blob_store) = committed_state.get_table_and_blob_store_or_create(table_id, &schema);
            unsafe {
                // Safety:
                // - The snapshot is uncorrupted because reconstructing it verified its hashes.
                // - The schema in `table` is either derived from the st_table and st_column,
                //   which were restored from the snapshot,
                //   or it is a known schema for a system table.
                // - We trust that the snapshot was consistent when created,
                //   so the layout used in the `pages` must be consistent with the schema.
                table.set_pages(pages, blob_store);
            }

            // Set the `rdb_num_table_rows` metric for the table.
            // NOTE: the `rdb_num_table_rows` metric is used by the query optimizer,
            // and therefore has performance implications and must not be disabled.
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&database_identity, &table_id.0, &schema.table_name)
                .set(table.row_count as i64);

            // Also set the `rdb_table_size` metric for the table.
            let table_size = table.bytes_occupied_overestimate();
            DB_METRICS
                .rdb_table_size
                .with_label_values(&database_identity, &table_id.into(), &schema.table_name)
                .set(table_size as i64);
        }

        // Fix up auto_inc IDs in the cached system table schemas.
        committed_state.reset_system_table_schemas()?;

        // The next TX offset after restoring from a snapshot is one greater than the snapshotted offset.
        committed_state.next_tx_offset = tx_offset + 1;

        Ok(datastore)
    }

    /// Take a snapshot of this [`Locking`] datastore's [`CommittedState`]
    /// and store it in `repo`.
    ///
    /// On success, returns:
    ///
    /// - `None` if the committed state is empty
    ///   (i.e. no transactions have been committed yet)
    ///   and therefore no snapshot was created
    ///
    /// - or `Some` path to the newly created snapshot directory
    ///
    /// Returns an error if [`SnapshotRepository::create_snapshot`] returns an
    /// error.
    pub fn take_snapshot(&self, repo: &SnapshotRepository) -> Result<Option<SnapshotDirPath>> {
        let maybe_offset_and_path = Self::take_snapshot_internal(&self.committed_state, repo)?;
        Ok(maybe_offset_and_path.map(|(_, path)| path))
    }

    pub(crate) fn take_snapshot_internal(
        committed_state: &RwLock<CommittedState>,
        repo: &SnapshotRepository,
    ) -> Result<Option<(TxOffset, SnapshotDirPath)>> {
        let mut committed_state = committed_state.write();
        let Some(tx_offset) = committed_state.next_tx_offset.checked_sub(1) else {
            return Ok(None);
        };

        log::info!(
            "Capturing snapshot of database {:?} at TX offset {}",
            repo.database_identity(),
            tx_offset,
        );

        let CommittedState {
            ref mut tables,
            ref blob_store,
            ..
        } = *committed_state;
        let snapshot_dir = repo.create_snapshot(tables.values_mut(), blob_store, tx_offset)?;

        Ok(Some((tx_offset, snapshot_dir)))
    }

    /// Returns a list over all the currently connected clients,
    /// reading from the `st_clients` system table.
    pub fn connected_clients<'a>(
        &'a self,
        tx: &'a TxId,
    ) -> Result<impl Iterator<Item = Result<(Identity, ConnectionId)>> + 'a> {
        let iter = self.iter_tx(tx, ST_CLIENT_ID)?.map(|row_ref| {
            let row = StClientRow::try_from(row_ref)?;
            Ok((row.identity.0, row.connection_id.0))
        });

        Ok(iter)
    }

    pub(crate) fn alter_table_access_mut_tx(&self, tx: &mut MutTxId, name: Box<str>, access: StAccess) -> Result<()> {
        let table_id = self
            .table_id_from_name_mut_tx(tx, &name)?
            .ok_or_else(|| TableError::NotFound(name.into()))?;

        tx.alter_table_access(table_id, access)
    }
}

impl DataRow for Locking {
    type RowId = RowPointer;
    type RowRef<'a> = RowRef<'a>;

    fn read_table_id(&self, row_ref: Self::RowRef<'_>) -> Result<TableId> {
        Ok(row_ref.read_col(StTableFields::TableId)?)
    }
}

impl Tx for Locking {
    type Tx = TxId;

    fn begin_tx(&self, workload: Workload) -> Self::Tx {
        let timer = Instant::now();

        let committed_state_shared_lock = self.committed_state.read_arc();
        let lock_wait_time = timer.elapsed();
        let ctx = ExecutionContext::with_workload(self.database_identity, workload);
        let metrics = ExecutionMetrics::default();
        Self::Tx {
            committed_state_shared_lock,
            lock_wait_time,
            timer,
            ctx,
            metrics,
        }
    }

    fn release_tx(&self, tx: Self::Tx) {
        tx.release();
    }
}

impl TxDatastore for Locking {
    type IterTx<'a>
        = IterTx<'a>
    where
        Self: 'a;
    type IterByColRangeTx<'a, R: RangeBounds<AlgebraicValue>>
        = IterByColRangeTx<'a, R>
    where
        Self: 'a;
    type IterByColEqTx<'a, 'r>
        = IterByColRangeTx<'a, &'r AlgebraicValue>
    where
        Self: 'a;

    fn iter_tx<'a>(&'a self, tx: &'a Self::Tx, table_id: TableId) -> Result<Self::IterTx<'a>> {
        tx.iter(table_id)
    }

    fn iter_by_col_range_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<Self::IterByColRangeTx<'a, R>> {
        tx.iter_by_col_range(table_id, cols.into(), range)
    }

    fn iter_by_col_eq_tx<'a, 'r>(
        &'a self,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEqTx<'a, 'r>> {
        tx.iter_by_col_eq(table_id, cols, value)
    }

    fn table_id_exists_tx(&self, tx: &Self::Tx, table_id: &TableId) -> bool {
        tx.table_name(*table_id).is_some()
    }

    fn table_id_from_name_tx(&self, tx: &Self::Tx, table_name: &str) -> Result<Option<TableId>> {
        tx.table_id_from_name(table_name)
    }

    fn table_name_from_id_tx<'a>(&'a self, tx: &'a Self::Tx, table_id: TableId) -> Result<Option<Cow<'a, str>>> {
        Ok(tx.table_name(table_id).map(Cow::Borrowed))
    }

    fn schema_for_table_tx(&self, tx: &Self::Tx, table_id: TableId) -> Result<Arc<TableSchema>> {
        tx.schema_for_table(table_id)
    }

    fn get_all_tables_tx(&self, tx: &Self::Tx) -> Result<Vec<Arc<TableSchema>>> {
        self.iter_tx(tx, ST_TABLE_ID)?
            .map(|row_ref| {
                let table_id = row_ref.read_col(StTableFields::TableId)?;
                self.schema_for_table_tx(tx, table_id)
            })
            .collect()
    }

    fn metadata(&self, tx: &Self::Tx) -> Result<Option<Metadata>> {
        self.iter_tx(tx, ST_MODULE_ID)?
            .next()
            .map(metadata_from_row)
            .transpose()
    }

    fn program(&self, tx: &Self::Tx) -> Result<Option<Program>> {
        self.iter_tx(tx, ST_MODULE_ID)?
            .next()
            .map(|row_ref| {
                let hash = read_hash_from_col(row_ref, StModuleFields::ProgramHash)?;
                let bytes = read_bytes_from_col(row_ref, StModuleFields::ProgramBytes)?;
                Ok(Program { hash, bytes })
            })
            .transpose()
    }
}

impl MutTxDatastore for Locking {
    type IterMutTx<'a>
        = IterMutTx<'a>
    where
        Self: 'a;
    type IterByColRangeMutTx<'a, R: RangeBounds<AlgebraicValue>> = IterByColRangeMutTx<'a, R>;
    type IterByColEqMutTx<'a, 'r>
        = IterByColRangeMutTx<'a, &'r AlgebraicValue>
    where
        Self: 'a;

    fn create_table_mut_tx(&self, tx: &mut Self::MutTx, schema: TableSchema) -> Result<TableId> {
        tx.create_table(schema)
    }

    /// This function is used to get the `ProductType` of the rows in a
    /// particular table.  This will be the `ProductType` as viewed through the
    /// lens of the current transaction. Because it is expensive to compute the
    /// `ProductType` in the context of the transaction, we cache the current
    /// `ProductType` as long as you have not made any changes to the schema of
    /// the table for in the current transaction.  If the cache is invalid, we
    /// fallback to computing the `ProductType` from the underlying datastore.
    ///
    /// NOTE: If you change the system tables directly rather than using the
    /// provided functions for altering tables, then the cache may incorrectly
    /// reflect the schema of the table.q
    ///
    /// This function is known to be called quite frequently.
    fn row_type_for_table_mut_tx<'tx>(&self, tx: &'tx Self::MutTx, table_id: TableId) -> Result<RowTypeForTable<'tx>> {
        tx.row_type_for_table(table_id)
    }

    /// IMPORTANT! This function is relatively expensive, and much more
    /// expensive than `row_type_for_table_mut_tx`.  Prefer
    /// `row_type_for_table_mut_tx` if you only need to access the `ProductType`
    /// of the table.
    fn schema_for_table_mut_tx(&self, tx: &Self::MutTx, table_id: TableId) -> Result<Arc<TableSchema>> {
        tx.schema_for_table(table_id)
    }

    /// This function is relatively expensive because it needs to be
    /// transactional, however we don't expect to be dropping tables very often.
    fn drop_table_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId) -> Result<()> {
        tx.drop_table(table_id)
    }

    fn rename_table_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId, new_name: &str) -> Result<()> {
        tx.rename_table(table_id, new_name)
    }

    fn table_id_from_name_mut_tx(&self, tx: &Self::MutTx, table_name: &str) -> Result<Option<TableId>> {
        tx.table_id_from_name(table_name)
    }

    fn table_id_exists_mut_tx(&self, tx: &Self::MutTx, table_id: &TableId) -> bool {
        tx.table_name(*table_id).is_some()
    }

    fn table_name_from_id_mut_tx<'a>(&'a self, tx: &'a Self::MutTx, table_id: TableId) -> Result<Option<Cow<'a, str>>> {
        tx.table_name_from_id(table_id)
            .map(|opt| opt.map(|s| Cow::Owned(s.into())))
    }

    fn create_index_mut_tx(&self, tx: &mut Self::MutTx, index_schema: IndexSchema, is_unique: bool) -> Result<IndexId> {
        tx.create_index(index_schema, is_unique)
    }

    fn drop_index_mut_tx(&self, tx: &mut Self::MutTx, index_id: IndexId) -> Result<()> {
        tx.drop_index(index_id)
    }

    fn index_id_from_name_mut_tx(&self, tx: &Self::MutTx, index_name: &str) -> Result<Option<IndexId>> {
        tx.index_id_from_name(index_name)
    }

    fn get_next_sequence_value_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> Result<i128> {
        tx.get_next_sequence_value(seq_id)
    }

    fn create_sequence_mut_tx(&self, tx: &mut Self::MutTx, sequence_schema: SequenceSchema) -> Result<SequenceId> {
        tx.create_sequence(sequence_schema)
    }

    fn drop_sequence_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> Result<()> {
        tx.drop_sequence(seq_id)
    }

    fn sequence_id_from_name_mut_tx(&self, tx: &Self::MutTx, sequence_name: &str) -> Result<Option<SequenceId>> {
        tx.sequence_id_from_name(sequence_name)
    }

    fn drop_constraint_mut_tx(&self, tx: &mut Self::MutTx, constraint_id: ConstraintId) -> Result<()> {
        tx.drop_constraint(constraint_id)
    }

    fn constraint_id_from_name(&self, tx: &Self::MutTx, constraint_name: &str) -> Result<Option<ConstraintId>> {
        tx.constraint_id_from_name(constraint_name)
    }

    fn iter_mut_tx<'a>(&'a self, tx: &'a Self::MutTx, table_id: TableId) -> Result<Self::IterMutTx<'a>> {
        tx.iter(table_id)
    }

    fn iter_by_col_range_mut_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<Self::IterByColRangeMutTx<'a, R>> {
        tx.iter_by_col_range(table_id, cols.into(), range)
    }

    fn iter_by_col_eq_mut_tx<'a, 'r>(
        &'a self,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEqMutTx<'a, 'r>> {
        tx.iter_by_col_eq(table_id, cols.into(), value)
    }

    fn get_mut_tx<'a>(
        &self,
        tx: &'a Self::MutTx,
        table_id: TableId,
        row_ptr: &'a Self::RowId,
    ) -> Result<Option<Self::RowRef<'a>>> {
        // TODO(perf, deep-integration): Rework this interface so that `row_ptr` can be trusted.
        tx.get(table_id, *row_ptr)
    }

    fn delete_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTx,
        table_id: TableId,
        row_ptrs: impl IntoIterator<Item = Self::RowId>,
    ) -> u32 {
        let mut num_deleted = 0;
        for row_ptr in row_ptrs {
            match tx.delete(table_id, row_ptr) {
                Err(e) => log::error!("delete_mut_tx: {:?}", e),
                Ok(b) => num_deleted += b as u32,
            }
        }
        num_deleted
    }

    fn delete_by_rel_mut_tx(
        &self,
        tx: &mut Self::MutTx,
        table_id: TableId,
        relation: impl IntoIterator<Item = ProductValue>,
    ) -> u32 {
        let mut num_deleted = 0;
        for row in relation {
            match tx.delete_by_row_value(table_id, &row) {
                Err(e) => log::error!("delete_by_rel_mut_tx: {:?}", e),
                Ok(b) => num_deleted += b as u32,
            }
        }
        num_deleted
    }

    fn insert_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTx,
        table_id: TableId,
        row: &[u8],
    ) -> Result<(ColList, RowRef<'a>, InsertFlags)> {
        let (gens, row_ref, insert_flags) = tx.insert::<true>(table_id, row)?;
        Ok((gens, row_ref.collapse(), insert_flags))
    }

    fn update_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTx,
        table_id: TableId,
        index_id: IndexId,
        row: &[u8],
    ) -> Result<(ColList, RowRef<'a>, UpdateFlags)> {
        tx.update(table_id, index_id, row)
    }

    fn metadata_mut_tx(&self, tx: &Self::MutTx) -> Result<Option<Metadata>> {
        tx.iter(ST_MODULE_ID)?.next().map(metadata_from_row).transpose()
    }

    fn update_program(&self, tx: &mut Self::MutTx, program_kind: ModuleKind, program: Program) -> Result<()> {
        let old = tx
            .iter(ST_MODULE_ID)?
            .next()
            .map(|row| {
                let ptr = row.pointer();
                let row = StModuleRow::try_from(row)?;
                Ok::<_, DBError>((ptr, row))
            })
            .transpose()?;
        match old {
            Some((ptr, mut row)) => {
                row.program_kind = program_kind;
                row.program_hash = program.hash;
                row.program_bytes = program.bytes;

                tx.delete(ST_MODULE_ID, ptr)?;
                tx.insert_via_serialize_bsatn(ST_MODULE_ID, &row).map(drop)
            }

            None => Err(anyhow!(
                "database {} improperly initialized: no metadata",
                self.database_identity
            )
            .into()),
        }
    }
}

/// This utility is responsible for recording all transaction metrics.
pub(super) fn record_tx_metrics(
    ctx: &ExecutionContext,
    tx_timer: Instant,
    lock_wait_time: Duration,
    committed: bool,
    tx_data: Option<&TxData>,
    committed_state: Option<&CommittedState>,
    metrics: ExecutionMetrics,
) {
    let workload = &ctx.workload();
    let db = &ctx.database_identity();
    let reducer = ctx.reducer_name();
    let elapsed_time = tx_timer.elapsed();
    let cpu_time = elapsed_time - lock_wait_time;

    let elapsed_time = elapsed_time.as_secs_f64();
    let cpu_time = cpu_time.as_secs_f64();

    // Increment tx counter
    DB_METRICS
        .rdb_num_txns
        .with_label_values(workload, db, reducer, &committed)
        .inc();
    // Record tx cpu time
    DB_METRICS
        .rdb_txn_cpu_time_sec
        .with_label_values(workload, db, reducer)
        .observe(cpu_time);
    // Record tx elapsed time
    DB_METRICS
        .rdb_txn_elapsed_time_sec
        .with_label_values(workload, db, reducer)
        .observe(elapsed_time);

    record_exec_metrics(workload, db, metrics);

    /// Update table rows and table size gauges,
    /// and sets them to zero if no table is present.
    fn update_table_gauges(db: &Identity, table_id: &TableId, table_name: &str, table: Option<&Table>) {
        let (mut table_rows, mut table_size) = (0, 0);
        if let Some(table) = table {
            table_rows = table.row_count as i64;
            table_size = table.bytes_occupied_overestimate() as i64;
        }
        DB_METRICS
            .rdb_num_table_rows
            .with_label_values(db, &table_id.0, table_name)
            .set(table_rows);
        DB_METRICS
            .rdb_table_size
            .with_label_values(db, &table_id.0, table_name)
            .set(table_size);
    }

    if let (Some(tx_data), Some(committed_state)) = (tx_data, committed_state) {
        for (table_id, table_name, inserts) in tx_data.inserts_with_table_name() {
            let table = committed_state.get_table(*table_id);
            let num_indexes = table.map(|t| t.indexes.len()).unwrap_or(0) as u64;

            update_table_gauges(db, table_id, table_name, table);
            // Increment rows inserted counter
            DB_METRICS
                .rdb_num_rows_inserted
                .with_label_values(workload, db, reducer, &table_id.0, table_name)
                .inc_by(inserts.len() as u64);
            // We don't have sparse indexes, so we can just multiply by the number of indexes.
            if num_indexes > 0 {
                // Increment index rows inserted counter
                DB_METRICS
                    .rdb_num_index_entries_inserted
                    .with_label_values(workload, db, reducer, &table_id.0, table_name)
                    .inc_by((inserts.len() as u64) * num_indexes);
            }
        }
        for (table_id, table_name, deletes) in tx_data.deletes_with_table_name() {
            let table = committed_state.get_table(*table_id);
            let num_indexes = table.map(|t| t.indexes.len()).unwrap_or(0) as u64;
            update_table_gauges(db, table_id, table_name, table);
            // Increment rows deleted counter
            DB_METRICS
                .rdb_num_rows_deleted
                .with_label_values(workload, db, reducer, &table_id.0, table_name)
                .inc_by(deletes.len() as u64);
            // We don't have sparse indexes, so we can just multiply by the number of indexes.
            if num_indexes > 0 {
                // Increment index rows inserted counter
                DB_METRICS
                    .rdb_num_index_entries_deleted
                    .with_label_values(workload, db, reducer, &table_id.0, table_name)
                    .inc_by((deletes.len() as u64) * num_indexes);
            }
        }
    }

    if let Some(committed_state) = committed_state {
        // TODO(cleanliness,bikeshedding): Consider inlining `report_data_size` here,
        // or moving the above metric writes into it, for consistency of organization.
        committed_state.report_data_size(*db);
    }
}

impl MutTx for Locking {
    type MutTx = MutTxId;

    /// Note: We do not use the isolation level here because this implementation
    /// guarantees the highest isolation level, Serializable.
    fn begin_mut_tx(&self, _isolation_level: IsolationLevel, workload: Workload) -> Self::MutTx {
        let timer = Instant::now();

        let committed_state_write_lock = self.committed_state.write_arc();
        let sequence_state_lock = self.sequence_state.lock_arc();
        let lock_wait_time = timer.elapsed();
        let ctx = ExecutionContext::with_workload(self.database_identity, workload);
        MutTxId {
            committed_state_write_lock,
            sequence_state_lock,
            tx_state: TxState::default(),
            lock_wait_time,
            timer,
            ctx,
            metrics: ExecutionMetrics::default(),
        }
    }

    fn rollback_mut_tx(&self, tx: Self::MutTx) {
        tx.rollback();
    }

    fn commit_mut_tx(&self, tx: Self::MutTx) -> Result<Option<TxData>> {
        Ok(Some(tx.commit()))
    }
}

impl Locking {
    pub fn rollback_mut_tx_downgrade(&self, tx: MutTxId, workload: Workload) -> TxId {
        tx.rollback_downgrade(workload)
    }

    pub fn commit_mut_tx_downgrade(&self, tx: MutTxId, workload: Workload) -> Result<Option<(TxData, TxId)>> {
        Ok(Some(tx.commit_downgrade(workload)))
    }
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("Expected tx offset {expected}, encountered {encountered}")]
    InvalidOffset { expected: u64, encountered: u64 },
    #[error(transparent)]
    Decode(#[from] bsatn::DecodeError),
    #[error(transparent)]
    Db(#[from] DBError),
    #[error(transparent)]
    Any(#[from] anyhow::Error),
}

/// A [`spacetimedb_commitlog::Decoder`] suitable for replaying a transaction
/// history into the database state.
pub struct Replay<F> {
    database_identity: Identity,
    committed_state: Arc<RwLock<CommittedState>>,
    progress: RefCell<F>,
}

impl<F> Replay<F> {
    fn using_visitor<T>(&self, f: impl FnOnce(&mut ReplayVisitor<F>) -> T) -> T {
        let mut committed_state = self.committed_state.write_arc();
        let mut visitor = ReplayVisitor {
            database_identity: &self.database_identity,
            committed_state: &mut committed_state,
            progress: &mut *self.progress.borrow_mut(),
        };
        f(&mut visitor)
    }

    pub fn next_tx_offset(&self) -> u64 {
        self.committed_state.read_arc().next_tx_offset
    }
}

impl<F: FnMut(u64)> spacetimedb_commitlog::Decoder for Replay<F> {
    type Record = Txdata<ProductValue>;
    type Error = txdata::DecoderError<ReplayError>;

    fn decode_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        tx_offset: u64,
        reader: &mut R,
    ) -> std::result::Result<Self::Record, Self::Error> {
        self.using_visitor(|visitor| txdata::decode_record_fn(visitor, version, tx_offset, reader))
    }

    fn consume_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        tx_offset: u64,
        reader: &mut R,
    ) -> std::result::Result<(), Self::Error> {
        self.using_visitor(|visitor| txdata::consume_record_fn(visitor, version, tx_offset, reader))
    }

    fn skip_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        _tx_offset: u64,
        reader: &mut R,
    ) -> std::result::Result<(), Self::Error> {
        self.using_visitor(|visitor| txdata::skip_record_fn(visitor, version, reader))
    }
}

impl<F: FnMut(u64)> spacetimedb_commitlog::Decoder for &mut Replay<F> {
    type Record = txdata::Txdata<ProductValue>;
    type Error = txdata::DecoderError<ReplayError>;

    #[inline]
    fn decode_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        tx_offset: u64,
        reader: &mut R,
    ) -> std::result::Result<Self::Record, Self::Error> {
        spacetimedb_commitlog::Decoder::decode_record(&**self, version, tx_offset, reader)
    }

    fn skip_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        tx_offset: u64,
        reader: &mut R,
    ) -> std::result::Result<(), Self::Error> {
        spacetimedb_commitlog::Decoder::skip_record(&**self, version, tx_offset, reader)
    }
}

// n.b. (Tyler) We actually **do not** want to check constraints at replay
// time because not only is it a pain, but actually **subtly wrong** the
// way we have it implemented. It's wrong because the actual constraints of
// the database may change as different transactions are added to the
// schema and you would actually have to change your indexes and
// constraints as you replayed the log. This we are not currently doing
// (we're building all the non-bootstrapped indexes at the end after
// replaying), and thus aren't implementing constraint checking correctly
// as it stands.
//
// However, the above is all rendered moot anyway because we don't need to
// check constraints while replaying if we just assume that they were all
// checked prior to the transaction committing in the first place.
//
// Note also that operation/mutation ordering **does not** matter for
// operations inside a transaction of the message log assuming we only ever
// insert **OR** delete a unique row in one transaction. If we ever insert
// **AND** delete then order **does** matter. The issue caused by checking
// constraints for each operation while replaying does not imply that order
// matters. Ordering of operations would **only** matter if you wanted to
// view the state of the database as of a partially applied transaction. We
// never actually want to do this, because after a transaction has been
// committed, it is assumed that all operations happen instantaneously and
// atomically at the timestamp of the transaction. The only time that we
// actually want to view the state of a database while a transaction is
// partially applied is while the transaction is running **before** it
// commits. Thus, we only care about operation ordering while the
// transaction is running, but we do not care about it at all in the
// context of the commit log.
//
// Not caring about the order in the log, however, requires that we **do
// not** check index constraints during replay of transaction operatoins.
// We **could** check them in between transactions if we wanted to update
// the indexes and constraints as they changed during replay, but that is
// unnecessary.

struct ReplayVisitor<'a, F> {
    database_identity: &'a Identity,
    committed_state: &'a mut CommittedState,
    progress: &'a mut F,
}

impl<F: FnMut(u64)> spacetimedb_commitlog::payload::txdata::Visitor for ReplayVisitor<'_, F> {
    type Error = ReplayError;
    // NOTE: Technically, this could be `()` if and when we can extract the
    // row data without going through `ProductValue` (PV).
    // To accomodate auxiliary traversals (e.g. for analytics), we may want to
    // provide a separate visitor yielding PVs.
    type Row = ProductValue;

    fn skip_row<'a, R: BufReader<'a>>(
        &mut self,
        table_id: TableId,
        reader: &mut R,
    ) -> std::result::Result<(), Self::Error> {
        let schema = self.committed_state.schema_for_table(table_id)?;
        ProductValue::decode(schema.get_row_type(), reader)?;
        Ok(())
    }

    fn visit_insert<'a, R: BufReader<'a>>(
        &mut self,
        table_id: TableId,
        reader: &mut R,
    ) -> std::result::Result<Self::Row, Self::Error> {
        let schema = self.committed_state.schema_for_table(table_id)?;
        let row = ProductValue::decode(schema.get_row_type(), reader)?;

        self.committed_state
            .replay_insert(table_id, &schema, &row)
            .with_context(|| {
                format!(
                    "Error inserting row {:?} during transaction {:?} playback",
                    row, self.committed_state.next_tx_offset
                )
            })?;
        // NOTE: the `rdb_num_table_rows` metric is used by the query optimizer,
        // and therefore has performance implications and must not be disabled.
        DB_METRICS
            .rdb_num_table_rows
            .with_label_values(self.database_identity, &table_id.into(), &schema.table_name)
            .inc();

        Ok(row)
    }

    fn visit_delete<'a, R: BufReader<'a>>(
        &mut self,
        table_id: TableId,
        reader: &mut R,
    ) -> std::result::Result<Self::Row, Self::Error> {
        let schema = self.committed_state.schema_for_table(table_id)?;
        // TODO: avoid clone
        let table_name = schema.table_name.clone();
        let row = ProductValue::decode(schema.get_row_type(), reader)?;

        self.committed_state
            .replay_delete_by_rel(table_id, &row)
            .with_context(|| {
                format!(
                    "Error deleting row {:?} during transaction {:?} playback",
                    row, self.committed_state.next_tx_offset
                )
            })?;
        // NOTE: the `rdb_num_table_rows` metric is used by the query optimizer,
        // and therefore has performance implications and must not be disabled.
        DB_METRICS
            .rdb_num_table_rows
            .with_label_values(self.database_identity, &table_id.into(), &table_name)
            .dec();

        Ok(row)
    }

    fn visit_truncate(&mut self, _table_id: TableId) -> std::result::Result<(), Self::Error> {
        Err(anyhow!("visit: truncate not yet supported").into())
    }

    fn visit_tx_start(&mut self, offset: u64) -> std::result::Result<(), Self::Error> {
        // The first transaction in a history must have the same offset as the
        // committed state.
        //
        // (Technically, the history should guarantee that all subsequent
        // transaction offsets are contiguous, but we don't currently have a
        // good way to only check the first transaction.)
        //
        // Note that this is not a panic, because the starting offset can be
        // chosen at runtime.
        if offset != self.committed_state.next_tx_offset {
            return Err(ReplayError::InvalidOffset {
                expected: self.committed_state.next_tx_offset,
                encountered: offset,
            });
        }
        (self.progress)(offset);

        Ok(())
    }

    fn visit_tx_end(&mut self) -> std::result::Result<(), Self::Error> {
        self.committed_state.next_tx_offset += 1;

        Ok(())
    }
}

/// Construct a [`Metadata`] from the given [`RowRef`],
/// reading only the columns necessary to construct the value.
fn metadata_from_row(row: RowRef<'_>) -> Result<Metadata> {
    Ok(Metadata {
        database_identity: read_identity_from_col(row, StModuleFields::DatabaseIdentity)?,
        owner_identity: read_identity_from_col(row, StModuleFields::OwnerIdentity)?,
        program_hash: read_hash_from_col(row, StModuleFields::ProgramHash)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::datastore::system_tables::{
        system_tables, StColumnRow, StConstraintData, StConstraintFields, StConstraintRow, StIndexAlgorithm,
        StIndexFields, StIndexRow, StRowLevelSecurityFields, StScheduledFields, StSequenceFields, StSequenceRow,
        StTableRow, StVarFields, ST_CLIENT_NAME, ST_COLUMN_ID, ST_COLUMN_NAME, ST_CONSTRAINT_ID, ST_CONSTRAINT_NAME,
        ST_INDEX_ID, ST_INDEX_NAME, ST_MODULE_NAME, ST_RESERVED_SEQUENCE_RANGE, ST_ROW_LEVEL_SECURITY_ID,
        ST_ROW_LEVEL_SECURITY_NAME, ST_SCHEDULED_ID, ST_SCHEDULED_NAME, ST_SEQUENCE_ID, ST_SEQUENCE_NAME,
        ST_TABLE_NAME, ST_VAR_ID, ST_VAR_NAME,
    };
    use crate::db::datastore::traits::{IsolationLevel, MutTx};
    use crate::db::datastore::Result;
    use crate::error::{DBError, IndexError};
    use bsatn::to_vec;
    use core::{fmt, mem};
    use itertools::Itertools;
    use pretty_assertions::{assert_eq, assert_matches};
    use spacetimedb_execution::Datastore;
    use spacetimedb_lib::bsatn::ToBsatn;
    use spacetimedb_lib::db::auth::{StAccess, StTableType};
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::st_var::StVarValue;
    use spacetimedb_lib::{resolved_type_via_v9, ScheduleAt, TimeDuration};
    use spacetimedb_primitives::{col_list, ColId, ColSet, ScheduleId};
    use spacetimedb_sats::algebraic_value::ser::value_serialize;
    use spacetimedb_sats::{product, AlgebraicType, GroundSpacetimeType};
    use spacetimedb_schema::def::{BTreeAlgorithm, ConstraintData, IndexAlgorithm, UniqueConstraintData};
    use spacetimedb_schema::schema::{
        ColumnSchema, ConstraintSchema, IndexSchema, RowLevelSecuritySchema, ScheduleSchema, SequenceSchema,
    };
    use spacetimedb_table::table::UniqueConstraintViolation;

    /// For the first user-created table, sequences in the system tables start
    /// from this value.
    const FIRST_NON_SYSTEM_ID: u32 = ST_RESERVED_SEQUENCE_RANGE + 1;

    /// Utility to query the system tables and return their concrete table row
    pub struct SystemTableQuery<'a> {
        db: &'a MutTxId,
    }

    fn query_st_tables(tx: &MutTxId) -> SystemTableQuery<'_> {
        SystemTableQuery { db: tx }
    }

    impl SystemTableQuery<'_> {
        pub fn scan_st_tables(&self) -> Result<Vec<StTableRow>> {
            Ok(self
                .db
                .iter(ST_TABLE_ID)?
                .map(|row| StTableRow::try_from(row).unwrap())
                .sorted_by_key(|x| x.table_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_tables_by_col(
            &self,
            cols: impl Into<ColList>,
            value: &AlgebraicValue,
        ) -> Result<Vec<StTableRow>> {
            Ok(self
                .db
                .iter_by_col_eq(ST_TABLE_ID, cols.into(), value)?
                .map(|row| StTableRow::try_from(row).unwrap())
                .sorted_by_key(|x| x.table_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_columns(&self) -> Result<Vec<StColumnRow>> {
            Ok(self
                .db
                .iter(ST_COLUMN_ID)?
                .map(|row| StColumnRow::try_from(row).unwrap())
                .sorted_by_key(|x| (x.table_id, x.col_pos))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_columns_by_col(
            &self,
            cols: impl Into<ColList>,
            value: &AlgebraicValue,
        ) -> Result<Vec<StColumnRow>> {
            Ok(self
                .db
                .iter_by_col_eq(ST_COLUMN_ID, cols.into(), value)?
                .map(|row| StColumnRow::try_from(row).unwrap())
                .sorted_by_key(|x| (x.table_id, x.col_pos))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_constraints(&self) -> Result<Vec<StConstraintRow>> {
            Ok(self
                .db
                .iter(ST_CONSTRAINT_ID)?
                .map(|row| StConstraintRow::try_from(row).unwrap())
                .sorted_by_key(|x| x.constraint_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_sequences(&self) -> Result<Vec<StSequenceRow>> {
            Ok(self
                .db
                .iter(ST_SEQUENCE_ID)?
                .map(|row| StSequenceRow::try_from(row).unwrap())
                .sorted_by_key(|x| (x.table_id, x.sequence_id))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_indexes(&self) -> Result<Vec<StIndexRow>> {
            Ok(self
                .db
                .iter(ST_INDEX_ID)?
                .map(|row| StIndexRow::try_from(row).unwrap())
                .sorted_by_key(|x| x.index_id)
                .collect::<Vec<_>>())
        }
    }

    fn u32_str_u32(a: u32, b: &str, c: u32) -> ProductValue {
        product![a, b, c]
    }

    fn get_datastore() -> Result<Locking> {
        Locking::bootstrap(Identity::ZERO)
    }

    fn col(col: u16) -> ColList {
        col.into()
    }

    fn map_array<A, B: From<A>, const N: usize>(a: [A; N]) -> Vec<B> {
        map_array_fn(a, Into::into)
    }

    fn map_array_fn<A, B, F: Fn(A) -> B, const N: usize>(a: [A; N], f: F) -> Vec<B> {
        a.map(f).into()
    }

    struct IndexRow<'a> {
        id: u32,
        table: u32,
        col: ColList,
        name: &'a str,
    }
    impl From<IndexRow<'_>> for StIndexRow {
        fn from(value: IndexRow<'_>) -> Self {
            Self {
                index_id: value.id.into(),
                table_id: value.table.into(),
                index_name: value.name.into(),
                index_algorithm: StIndexAlgorithm::BTree { columns: value.col },
            }
        }
    }
    impl From<IndexRow<'_>> for IndexSchema {
        fn from(value: IndexRow<'_>) -> Self {
            let st = StIndexRow::from(value);
            st.into()
        }
    }

    struct TableRow<'a> {
        id: u32,
        name: &'a str,
        ty: StTableType,
        access: StAccess,
        primary_key: Option<ColId>,
    }
    impl From<TableRow<'_>> for StTableRow {
        fn from(value: TableRow<'_>) -> Self {
            Self {
                table_id: value.id.into(),
                table_name: value.name.into(),
                table_type: value.ty,
                table_access: value.access,
                table_primary_key: value.primary_key.map(ColList::new),
            }
        }
    }

    struct ColRow<'a> {
        table: u32,
        pos: u16,
        name: &'a str,
        ty: AlgebraicType,
    }
    impl From<ColRow<'_>> for StColumnRow {
        fn from(value: ColRow<'_>) -> Self {
            Self {
                table_id: value.table.into(),
                col_pos: value.pos.into(),
                col_name: value.name.into(),
                col_type: value.ty.into(),
            }
        }
    }
    impl From<ColRow<'_>> for ColumnSchema {
        fn from(value: ColRow<'_>) -> Self {
            Self {
                table_id: value.table.into(),
                col_pos: value.pos.into(),
                col_name: value.name.into(),
                col_type: value.ty,
            }
        }
    }

    struct SequenceRow<'a> {
        id: u32,
        name: &'a str,
        table: u32,
        col_pos: u16,
        start: i128,
    }
    impl From<SequenceRow<'_>> for StSequenceRow {
        fn from(value: SequenceRow<'_>) -> Self {
            Self {
                sequence_id: value.id.into(),
                sequence_name: value.name.into(),
                table_id: value.table.into(),
                col_pos: value.col_pos.into(),
                increment: 1,
                start: value.start,
                min_value: 1,
                max_value: i128::MAX,
                allocated: 0,
            }
        }
    }

    impl From<SequenceRow<'_>> for SequenceSchema {
        fn from(value: SequenceRow<'_>) -> Self {
            Self {
                sequence_id: value.id.into(),
                sequence_name: value.name.into(),
                table_id: value.table.into(),
                col_pos: value.col_pos.into(),
                increment: 1,
                start: value.start,
                min_value: 1,
                max_value: i128::MAX,
                allocated: 0,
            }
        }
    }

    struct ConstraintRow<'a> {
        constraint_id: u32,
        constraint_name: &'a str,
        table_id: u32,
        unique_columns: ColList,
    }
    impl From<ConstraintRow<'_>> for StConstraintRow {
        fn from(value: ConstraintRow<'_>) -> Self {
            Self {
                constraint_id: value.constraint_id.into(),
                constraint_name: value.constraint_name.into(),
                table_id: value.table_id.into(),
                constraint_data: StConstraintData::Unique {
                    columns: value.unique_columns.into(),
                },
            }
        }
    }

    impl From<ConstraintRow<'_>> for ConstraintSchema {
        fn from(value: ConstraintRow<'_>) -> Self {
            let st = StConstraintRow::from(value);
            st.into()
        }
    }

    // TODO(centril): find-replace all occurrences of body.
    fn begin_mut_tx(datastore: &Locking) -> MutTxId {
        datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests)
    }

    fn commit(datastore: &Locking, tx: MutTxId) -> ResultTest<TxData> {
        Ok(datastore.commit_mut_tx(tx)?.expect("commit should produce `TxData`"))
    }

    #[rustfmt::skip]
    fn basic_table_schema_cols() -> [ColRow<'static>; 3] {
        let table = FIRST_NON_SYSTEM_ID;
        [
            ColRow { table, pos: 0, name: "id", ty: AlgebraicType::U32 },
            ColRow { table, pos: 1, name: "name", ty: AlgebraicType::String },
            ColRow { table, pos: 2, name: "age", ty: AlgebraicType::U32 },
        ]
    }

    fn basic_indices() -> Vec<IndexSchema> {
        vec![
            IndexSchema {
                index_id: IndexId::SENTINEL,
                table_id: TableId::SENTINEL,
                index_name: "Foo_id_idx_btree".into(),
                index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![0] }),
            },
            IndexSchema {
                index_id: IndexId::SENTINEL,
                table_id: TableId::SENTINEL,
                index_name: "Foo_name_idx_btree".into(),
                index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![1] }),
            },
        ]
    }

    fn extract_index_id(datastore: &Locking, tx: &MutTxId, index: &IndexSchema) -> ResultTest<IndexId> {
        let index_id = datastore.index_id_from_name_mut_tx(tx, &index.index_name)?;
        Ok(index_id.expect("the index should exist"))
    }

    fn basic_constraints() -> Vec<ConstraintSchema> {
        vec![
            ConstraintSchema {
                table_id: TableId::SENTINEL,
                constraint_id: ConstraintId::SENTINEL,
                constraint_name: "Foo_id_key".into(),
                data: ConstraintData::Unique(UniqueConstraintData {
                    columns: col_list![0].into(),
                }),
            },
            ConstraintSchema {
                table_id: TableId::SENTINEL,
                constraint_id: ConstraintId::SENTINEL,
                constraint_name: "Foo_name_key".into(),
                data: ConstraintData::Unique(UniqueConstraintData {
                    columns: col_list![1].into(),
                }),
            },
        ]
    }

    fn basic_table_schema_with_indices(indices: Vec<IndexSchema>, constraints: Vec<ConstraintSchema>) -> TableSchema {
        TableSchema::new(
            TableId::SENTINEL,
            "Foo".into(),
            map_array(basic_table_schema_cols()),
            indices,
            constraints,
            vec![SequenceSchema {
                sequence_id: SequenceId::SENTINEL,
                table_id: TableId::SENTINEL,
                col_pos: 0.into(),
                sequence_name: "Foo_id_seq".into(),
                start: 1,
                increment: 1,
                min_value: 1,
                max_value: i128::MAX,
                allocated: 0,
            }],
            StTableType::User,
            StAccess::Public,
            None,
            None,
        )
    }

    #[rustfmt::skip]
    fn basic_table_schema_created(table_id: TableId) -> TableSchema {
        let table: u32 = table_id.into();
        let seq_start = FIRST_NON_SYSTEM_ID;

        TableSchema::new(
            table_id,
            "Foo".into(),
            map_array(basic_table_schema_cols()),
             map_array([
                IndexRow { id: seq_start,     table, col: ColList::new(0.into()), name: "Foo_id_idx_btree", },
                IndexRow { id: seq_start + 1, table, col: ColList::new(1.into()), name: "Foo_name_idx_btree", },
            ]),
            map_array([
                ConstraintRow { constraint_id: seq_start,     table_id: table, unique_columns: col(0), constraint_name: "Foo_id_key" },
                ConstraintRow { constraint_id: seq_start + 1, table_id: table, unique_columns: col(1), constraint_name: "Foo_name_key" }
            ]),
             map_array([
                SequenceRow { id: seq_start, table, col_pos: 0, name: "Foo_id_seq", start: 1 }
            ]),
            StTableType::User,
            StAccess::Public,
            None,
            None
        )
    }

    fn setup_table_with_indices(
        indices: Vec<IndexSchema>,
        constraints: Vec<ConstraintSchema>,
    ) -> ResultTest<(Locking, MutTxId, TableId)> {
        let datastore = get_datastore()?;
        let mut tx = begin_mut_tx(&datastore);
        let schema = basic_table_schema_with_indices(indices, constraints);
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        Ok((datastore, tx, table_id))
    }

    fn setup_table() -> ResultTest<(Locking, MutTxId, TableId)> {
        setup_table_with_indices(basic_indices(), basic_constraints())
    }

    fn random_row() -> ProductValue {
        u32_str_u32(42, "foo", 24)
    }

    fn all_rows(datastore: &Locking, tx: &MutTxId, table_id: TableId) -> Vec<ProductValue> {
        datastore
            .iter_mut_tx(tx, table_id)
            .unwrap()
            .map(|r| r.to_product_value().clone())
            .collect()
    }

    fn all_rows_tx(tx: &TxId, table_id: TableId) -> Vec<ProductValue> {
        tx.iter(table_id)
            .unwrap()
            .map(|r| r.to_product_value().clone())
            .collect()
    }

    fn insert<'a>(
        datastore: &'a Locking,
        tx: &'a mut MutTxId,
        table_id: TableId,
        row: &ProductValue,
    ) -> Result<(AlgebraicValue, RowRef<'a>)> {
        let row = to_vec(&row).unwrap();
        let (gen_cols, row_ref, _) = datastore.insert_mut_tx(tx, table_id, &row)?;
        let gen_cols = row_ref.project(&gen_cols)?;
        Ok((gen_cols, row_ref))
    }

    fn update<'a>(
        datastore: &'a Locking,
        tx: &'a mut MutTxId,
        table_id: TableId,
        index_id: IndexId,
        row: &ProductValue,
    ) -> Result<(AlgebraicValue, RowRef<'a>)> {
        let row = to_vec(&row).unwrap();
        let (gen_cols, row_ref, _) = datastore.update_mut_tx(tx, table_id, index_id, &row)?;
        let gen_cols = row_ref.project(&gen_cols)?;
        Ok((gen_cols, row_ref))
    }

    #[test]
    fn test_bootstrapping_sets_up_tables() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let query = query_st_tables(&tx);
        #[rustfmt::skip]
        assert_eq!(query.scan_st_tables()?, map_array([
            TableRow { id: ST_TABLE_ID.into(), name: ST_TABLE_NAME, ty: StTableType::System, access: StAccess::Public, primary_key: Some(StTableFields::TableId.into()) },
            TableRow { id: ST_COLUMN_ID.into(), name: ST_COLUMN_NAME, ty: StTableType::System, access: StAccess::Public, primary_key: None },
            TableRow { id: ST_SEQUENCE_ID.into(), name: ST_SEQUENCE_NAME, ty: StTableType::System, access: StAccess::Public, primary_key: Some(StSequenceFields::SequenceId.into()) },
            TableRow { id: ST_INDEX_ID.into(), name: ST_INDEX_NAME, ty: StTableType::System, access: StAccess::Public, primary_key: Some(StIndexFields::IndexId.into()) },
            TableRow { id: ST_CONSTRAINT_ID.into(), name: ST_CONSTRAINT_NAME, ty: StTableType::System, access: StAccess::Public, primary_key: Some(StConstraintFields::ConstraintId.into()) },
            TableRow { id: ST_MODULE_ID.into(), name: ST_MODULE_NAME, ty: StTableType::System, access: StAccess::Public, primary_key: None },
            TableRow { id: ST_CLIENT_ID.into(), name: ST_CLIENT_NAME, ty: StTableType::System, access: StAccess::Public, primary_key: None },
            TableRow { id: ST_VAR_ID.into(), name: ST_VAR_NAME, ty: StTableType::System, access: StAccess::Public, primary_key: Some(StVarFields::Name.into()) },
            TableRow { id: ST_SCHEDULED_ID.into(), name: ST_SCHEDULED_NAME, ty: StTableType::System, access: StAccess::Public, primary_key: Some(StScheduledFields::ScheduleId.into()) },
            TableRow { id: ST_ROW_LEVEL_SECURITY_ID.into(), name: ST_ROW_LEVEL_SECURITY_NAME, ty: StTableType::System, access: StAccess::Public, primary_key: Some(StRowLevelSecurityFields::Sql.into()) },
        ]));
        #[rustfmt::skip]
        assert_eq!(query.scan_st_columns()?, map_array([
            ColRow { table: ST_TABLE_ID.into(), pos: 0, name: "table_id", ty: TableId::get_type() },
            ColRow { table: ST_TABLE_ID.into(), pos: 1, name: "table_name", ty: AlgebraicType::String },
            ColRow { table: ST_TABLE_ID.into(), pos: 2, name: "table_type", ty: AlgebraicType::String },
            ColRow { table: ST_TABLE_ID.into(), pos: 3, name: "table_access", ty: AlgebraicType::String },
            ColRow { table: ST_TABLE_ID.into(), pos: 4, name: "table_primary_key", ty: AlgebraicType::option(resolved_type_via_v9::<ColList>()) },

            ColRow { table: ST_COLUMN_ID.into(), pos: 0, name: "table_id", ty: TableId::get_type() },
            ColRow { table: ST_COLUMN_ID.into(), pos: 1, name: "col_pos", ty: ColId::get_type() },
            ColRow { table: ST_COLUMN_ID.into(), pos: 2, name: "col_name", ty: AlgebraicType::String },
            ColRow { table: ST_COLUMN_ID.into(), pos: 3, name: "col_type", ty: AlgebraicType::bytes() },

            ColRow { table: ST_SEQUENCE_ID.into(), pos: 0, name: "sequence_id", ty: SequenceId::get_type() },
            ColRow { table: ST_SEQUENCE_ID.into(), pos: 1, name: "sequence_name", ty: AlgebraicType::String },
            ColRow { table: ST_SEQUENCE_ID.into(), pos: 2, name: "table_id", ty: TableId::get_type() },
            ColRow { table: ST_SEQUENCE_ID.into(), pos: 3, name: "col_pos", ty: ColId::get_type() },
            ColRow { table: ST_SEQUENCE_ID.into(), pos: 4, name: "increment", ty: AlgebraicType::I128 },
            ColRow { table: ST_SEQUENCE_ID.into(), pos: 5, name: "start", ty: AlgebraicType::I128 },
            ColRow { table: ST_SEQUENCE_ID.into(), pos: 6, name: "min_value", ty: AlgebraicType::I128 },
            ColRow { table: ST_SEQUENCE_ID.into(), pos: 7, name: "max_value", ty: AlgebraicType::I128 },
            ColRow { table: ST_SEQUENCE_ID.into(), pos: 8, name: "allocated", ty: AlgebraicType::I128 },

            ColRow { table: ST_INDEX_ID.into(), pos: 0, name: "index_id", ty: IndexId::get_type() },
            ColRow { table: ST_INDEX_ID.into(), pos: 1, name: "table_id", ty: TableId::get_type() },
            ColRow { table: ST_INDEX_ID.into(), pos: 2, name: "index_name", ty: AlgebraicType::String },
            ColRow { table: ST_INDEX_ID.into(), pos: 3, name: "index_algorithm", ty: resolved_type_via_v9::<StIndexAlgorithm>() },

            ColRow { table: ST_CONSTRAINT_ID.into(), pos: 0, name: "constraint_id", ty: ConstraintId::get_type() },
            ColRow { table: ST_CONSTRAINT_ID.into(), pos: 1, name: "constraint_name", ty: AlgebraicType::String },
            ColRow { table: ST_CONSTRAINT_ID.into(), pos: 2, name: "table_id", ty: TableId::get_type() },
            ColRow { table: ST_CONSTRAINT_ID.into(), pos: 3, name: "constraint_data", ty: resolved_type_via_v9::<StConstraintData>() },

            ColRow { table: ST_MODULE_ID.into(), pos: 0, name: "database_identity", ty: AlgebraicType::U256 },
            ColRow { table: ST_MODULE_ID.into(), pos: 1, name: "owner_identity", ty: AlgebraicType::U256 },
            ColRow { table: ST_MODULE_ID.into(), pos: 2, name: "program_kind", ty: AlgebraicType::U8 },
            ColRow { table: ST_MODULE_ID.into(), pos: 3, name: "program_hash", ty: AlgebraicType::U256 },
            ColRow { table: ST_MODULE_ID.into(), pos: 4, name: "program_bytes", ty: AlgebraicType::bytes() },
            ColRow { table: ST_MODULE_ID.into(), pos: 5, name: "module_version", ty: AlgebraicType::String },

            ColRow { table: ST_CLIENT_ID.into(), pos: 0, name: "identity", ty: AlgebraicType::U256},
            ColRow { table: ST_CLIENT_ID.into(), pos: 1, name: "connection_id", ty: AlgebraicType::U128},

            ColRow { table: ST_VAR_ID.into(), pos: 0, name: "name", ty: AlgebraicType::String },
            ColRow { table: ST_VAR_ID.into(), pos: 1, name: "value", ty: resolved_type_via_v9::<StVarValue>() },

            ColRow { table: ST_SCHEDULED_ID.into(), pos: 0, name: "schedule_id", ty: ScheduleId::get_type() },
            ColRow { table: ST_SCHEDULED_ID.into(), pos: 1, name: "table_id", ty: TableId::get_type() },
            ColRow { table: ST_SCHEDULED_ID.into(), pos: 2, name: "reducer_name", ty: AlgebraicType::String },
            ColRow { table: ST_SCHEDULED_ID.into(), pos: 3, name: "schedule_name", ty: AlgebraicType::String },
            ColRow { table: ST_SCHEDULED_ID.into(), pos: 4, name: "at_column", ty: AlgebraicType::U16 },

            ColRow { table: ST_ROW_LEVEL_SECURITY_ID.into(), pos: 0, name: "table_id", ty: TableId::get_type() },
            ColRow { table: ST_ROW_LEVEL_SECURITY_ID.into(), pos: 1, name: "sql", ty: AlgebraicType::String },
        ]));
        #[rustfmt::skip]
        assert_eq!(query.scan_st_indexes()?, map_array([
            IndexRow { id: 1, table: ST_TABLE_ID.into(), col: col(0), name: "st_table_table_id_idx_btree", },
            IndexRow { id: 2, table: ST_TABLE_ID.into(), col: col(1), name: "st_table_table_name_idx_btree", },
            IndexRow { id: 3, table: ST_COLUMN_ID.into(), col: col_list![0, 1], name: "st_column_table_id_col_pos_idx_btree", },
            IndexRow { id: 4, table: ST_SEQUENCE_ID.into(), col: col(0), name: "st_sequence_sequence_id_idx_btree", },
            IndexRow { id: 5, table: ST_INDEX_ID.into(), col: col(0), name: "st_index_index_id_idx_btree", },
            IndexRow { id: 6, table: ST_CONSTRAINT_ID.into(), col: col(0), name: "st_constraint_constraint_id_idx_btree", },
            IndexRow { id: 7, table: ST_CLIENT_ID.into(), col: col_list![0, 1], name: "st_client_identity_connection_id_idx_btree", },
            IndexRow { id: 8, table: ST_VAR_ID.into(), col: col(0), name: "st_var_name_idx_btree", },
            IndexRow { id: 9, table: ST_SCHEDULED_ID.into(), col: col(0), name: "st_scheduled_schedule_id_idx_btree", },
            IndexRow { id: 10, table: ST_SCHEDULED_ID.into(), col: col(1), name: "st_scheduled_table_id_idx_btree", },
            IndexRow { id: 11, table: ST_ROW_LEVEL_SECURITY_ID.into(), col: col(0), name: "st_row_level_security_table_id_idx_btree", },
            IndexRow { id: 12, table: ST_ROW_LEVEL_SECURITY_ID.into(), col: col(1), name: "st_row_level_security_sql_idx_btree", },
        ]));
        let start = FIRST_NON_SYSTEM_ID as i128;
        #[rustfmt::skip]
        assert_eq!(query.scan_st_sequences()?, map_array_fn(
            [
                SequenceRow { id: 1, table: ST_TABLE_ID.into(), col_pos: 0, name: "st_table_table_id_seq", start },
                SequenceRow { id: 5, table: ST_SEQUENCE_ID.into(), col_pos: 0, name: "st_sequence_sequence_id_seq", start },
                SequenceRow { id: 2, table: ST_INDEX_ID.into(), col_pos: 0, name: "st_index_index_id_seq", start },
                SequenceRow { id: 3, table: ST_CONSTRAINT_ID.into(), col_pos: 0, name: "st_constraint_constraint_id_seq", start },
                SequenceRow { id: 4, table: ST_SCHEDULED_ID.into(), col_pos: 0, name: "st_scheduled_schedule_id_seq", start },
            ],
            |row| StSequenceRow {
                allocated: ST_RESERVED_SEQUENCE_RANGE as i128,
                ..StSequenceRow::from(row)
            }
        ));
        #[rustfmt::skip]
        assert_eq!(query.scan_st_constraints()?, map_array([
            ConstraintRow { constraint_id: 1, table_id: ST_TABLE_ID.into(), unique_columns: col(0), constraint_name: "st_table_table_id_key", },
            ConstraintRow { constraint_id: 2, table_id: ST_TABLE_ID.into(), unique_columns: col(1), constraint_name: "st_table_table_name_key", },
            ConstraintRow { constraint_id: 3, table_id: ST_COLUMN_ID.into(), unique_columns: col_list![0, 1], constraint_name: "st_column_table_id_col_pos_key", },
            ConstraintRow { constraint_id: 4, table_id: ST_SEQUENCE_ID.into(), unique_columns: col(0), constraint_name: "st_sequence_sequence_id_key", },
            ConstraintRow { constraint_id: 5, table_id: ST_INDEX_ID.into(), unique_columns: col(0), constraint_name: "st_index_index_id_key", },
            ConstraintRow { constraint_id: 6, table_id: ST_CONSTRAINT_ID.into(), unique_columns: col(0), constraint_name: "st_constraint_constraint_id_key", },
            ConstraintRow { constraint_id: 7, table_id: ST_CLIENT_ID.into(), unique_columns: col_list![0, 1], constraint_name: "st_client_identity_connection_id_key", },
            ConstraintRow { constraint_id: 8, table_id: ST_VAR_ID.into(), unique_columns: col(0), constraint_name: "st_var_name_key", },
            ConstraintRow { constraint_id: 9, table_id: ST_SCHEDULED_ID.into(), unique_columns: col(0), constraint_name: "st_scheduled_schedule_id_key", },
            ConstraintRow { constraint_id: 10, table_id: ST_SCHEDULED_ID.into(), unique_columns: col(1), constraint_name: "st_scheduled_table_id_key", },
            ConstraintRow { constraint_id: 11, table_id: ST_ROW_LEVEL_SECURITY_ID.into(), unique_columns: col(1), constraint_name: "st_row_level_security_sql_key", },
        ]));

        // Verify we get back the tables correctly with the proper ids...
        let cols = query.scan_st_columns()?;
        let idx = query.scan_st_indexes()?;
        let seq = query.scan_st_sequences()?;
        let ct = query.scan_st_constraints()?;

        for st in system_tables() {
            let schema = datastore.schema_for_table_mut_tx(&tx, st.table_id).unwrap();
            assert_eq!(
                schema.columns().to_vec(),
                cols.iter()
                    .filter(|x| x.table_id == st.table_id)
                    .cloned()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
                "Columns for {}",
                schema.table_name
            );

            assert_eq!(
                schema.indexes,
                idx.iter()
                    .filter(|x| x.table_id == st.table_id)
                    .cloned()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
                "Indexes for {}",
                schema.table_name
            );

            assert_eq!(
                schema.sequences,
                seq.iter()
                    .filter(|x| x.table_id == st.table_id)
                    .cloned()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
                "Sequences for {}",
                schema.table_name
            );

            assert_eq!(
                schema.constraints,
                ct.iter()
                    .filter(|x| x.table_id == st.table_id)
                    .cloned()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
                "Constraints for {}",
                schema.table_name
            );
        }

        datastore.rollback_mut_tx(tx);
        Ok(())
    }

    #[test]
    fn test_create_table_pre_commit() -> ResultTest<()> {
        let (_, tx, table_id) = setup_table()?;
        let query = query_st_tables(&tx);

        let table_rows = query.scan_st_tables_by_col(ColId(0), &table_id.into())?;
        #[rustfmt::skip]
        assert_eq!(table_rows, map_array([
            TableRow { id: FIRST_NON_SYSTEM_ID, name: "Foo", ty: StTableType::User, access: StAccess::Public, primary_key: None }
        ]));
        let column_rows = query.scan_st_columns_by_col(ColId(0), &table_id.into())?;
        #[rustfmt::skip]
        assert_eq!(column_rows, map_array(basic_table_schema_cols()));
        Ok(())
    }

    #[test]
    fn test_create_table_post_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx(tx)?;
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let query = query_st_tables(&tx);

        let table_rows = query.scan_st_tables_by_col(ColId(0), &table_id.into())?;
        #[rustfmt::skip]
        assert_eq!(table_rows, map_array([
            TableRow { id: FIRST_NON_SYSTEM_ID, name: "Foo", ty: StTableType::User, access: StAccess::Public, primary_key: None }
        ]));
        let column_rows = query.scan_st_columns_by_col(ColId(0), &table_id.into())?;
        #[rustfmt::skip]
        assert_eq!(column_rows, map_array(basic_table_schema_cols()));

        Ok(())
    }

    #[test]
    fn test_create_table_post_rollback() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.rollback_mut_tx(tx);
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        assert!(
            !datastore.table_id_exists_mut_tx(&tx, &table_id),
            "Table should not exist"
        );
        let query = query_st_tables(&tx);

        let table_rows = query.scan_st_tables_by_col(ColId(0), &table_id.into())?;
        assert_eq!(table_rows, []);
        let column_rows = query.scan_st_columns_by_col(ColId(0), &table_id.into())?;
        assert_eq!(column_rows, []);
        Ok(())
    }

    fn verify_schemas_consistent(tx: &mut MutTxId, table_id: TableId) {
        let s1 = tx.get_schema(table_id).expect("should exist");
        let s2 = tx.schema_for_table_raw(table_id).expect("should exist");
        assert_eq!(**s1, s2);
    }

    #[test]
    fn test_schema_for_table_pre_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let schema = &*datastore.schema_for_table_mut_tx(&tx, table_id)?;

        verify_schemas_consistent(&mut tx, table_id);

        #[rustfmt::skip]
        assert_eq!(schema, &basic_table_schema_created(table_id));
        Ok(())
    }

    #[test]
    fn test_schema_for_table_post_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        verify_schemas_consistent(&mut tx, table_id);
        let schema = &*datastore.schema_for_table_mut_tx(&tx, table_id)?;
        #[rustfmt::skip]
        assert_eq!(schema, &basic_table_schema_created(table_id));
        Ok(())
    }

    #[test]
    fn test_schema_for_table_alter_indexes() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx(tx)?;

        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let schema = datastore.schema_for_table_mut_tx(&tx, table_id)?;

        let mut dropped_indexes = 0;
        for index in &*schema.indexes {
            datastore.drop_index_mut_tx(&mut tx, index.index_id)?;
            dropped_indexes += 1;
        }
        assert!(
            datastore.schema_for_table_mut_tx(&tx, table_id)?.indexes.is_empty(),
            "no indexes should be left in the schema pre-commit"
        );
        datastore.commit_mut_tx(tx)?;

        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        assert!(
            datastore.schema_for_table_mut_tx(&tx, table_id)?.indexes.is_empty(),
            "no indexes should be left in the schema post-commit"
        );

        verify_schemas_consistent(&mut tx, table_id);

        datastore.create_index_mut_tx(
            &mut tx,
            IndexSchema {
                index_id: IndexId::SENTINEL,
                table_id,
                index_name: "Foo_id_idx_btree".into(),
                index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![0] }),
            },
            true,
        )?;

        verify_schemas_consistent(&mut tx, table_id);

        let expected_indexes = [IndexRow {
            id: ST_RESERVED_SEQUENCE_RANGE + dropped_indexes + 1,
            table: FIRST_NON_SYSTEM_ID,
            col: col_list![0],
            name: "Foo_id_idx_btree",
        }]
        .map(Into::into);
        assert_eq!(
            datastore.schema_for_table_mut_tx(&tx, table_id)?.indexes,
            expected_indexes,
            "created index should be present in schema pre-commit"
        );

        datastore.commit_mut_tx(tx)?;

        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        assert_eq!(
            datastore.schema_for_table_mut_tx(&tx, table_id)?.indexes,
            expected_indexes,
            "created index should be present in schema post-commit"
        );

        datastore.commit_mut_tx(tx)?;

        Ok(())
    }

    #[test]
    fn test_schema_for_table_rollback() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.rollback_mut_tx(tx);
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let schema = datastore.schema_for_table_mut_tx(&tx, table_id);
        assert!(schema.is_err());
        Ok(())
    }

    #[test]
    fn test_insert_pre_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &row)?;
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_insert_wrong_schema_pre_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = product!(0, "Foo");
        assert!(insert(&datastore, &mut tx, table_id, &row).is_err());
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![]);
        Ok(())
    }

    #[test]
    fn test_insert_post_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &u32_str_u32(0, "Foo", 18))?;
        datastore.commit_mut_tx(tx)?;
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_insert_post_rollback() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        let row = u32_str_u32(15, "Foo", 18); // 15 is ignored.
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        insert(&datastore, &mut tx, table_id, &row)?;
        datastore.rollback_mut_tx(tx);
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![]);
        Ok(())
    }

    #[test]
    fn test_insert_commit_delete_insert() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &row)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let created_row = u32_str_u32(1, "Foo", 18);
        let num_deleted = datastore.delete_by_rel_mut_tx(&mut tx, table_id, [created_row]);
        assert_eq!(num_deleted, 1);
        assert_eq!(all_rows(&datastore, &tx, table_id).len(), 0);
        let created_row = u32_str_u32(1, "Foo", 19);
        insert(&datastore, &mut tx, table_id, &created_row)?;
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 19)]);
        Ok(())
    }

    #[test]
    fn test_insert_delete_insert_delete_insert() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(1, "Foo", 18); // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &row)?;
        for i in 0..2 {
            assert_eq!(
                all_rows(&datastore, &tx, table_id),
                vec![row.clone()],
                "Found unexpected set of rows before deleting",
            );
            let num_deleted = datastore.delete_by_rel_mut_tx(&mut tx, table_id, [row.clone()]);
            assert_eq!(
                num_deleted, 1,
                "delete_by_rel deleted an unexpected number of rows on iter {i}",
            );
            assert_eq!(
                &all_rows(&datastore, &tx, table_id),
                &[],
                "Found rows present after deleting",
            );
            insert(&datastore, &mut tx, table_id, &row)?;
            assert_eq!(
                all_rows(&datastore, &tx, table_id),
                vec![row.clone()],
                "Found unexpected set of rows after inserting",
            );
        }
        Ok(())
    }

    #[test]
    fn test_unique_constraint_pre_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &row)?;
        let result = insert(&datastore, &mut tx, table_id, &row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation(UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                cols: _,
                value: _,
            }))) => (),
            _ => panic!("Expected an unique constraint violation error."),
        }
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_unique_constraint_post_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &row)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let result = insert(&datastore, &mut tx, table_id, &row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation(UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                cols: _,
                value: _,
            }))) => (),
            _ => panic!("Expected an unique constraint violation error."),
        }
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_unique_constraint_post_rollback() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &row)?;
        datastore.rollback_mut_tx(tx);
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        insert(&datastore, &mut tx, table_id, &row)?;
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(2, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_create_index_pre_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx(tx)?;

        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &row)?;
        datastore.commit_mut_tx(tx)?;

        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let index_def = IndexSchema {
            index_id: IndexId::SENTINEL,
            table_id,
            index_name: "Foo_age_idx_btree".into(),
            index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![2] }),
        };
        // TODO: it's slightly incorrect to create an index with `is_unique: true` without creating a corresponding constraint.
        // But the `Table` crate allows it for now.
        datastore.create_index_mut_tx(&mut tx, index_def, true)?;
        let query = query_st_tables(&tx);
        let seq_start = FIRST_NON_SYSTEM_ID;
        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 1, table: ST_TABLE_ID.into(), col: col(0), name: "st_table_table_id_idx_btree", },
            IndexRow { id: 2, table: ST_TABLE_ID.into(), col: col(1), name: "st_table_table_name_idx_btree", },
            IndexRow { id: 3, table: ST_COLUMN_ID.into(), col: col_list![0, 1], name: "st_column_table_id_col_pos_idx_btree", },
            IndexRow { id: 4, table: ST_SEQUENCE_ID.into(), col: col(0), name: "st_sequence_sequence_id_idx_btree", },
            IndexRow { id: 5, table: ST_INDEX_ID.into(), col: col(0), name: "st_index_index_id_idx_btree", },
            IndexRow { id: 6, table: ST_CONSTRAINT_ID.into(), col: col(0), name: "st_constraint_constraint_id_idx_btree", },
            IndexRow { id: 7, table: ST_CLIENT_ID.into(), col: col_list![0, 1], name: "st_client_identity_connection_id_idx_btree", },
            IndexRow { id: 8, table: ST_VAR_ID.into(), col: col(0), name: "st_var_name_idx_btree", },
            IndexRow { id: 9, table: ST_SCHEDULED_ID.into(), col: col(0), name: "st_scheduled_schedule_id_idx_btree", },
            IndexRow { id: 10, table: ST_SCHEDULED_ID.into(), col: col(1), name: "st_scheduled_table_id_idx_btree", },
            IndexRow { id: 11, table: ST_ROW_LEVEL_SECURITY_ID.into(), col: col(0), name: "st_row_level_security_table_id_idx_btree", },
            IndexRow { id: 12, table: ST_ROW_LEVEL_SECURITY_ID.into(), col: col(1), name: "st_row_level_security_sql_idx_btree", },
            IndexRow { id: seq_start,     table: FIRST_NON_SYSTEM_ID, col: col(0), name: "Foo_id_idx_btree",  },
            IndexRow { id: seq_start + 1, table: FIRST_NON_SYSTEM_ID, col: col(1), name: "Foo_name_idx_btree",  },
            IndexRow { id: seq_start + 2, table: FIRST_NON_SYSTEM_ID, col: col(2), name: "Foo_age_idx_btree",  },
        ].map(Into::into));
        let row = u32_str_u32(0, "Bar", 18); // 0 will be ignored.
        let result = insert(&datastore, &mut tx, table_id, &row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation(UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                cols: _,
                value: _,
            }))) => (),
            e => panic!("Expected an unique constraint violation error but got {e:?}."),
        }
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_create_index_post_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &row)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let index_def = IndexSchema {
            index_id: IndexId::SENTINEL,
            table_id,
            index_name: "Foo_age_idx_btree".into(),
            index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![2] }),
        };
        datastore.create_index_mut_tx(&mut tx, index_def, true)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let query = query_st_tables(&tx);

        let seq_start = FIRST_NON_SYSTEM_ID;
        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 1, table: ST_TABLE_ID.into(), col: col(0), name: "st_table_table_id_idx_btree", },
            IndexRow { id: 2, table: ST_TABLE_ID.into(), col: col(1), name: "st_table_table_name_idx_btree", },
            IndexRow { id: 3, table: ST_COLUMN_ID.into(), col: col_list![0, 1], name: "st_column_table_id_col_pos_idx_btree", },
            IndexRow { id: 4, table: ST_SEQUENCE_ID.into(), col: col(0), name: "st_sequence_sequence_id_idx_btree", },
            IndexRow { id: 5, table: ST_INDEX_ID.into(), col: col(0), name: "st_index_index_id_idx_btree", },
            IndexRow { id: 6, table: ST_CONSTRAINT_ID.into(), col: col(0), name: "st_constraint_constraint_id_idx_btree", },
            IndexRow { id: 7, table: ST_CLIENT_ID.into(), col: col_list![0, 1], name: "st_client_identity_connection_id_idx_btree", },
            IndexRow { id: 8, table: ST_VAR_ID.into(), col: col(0), name: "st_var_name_idx_btree", },
            IndexRow { id: 9, table: ST_SCHEDULED_ID.into(), col: col(0), name: "st_scheduled_schedule_id_idx_btree", },
            IndexRow { id: 10, table: ST_SCHEDULED_ID.into(), col: col(1), name: "st_scheduled_table_id_idx_btree", },
            IndexRow { id: 11, table: ST_ROW_LEVEL_SECURITY_ID.into(), col: col(0), name: "st_row_level_security_table_id_idx_btree", },
            IndexRow { id: 12, table: ST_ROW_LEVEL_SECURITY_ID.into(), col: col(1), name: "st_row_level_security_sql_idx_btree", },
            IndexRow { id: seq_start    , table: FIRST_NON_SYSTEM_ID, col: col(0), name: "Foo_id_idx_btree",  },
            IndexRow { id: seq_start + 1, table: FIRST_NON_SYSTEM_ID, col: col(1), name: "Foo_name_idx_btree", },
            IndexRow { id: seq_start + 2, table: FIRST_NON_SYSTEM_ID, col: col(2), name: "Foo_age_idx_btree", },
        ].map(Into::into));
        let row = u32_str_u32(0, "Bar", 18); // 0 will be ignored.
        let result = insert(&datastore, &mut tx, table_id, &row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation(UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                cols: _,
                value: _,
            }))) => (),
            _ => panic!("Expected an unique constraint violation error."),
        }
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_create_index_post_rollback() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &row)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let index_def = IndexSchema {
            index_id: IndexId::SENTINEL,
            table_id,
            index_name: "age_idx".into(),
            index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![2] }),
        };
        datastore.create_index_mut_tx(&mut tx, index_def, true)?;

        datastore.rollback_mut_tx(tx);
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        let query = query_st_tables(&tx);

        let seq_start = FIRST_NON_SYSTEM_ID;
        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 1, table: ST_TABLE_ID.into(), col: col(0), name: "st_table_table_id_idx_btree", },
            IndexRow { id: 2, table: ST_TABLE_ID.into(), col: col(1), name: "st_table_table_name_idx_btree", },
            IndexRow { id: 3, table: ST_COLUMN_ID.into(), col: col_list![0, 1], name: "st_column_table_id_col_pos_idx_btree", },
            IndexRow { id: 4, table: ST_SEQUENCE_ID.into(), col: col(0), name: "st_sequence_sequence_id_idx_btree", },
            IndexRow { id: 5, table: ST_INDEX_ID.into(), col: col(0), name: "st_index_index_id_idx_btree", },
            IndexRow { id: 6, table: ST_CONSTRAINT_ID.into(), col: col(0), name: "st_constraint_constraint_id_idx_btree", },
            IndexRow { id: 7, table: ST_CLIENT_ID.into(), col: col_list![0, 1], name: "st_client_identity_connection_id_idx_btree", },
            IndexRow { id: 8, table: ST_VAR_ID.into(), col: col(0), name: "st_var_name_idx_btree", },
            IndexRow { id: 9, table: ST_SCHEDULED_ID.into(), col: col(0), name: "st_scheduled_schedule_id_idx_btree", },
            IndexRow { id: 10, table: ST_SCHEDULED_ID.into(), col: col(1), name: "st_scheduled_table_id_idx_btree", },
            IndexRow { id: 11, table: ST_ROW_LEVEL_SECURITY_ID.into(), col: col(0), name: "st_row_level_security_table_id_idx_btree", },
            IndexRow { id: 12, table: ST_ROW_LEVEL_SECURITY_ID.into(), col: col(1), name: "st_row_level_security_sql_idx_btree", },
            IndexRow { id: seq_start,     table: FIRST_NON_SYSTEM_ID, col: col(0), name: "Foo_id_idx_btree", },
            IndexRow { id: seq_start + 1, table: FIRST_NON_SYSTEM_ID, col: col(1), name: "Foo_name_idx_btree", },
        ].map(Into::into));
        let row = u32_str_u32(0, "Bar", 18); // 0 will be ignored.
        insert(&datastore, &mut tx, table_id, &row)?;
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![
            u32_str_u32(1, "Foo", 18),
            u32_str_u32(2, "Bar", 18),
        ]);
        Ok(())
    }

    #[test]
    fn test_update_reinsert() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;

        // Insert a row and commit the tx.
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
                                             // Because of auto_inc columns, we will get a slightly different
                                             // value than the one we inserted.
        let row = insert(&datastore, &mut tx, table_id, &row)?.1.to_product_value();
        datastore.commit_mut_tx(tx)?;

        let all_rows_col_0_eq_1 = |tx: &MutTxId| {
            datastore
                .iter_by_col_eq_mut_tx(tx, table_id, ColId(0), &AlgebraicValue::U32(1))
                .unwrap()
                .map(|row_ref| row_ref.to_product_value())
                .collect::<Vec<_>>()
        };

        // Update the db with the same actual value for that row, in a new tx.
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable, Workload::ForTests);
        // Iterate over all rows with the value 1 (from the auto_inc) in column 0.
        let rows = all_rows_col_0_eq_1(&tx);
        assert_eq!(rows.len(), 1);
        assert_eq!(row, rows[0]);
        // Delete the row.
        let count_deleted = datastore.delete_by_rel_mut_tx(&mut tx, table_id, rows);
        assert_eq!(count_deleted, 1);

        // We shouldn't see the row when iterating now that it's deleted.
        assert_eq!(all_rows_col_0_eq_1(&tx).len(), 0);

        // Reinsert the row.
        let reinserted_row = insert(&datastore, &mut tx, table_id, &row)?.1.to_product_value();
        assert_eq!(reinserted_row, row);

        // The actual test: we should be able to iterate again, while still in the
        // second transaction, and see exactly one row.
        assert_eq!(all_rows_col_0_eq_1(&tx).len(), 1);

        datastore.commit_mut_tx(tx)?;

        Ok(())
    }

    fn expect_index_err(res: Result<impl fmt::Debug>) -> IndexError {
        res.expect_err("`res` should be an error")
            .into_index()
            .expect("the error should be an `IndexError`")
    }

    fn test_under_tx_and_commit(
        datastore: &Locking,
        mut tx: MutTxId,
        mut test: impl FnMut(&mut MutTxId) -> ResultTest<()>,
    ) -> ResultTest<()> {
        // Test the tx state.
        test(&mut tx)?;

        // Test the commit state.
        commit(datastore, tx)?;
        test(&mut begin_mut_tx(datastore))
    }

    /// Checks that update validates the row against the row type.
    #[test]
    fn test_update_wrong_row_type() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table_with_indices([].into(), [].into())?;
        test_under_tx_and_commit(&datastore, tx, |tx| {
            // We provide an index that doesn't exist on purpose.
            let index_id = 0.into();
            // Remove the last field of the row, invalidating it.
            let mut row = Vec::from(random_row().elements);
            let _ = row.pop().expect("there should be an element to remove");
            let row = row.into();
            // Now attempt the update.
            let err = update(&datastore, tx, table_id, index_id, &row)
                .expect_err("the update should fail")
                .into_table()
                .expect("the error should be a `TableError`")
                .into_bflatn()
                .expect("the error should be a bflatn error");

            assert_matches!(err, spacetimedb_table::bflatn_to::Error::Decode(..));
            Ok(())
        })
    }

    /// Checks that update checks if the index exists.
    #[test]
    fn test_regression_2134() -> ResultTest<()> {
        // Get us a datastore and tx.
        let datastore = get_datastore()?;
        let mut tx = begin_mut_tx(&datastore);

        // Create the table. The minimal repro is a one column table with a unique constraint.
        let table_id = TableId::SENTINEL;
        let table_schema = TableSchema::new(
            table_id,
            "Foo".into(),
            vec![ColumnSchema {
                table_id,
                col_pos: 0.into(),
                col_name: "id".into(),
                col_type: AlgebraicType::I32,
            }],
            vec![IndexSchema {
                table_id,
                index_id: IndexId::SENTINEL,
                index_name: "btree".into(),
                index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: 0.into() }),
            }],
            vec![ConstraintSchema {
                table_id,
                constraint_id: ConstraintId::SENTINEL,
                constraint_name: "constraint".into(),
                data: ConstraintData::Unique(UniqueConstraintData {
                    columns: col_list![0].into(),
                }),
            }],
            vec![],
            StTableType::User,
            StAccess::Public,
            None,
            None,
        );
        let table_id = datastore.create_table_mut_tx(&mut tx, table_schema)?;
        commit(&datastore, tx)?;

        // A "reducer" that deletes and then inserts the same row.
        let row = &product![1];
        let update = |datastore| -> ResultTest<_> {
            let mut tx = begin_mut_tx(datastore);
            // Delete the row.
            let deleted = tx.delete_by_row_value(table_id, row)?;
            // Insert it again.
            insert(datastore, &mut tx, table_id, row)?;
            let tx_data = commit(datastore, tx)?;
            Ok((deleted, tx_data))
        };

        // In two separate transactions, we update a row to itself.
        // What should happen is that the row is added to the committed state the first time,
        // as the delete does nothing.
        //
        // The second time however,
        // the delete should first mark the committed row as deleted in the delete tables,
        // and then it should remove it from the delete tables upon insertion,
        // rather than actually inserting it in the tx state.
        // So the second transaction should be observationally a no-op.s
        // There was a bug in the datastore that did not respect this in the presence of a unique index.
        let (deleted_1, tx_data_1) = update(&datastore)?;
        let (deleted_2, tx_data_2) = update(&datastore)?;

        // In the first tx, the row is not deleted, but it is inserted, so we end up with the row committed.
        assert_eq!(deleted_1, false);
        assert_eq!(tx_data_1.deletes().count(), 0);
        assert_eq!(tx_data_1.inserts().collect_vec(), [(&table_id, &[row.clone()].into())]);

        // In the second tx, the row is deleted from the commit state,
        // by marking it in the delete tables.
        // Then, when inserting, it is un-deleted by un-marking.
        // This sequence results in an empty tx-data.
        assert_eq!(deleted_2, true);
        assert_eq!(tx_data_2.deletes().count(), 0);
        assert_eq!(tx_data_2.inserts().collect_vec(), []);
        Ok(())
    }

    #[test]
    fn test_update_brings_back_deleted_commit_row_repro_2296() -> ResultTest<()> {
        // Get us a datastore and tx.
        let datastore = get_datastore()?;
        let mut tx = begin_mut_tx(&datastore);

        // Create the table. The minimal repro is a two column table with a unique constraint.
        let table_id = TableId::SENTINEL;
        let col = |pos: usize| ColumnSchema {
            table_id,
            col_pos: pos.into(),
            col_name: format!("c{pos}").into(),
            col_type: AlgebraicType::U32,
        };
        let table_schema = TableSchema::new(
            table_id,
            "Foo".into(),
            [col(0), col(1)].into(),
            vec![IndexSchema {
                table_id,
                index_id: IndexId::SENTINEL,
                index_name: "index".into(),
                index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: 0.into() }),
            }],
            vec![ConstraintSchema {
                table_id,
                constraint_id: ConstraintId::SENTINEL,
                constraint_name: "constraint".into(),
                data: ConstraintData::Unique(UniqueConstraintData { columns: 0.into() }),
            }],
            vec![],
            StTableType::User,
            StAccess::Public,
            None,
            None,
        );
        let table_id = datastore.create_table_mut_tx(&mut tx, table_schema)?;
        let index_id = datastore.index_id_from_name_mut_tx(&tx, "index")?.unwrap();
        let find_row_by_key = |tx: &MutTxId, key: u32| {
            tx.index_scan_point(table_id, index_id, &key.into())
                .unwrap()
                .map(|row| row.pointer())
                .collect::<Vec<_>>()
        };

        // Insert `Foo { c0: 0, c1: 0 }`.
        const KEY: u32 = 0;
        const DATA: u32 = 0;
        let row = &product![KEY, DATA];
        let row_prime = &product![KEY, DATA + 1];
        insert(&datastore, &mut tx, table_id, row)?;

        // It's important for the test that the row is committed.
        commit(&datastore, tx)?;

        // Start a new transaction where we:
        let mut tx = begin_mut_tx(&datastore);
        // 1. delete the row.
        let row_to_del = find_row_by_key(&tx, KEY);
        datastore.delete_mut_tx(&mut tx, table_id, row_to_del.iter().copied());
        // 2. insert a new row with the same key as the one we deleted but different extra field.
        // We should now have a committed row `row` marked as deleted and a row `row_prime`.
        // These share `KEY`.
        insert(&datastore, &mut tx, table_id, row_prime)?;
        // 3. update `row_prime` -> `row`.
        // Because `row` exists in the committed state but was marked as deleted,
        // it should be undeleted, without a new row being added to the tx state.
        let (_, row_ref) = update(&datastore, &mut tx, table_id, index_id, row)?;
        assert_eq!([row_ref.pointer()], &*row_to_del);
        assert_eq!(row_to_del, find_row_by_key(&tx, KEY));

        // Commit the transaction.
        // We expect the transaction to be a noop.
        let tx_data = commit(&datastore, tx)?;
        assert_eq!(tx_data.inserts().count(), 0);
        assert_eq!(tx_data.deletes().count(), 0);
        Ok(())
    }

    #[test]
    fn test_update_no_such_index() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table_with_indices([].into(), [].into())?;
        test_under_tx_and_commit(&datastore, tx, |tx| {
            let index_id = 0.into();
            let err = expect_index_err(update(&datastore, tx, table_id, index_id, &random_row()));
            assert_eq!(err, IndexError::NotFound(index_id));
            Ok(())
        })
    }

    /// Checks that update checks if the index exists and that this considers tx-state index deletion.
    #[test]
    fn test_update_no_such_index_because_deleted() -> ResultTest<()> {
        // Setup and immediately commit.
        let (datastore, tx, table_id) = setup_table()?;
        commit(&datastore, tx)?;

        // Remove index in tx state.
        let mut tx = begin_mut_tx(&datastore);
        let index_id = extract_index_id(&datastore, &tx, &basic_indices()[0])?;
        tx.drop_index(index_id)?;

        test_under_tx_and_commit(&datastore, tx, |tx: &mut _| {
            let err = expect_index_err(update(&datastore, tx, table_id, index_id, &random_row()));
            assert_eq!(err, IndexError::NotFound(index_id));
            Ok(())
        })
    }

    /// Checks that update ensures the index is unique.
    #[test]
    fn test_update_index_not_unique() -> ResultTest<()> {
        let indices = basic_indices();
        let (datastore, mut tx, table_id) = setup_table_with_indices(indices.clone(), [].into())?;
        let row = &random_row();
        insert(&datastore, &mut tx, table_id, row)?;

        test_under_tx_and_commit(&datastore, tx, |tx| {
            for index in &indices {
                let index_id = extract_index_id(&datastore, tx, index)?;
                let err = expect_index_err(update(&datastore, tx, table_id, index_id, row));
                assert_eq!(err, IndexError::NotUnique(index_id));
            }
            Ok(())
        })
    }

    /// Checks that update ensures that the row-to-update exists.
    #[test]
    fn test_update_no_such_row() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;

        test_under_tx_and_commit(&datastore, tx, |tx| {
            let row = &random_row();
            for (index_pos, index) in basic_indices().into_iter().enumerate() {
                let index_id = extract_index_id(&datastore, tx, &index)?;
                let err = expect_index_err(update(&datastore, tx, table_id, index_id, row));
                let needle = row.get_field(index_pos, None).unwrap().clone();
                assert_eq!(err, IndexError::KeyNotFound(index_id, needle));
            }
            Ok(())
        })
    }

    /// Checks that update ensures that the row-to-update exists and considers delete tables.
    #[test]
    fn test_update_no_such_row_because_deleted() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;

        // Insert the row, commit, and delete it.
        let row = &random_row();
        insert(&datastore, &mut tx, table_id, row)?;
        commit(&datastore, tx)?;
        let mut tx = begin_mut_tx(&datastore);
        assert_eq!(1, datastore.delete_by_rel_mut_tx(&mut tx, table_id, [row.clone()]));

        test_under_tx_and_commit(&datastore, tx, |tx| {
            for (index_pos, index) in basic_indices().into_iter().enumerate() {
                let index_id = extract_index_id(&datastore, tx, &index)?;
                let err = expect_index_err(update(&datastore, tx, table_id, index_id, row));
                let needle = row.get_field(index_pos, None).unwrap().clone();
                assert_eq!(err, IndexError::KeyNotFound(index_id, needle));
            }
            Ok(())
        })
    }

    /// Checks that update ensures that the row-to-update exists and considers delete tables.
    #[test]
    fn test_update_no_such_row_because_deleted_new_index_in_tx() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table_with_indices([].into(), [].into())?;

        // Insert the row and commit.
        let row = &random_row();
        insert(&datastore, &mut tx, table_id, row)?;
        commit(&datastore, tx)?;

        // Now add the indices and then delete the row.
        let mut tx = begin_mut_tx(&datastore);
        let mut indices = basic_indices();
        for index in &mut indices {
            index.table_id = table_id;
            index.index_id = datastore.create_index_mut_tx(&mut tx, index.clone(), true)?;
        }
        assert_eq!(1, datastore.delete_by_rel_mut_tx(&mut tx, table_id, [row.clone()]));

        test_under_tx_and_commit(&datastore, tx, |tx| {
            for (index_pos, index) in indices.iter().enumerate() {
                let err = expect_index_err(update(&datastore, tx, table_id, index.index_id, row));
                let needle = row.get_field(index_pos, None).unwrap().clone();
                assert_eq!(err, IndexError::KeyNotFound(index.index_id, needle));
            }
            Ok(())
        })
    }

    /// Checks that update ensures that the row-to-update exists and that sequences were used.
    #[test]
    fn test_update_no_such_row_seq_triggered() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        test_under_tx_and_commit(&datastore, tx, |tx| {
            let mut row = random_row();
            let field_before = mem::replace(&mut row.elements[0], 0u32.into());

            // Use the index on the first u32 field as it's unique auto_inc.
            let index_id = extract_index_id(&datastore, tx, &basic_indices()[0])?;

            // Attempt the update.
            let err = expect_index_err(update(&datastore, tx, table_id, index_id, &row));
            assert_matches!(err, IndexError::KeyNotFound(_, key) if key != field_before);
            Ok(())
        })
    }

    /// Checks that update checks other unique constraints against the committed state.
    #[test]
    fn test_update_violates_commit_unique_constraints() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table_with_indices([].into(), [].into())?;

        // Insert two rows.
        let mut row = random_row();
        insert(&datastore, &mut tx, table_id, &row)?;
        row.elements[0] = 24u32.into();
        let original_string = mem::replace(&mut row.elements[1], "bar".into());
        insert(&datastore, &mut tx, table_id, &row)?;
        row.elements[1] = original_string;

        // Add the index on the string field.
        let mut indices = basic_indices();
        for index in &mut indices {
            index.table_id = table_id;
        }
        datastore.create_index_mut_tx(&mut tx, indices.swap_remove(1), true)?;
        // Commit.
        commit(&datastore, tx)?;

        // *After committing*, add the u32 field index.
        // We'll use that index to seek whilst changing the second field to the first row we added.
        let mut tx = begin_mut_tx(&datastore);
        let index_id = datastore.create_index_mut_tx(&mut tx, indices.swap_remove(0), true)?;

        test_under_tx_and_commit(&datastore, tx, |tx| {
            // Attempt the update. There should be a unique constraint violation on the string field.
            let err = expect_index_err(update(&datastore, tx, table_id, index_id, &row));
            assert_matches!(err, IndexError::UniqueConstraintViolation(_));
            Ok(())
        })
    }

    /// Checks that update checks other unique constraints against the committed state.
    #[test]
    fn test_update_violates_tx_unique_constraints() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;

        // Insert two rows.
        let mut row = random_row();
        insert(&datastore, &mut tx, table_id, &row)?;
        row.elements[0] = 24u32.into();
        let original_string = mem::replace(&mut row.elements[1], "bar".into());
        insert(&datastore, &mut tx, table_id, &row)?;
        row.elements[1] = original_string;

        // Seek the index on the first u32 field.
        let index_id = extract_index_id(&datastore, &tx, &basic_indices()[0])?;

        test_under_tx_and_commit(&datastore, tx, |tx| {
            // Attempt the update. There should be a unique constraint violation on the string field.
            let err = expect_index_err(update(&datastore, tx, table_id, index_id, &row));
            assert_matches!(err, IndexError::UniqueConstraintViolation(_));
            Ok(())
        })
    }

    /// Checks that update is idempotent.
    #[test]
    fn test_update_idempotent() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;

        // Insert a row.
        let row = &random_row();
        insert(&datastore, &mut tx, table_id, row)?;
        // Seek the index on the first u32 field.
        let index_id = extract_index_id(&datastore, &tx, &basic_indices()[0])?;

        // Test before commit:
        let (_, new_row) = update(&datastore, &mut tx, table_id, index_id, row).expect("update should have succeeded");
        assert_eq!(row, &new_row.to_product_value());
        // Commit.
        let tx_data_1 = commit(&datastore, tx)?;
        let mut tx = begin_mut_tx(&datastore);
        // Test after commit:
        let (_, new_row) = update(&datastore, &mut tx, table_id, index_id, row).expect("update should have succeeded");
        assert_eq!(row, &new_row.to_product_value());
        let tx_data_2 = commit(&datastore, tx)?;
        // Ensure that none of the commits deleted rows in our table.
        for tx_data in [&tx_data_1, &tx_data_2] {
            assert_eq!(tx_data.deletes().find(|(tid, _)| **tid == table_id), None);
        }
        // Ensure that the first commit added the row but that the second didn't.
        for (tx_data, expected_rows) in [(&tx_data_1, vec![row.clone()]), (&tx_data_2, vec![])] {
            let inserted_rows = tx_data
                .inserts()
                .find(|(tid, _)| **tid == table_id)
                .map(|(_, pvs)| pvs.to_vec())
                .unwrap_or_default();
            assert_eq!(inserted_rows, expected_rows);
        }

        Ok(())
    }

    /// Checks that update successfully uses sequences.
    #[test]
    fn test_update_uses_sequences() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;

        // Insert a row.
        let mut row = random_row();
        row.elements[0] = 0u32.into();
        insert(&datastore, &mut tx, table_id, &row)?;

        // Seek the index on the string field.
        let index_id = extract_index_id(&datastore, &tx, &basic_indices()[1])?;

        test_under_tx_and_commit(&datastore, tx, |tx| {
            let mut row = row.clone();
            let (seq_val, new_row) =
                update(&datastore, tx, table_id, index_id, &row).expect("update should have succeeded");
            let new_row = new_row.to_product_value();
            assert_eq!(&seq_val, &new_row.elements[0]);
            row.elements[0] = seq_val;
            assert_eq!(row, new_row);
            Ok(())
        })
    }

    #[test]
    /// Test that two read-only TXes can operate concurrently without deadlock or blocking,
    /// and that both observe correct results for a simple table scan.
    fn test_read_only_tx_shared_lock() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row1 = u32_str_u32(1, "Foo", 18);
        insert(&datastore, &mut tx, table_id, &row1)?;
        let row2 = u32_str_u32(2, "Bar", 20);
        insert(&datastore, &mut tx, table_id, &row2)?;
        datastore.commit_mut_tx(tx)?;

        // create multiple read only tx, and use them together.
        let read_tx_1 = datastore.begin_tx(Workload::Internal);
        let read_tx_2 = datastore.begin_tx(Workload::Internal);
        let rows = &[row1, row2];
        assert_eq!(&all_rows_tx(&read_tx_2, table_id), rows);
        assert_eq!(&all_rows_tx(&read_tx_1, table_id), rows);
        read_tx_2.release();
        read_tx_1.release();
        Ok(())
    }

    #[test]
    fn test_scheduled_table_insert_and_update() -> ResultTest<()> {
        let table_id = TableId::SENTINEL;
        // Build the minimal schema that is a valid scheduler table.
        let schema = TableSchema::new(
            table_id,
            "Foo".into(),
            vec![
                ColumnSchema {
                    table_id,
                    col_pos: 0.into(),
                    col_name: "id".into(),
                    col_type: AlgebraicType::U64,
                },
                ColumnSchema {
                    table_id,
                    col_pos: 1.into(),
                    col_name: "at".into(),
                    col_type: ScheduleAt::get_type(),
                },
            ],
            vec![IndexSchema {
                table_id,
                index_id: IndexId::SENTINEL,
                index_name: "id_idx".into(),
                index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: 0.into() }),
            }],
            vec![ConstraintSchema {
                table_id,
                constraint_id: ConstraintId::SENTINEL,
                constraint_name: "id_unique".into(),
                data: ConstraintData::Unique(UniqueConstraintData { columns: 0.into() }),
            }],
            vec![],
            StTableType::User,
            StAccess::Public,
            Some(ScheduleSchema {
                table_id,
                schedule_id: ScheduleId::SENTINEL,
                schedule_name: "schedule".into(),
                reducer_name: "reducer".into(),
                at_column: 1.into(),
            }),
            Some(0.into()),
        );

        // Create the table.
        let datastore = get_datastore()?;
        let mut tx = begin_mut_tx(&datastore);
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let index_id = datastore
            .index_id_from_name_mut_tx(&tx, "id_idx")?
            .expect("there should be an index with this name");

        // Make us a row and insert + identity update.
        let row = &product![
            24u64,
            value_serialize(&ScheduleAt::Interval(TimeDuration::from_micros(42)))
        ];
        let row = &to_vec(row).unwrap();
        let (_, _, insert_flags) = datastore.insert_mut_tx(&mut tx, table_id, row)?;
        let (_, _, update_flags) = datastore.update_mut_tx(&mut tx, table_id, index_id, row)?;

        // The whole point of the test.
        assert!(insert_flags.is_scheduler_table);
        assert!(update_flags.is_scheduler_table);

        Ok(())
    }

    #[test]
    fn test_row_level_security() -> ResultTest<()> {
        let (_, mut tx, table_id) = setup_table()?;

        let rls = RowLevelSecuritySchema {
            sql: "SELECT * FROM bar".into(),
            table_id,
        };
        tx.create_row_level_security(rls.clone())?;

        let result = tx.row_level_security_for_table_id(table_id)?;
        assert_eq!(
            result,
            vec![RowLevelSecuritySchema {
                sql: "SELECT * FROM bar".into(),
                table_id,
            }]
        );

        tx.drop_row_level_security(rls.sql)?;
        assert_eq!(tx.row_level_security_for_table_id(table_id)?, []);

        Ok(())
    }

    #[test]
    fn test_set_semantics() -> ResultTest<()> {
        let col_schema = |col_name, col_pos| ColumnSchema {
            table_id: TableId::SENTINEL,
            col_pos,
            col_name,
            col_type: AlgebraicType::U8,
        };

        // Create a table schema for (a: u8, b: u8)
        let table_schema = |primary_key, constraint: Option<_>| {
            TableSchema::new(
                TableId::SENTINEL,
                "Foo".into(),
                vec![col_schema("a".into(), 0.into()), col_schema("b".into(), 1.into())],
                vec![],
                constraint.map(|cs| vec![cs]).unwrap_or_default(),
                vec![],
                StTableType::User,
                StAccess::Public,
                None,
                primary_key,
            )
        };
        let table_schema_no_constraints = || table_schema(None, None);
        let table_schema_pk = || table_schema(Some(0.into()), None);
        let table_schema_unique_constraint = || {
            table_schema(
                None,
                Some(ConstraintSchema {
                    table_id: TableId::SENTINEL,
                    constraint_id: ConstraintId::SENTINEL,
                    constraint_name: "a_unique".into(),
                    data: ConstraintData::Unique(UniqueConstraintData {
                        columns: ColSet::from_iter([0]),
                    }),
                }),
            )
        };

        fn create_table(schema: TableSchema) -> ResultTest<(Locking, TableId)> {
            let datastore = get_datastore()?;
            let mut tx = begin_mut_tx(&datastore);
            let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
            datastore.commit_mut_tx(tx)?;
            Ok((datastore, table_id))
        }

        fn insert_rows(datastore: &Locking, rows: Vec<ProductValue>, table_id: TableId) -> ResultTest<()> {
            let mut tx = begin_mut_tx(datastore);
            for row in rows {
                datastore.insert_mut_tx(&mut tx, table_id, row.to_bsatn_vec()?.as_slice())?;
            }
            datastore.commit_mut_tx(tx)?;
            Ok(())
        }

        fn assert_rows(datastore: &Locking, table_id: TableId, rows: Vec<ProductValue>) -> ResultTest<()> {
            let tx = datastore.begin_tx(Workload::ForTests);
            for (actual, expected) in datastore.iter_tx(&tx, table_id)?.zip_eq(rows.into_iter()) {
                assert_eq!(actual.to_bsatn_vec()?, expected.to_bsatn_vec()?);
            }
            Ok(())
        }

        // Assert the datastore implements set semantics when inserting the same row
        fn assert_set_semantics_for_table(schema: impl Fn() -> TableSchema) -> ResultTest<()> {
            |(datastore, table_id)| -> ResultTest<()> {
                insert_rows(
                    &datastore,
                    vec![
                        // Insert one row
                        product!(1u8, 2u8),
                    ],
                    table_id,
                )?;
                assert_rows(
                    &datastore,
                    table_id,
                    vec![
                        // Assert one row
                        product!(1u8, 2u8),
                    ],
                )?;
                Ok(())
            }(create_table(schema())?)?;

            |(datastore, table_id)| -> ResultTest<()> {
                insert_rows(
                    &datastore,
                    vec![
                        // Insert two equal rows in the same tx
                        product!(1u8, 2u8),
                        product!(1u8, 2u8),
                    ],
                    table_id,
                )?;
                assert_rows(
                    &datastore,
                    table_id,
                    vec![
                        // Assert one row
                        product!(1u8, 2u8),
                    ],
                )?;
                Ok(())
            }(create_table(schema())?)?;

            |(datastore, table_id)| -> ResultTest<()> {
                insert_rows(
                    &datastore,
                    vec![
                        // Insert one row
                        product!(1u8, 2u8),
                    ],
                    table_id,
                )?;
                insert_rows(
                    &datastore,
                    vec![
                        // Insert same row in different tx
                        product!(1u8, 2u8),
                    ],
                    table_id,
                )?;
                assert_rows(
                    &datastore,
                    table_id,
                    vec![
                        // Assert one row
                        product!(1u8, 2u8),
                    ],
                )?;
                Ok(())
            }(create_table(schema())?)?;

            Ok(())
        }

        assert_set_semantics_for_table(table_schema_pk)?;
        assert_set_semantics_for_table(table_schema_unique_constraint)?;
        assert_set_semantics_for_table(table_schema_no_constraints)?;

        Ok(())
    }

    // TODO: Add the following tests
    // - Create index with unique constraint and immediately insert a row that violates the constraint before committing.
    // - Create a tx that inserts 2000 rows with an auto_inc column
    // - Create a tx that inserts 2000 rows with an auto_inc column and then rolls back
    // - Test creating sequences pre_commit, post_commit, post_rollback
}
