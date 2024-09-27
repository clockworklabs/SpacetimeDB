use super::{
    committed_state::CommittedState,
    mut_tx::MutTxId,
    sequence::SequencesState,
    state_view::{Iter, IterByColRange, StateView},
    tx::TxId,
    tx_state::TxState,
};
use crate::{
    db::{
        datastore::{
            system_tables::{
                read_addr_from_col, read_bytes_from_col, read_hash_from_col, read_identity_from_col,
                system_table_schema, ModuleKind, StClientRow, StModuleFields, StModuleRow, StTableFields, ST_CLIENT_ID,
                ST_MODULE_ID, ST_TABLE_ID,
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
use spacetimedb_lib::db::auth::StAccess;
use spacetimedb_lib::{Address, Identity};
use spacetimedb_primitives::{ColList, ConstraintId, IndexId, SequenceId, TableId};
use spacetimedb_sats::{bsatn, buffer::BufReader, AlgebraicValue, ProductValue};
use spacetimedb_schema::schema::{IndexSchema, SequenceSchema, TableSchema};
use spacetimedb_snapshot::ReconstructedSnapshot;
use spacetimedb_table::{
    indexes::RowPointer,
    table::{RowRef, Table},
};
use std::time::{Duration, Instant};
use std::{borrow::Cow, sync::Arc};
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
    /// The address of this database.
    database_address: Address,
}

impl Locking {
    pub fn new(database_address: Address) -> Self {
        Self {
            committed_state: <_>::default(),
            sequence_state: <_>::default(),
            database_address,
        }
    }

    /// IMPORTANT! This the most delicate function in the entire codebase.
    /// DO NOT CHANGE UNLESS YOU KNOW WHAT YOU'RE DOING!!!
    pub fn bootstrap(database_address: Address) -> Result<Self> {
        log::trace!("DATABASE: BOOTSTRAPPING SYSTEM TABLES...");

        // NOTE! The bootstrapping process does not take plan in a transaction.
        // This is intentional.
        let datastore = Self::new(database_address);
        let mut commit_state = datastore.committed_state.write_arc();
        let database_address = datastore.database_address;
        // TODO(cloutiertyler): One thing to consider in the future is, should
        // we persist the bootstrap transaction in the message log? My intuition
        // is no, because then if we change the schema of the system tables we
        // would need to migrate that data, whereas since the tables are defined
        // in the code we don't have that issue. We may have other issues though
        // for code that relies on the old schema...

        // Create the system tables and insert information about themselves into
        commit_state.bootstrap_system_tables(database_address)?;
        // The database tables are now initialized with the correct data.
        // Now we have to build our in memory structures.
        commit_state.build_sequence_state(&mut datastore.sequence_state.lock())?;
        commit_state.build_indexes()?;

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
            database_address: self.database_address,
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
            database_address,
            tx_offset,
            blob_store,
            tables,
            ..
        } = snapshot;

        let datastore = Self::new(database_address);
        let mut committed_state = datastore.committed_state.write_arc();
        committed_state.blob_store = blob_store;

        let ctx = ExecutionContext::internal(datastore.database_address);

        // Note that `tables` is a `BTreeMap`, and so iterates in increasing order.
        // This means that we will instantiate and populate the system tables before any user tables.
        for (table_id, pages) in tables {
            let schema = match system_table_schema(table_id) {
                Some(schema) => Arc::new(schema),
                // In this case, `schema_for_table` will never see a cached schema,
                // as the committed state is newly constructed and we have not accessed this schema yet.
                // As such, this call will compute and save the schema from `st_table` and friends.
                None => committed_state.schema_for_table(&ctx, table_id)?,
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
                .with_label_values(&database_address, &table_id.0, &schema.table_name)
                .set(table.row_count as i64);

            // Also set the `rdb_table_size` metric for the table.
            let table_size = table.bytes_occupied_overestimate();
            DB_METRICS
                .rdb_table_size
                .with_label_values(&database_address, &table_id.into(), &schema.table_name)
                .set(table_size as i64);
        }

        // Fix up auto_inc IDs in the cached system table schemas.
        committed_state.reset_system_table_schemas(database_address)?;

        // The next TX offset after restoring from a snapshot is one greater than the snapshotted offset.
        committed_state.next_tx_offset = tx_offset + 1;

        Ok(datastore)
    }

    /// Returns a list over all the currently connected clients,
    /// reading from the `st_clients` system table.
    pub fn connected_clients<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a TxId,
    ) -> Result<impl Iterator<Item = Result<(Identity, Address)>> + 'a> {
        let iter = self.iter_tx(ctx, tx, ST_CLIENT_ID)?.map(|row_ref| {
            let row = StClientRow::try_from(row_ref)?;
            Ok((row.identity.0, row.address.0))
        });

        Ok(iter)
    }

    pub(crate) fn alter_table_access_mut_tx(&self, tx: &mut MutTxId, name: Box<str>, access: StAccess) -> Result<()> {
        let table_id = self
            .table_id_from_name_mut_tx(tx, &name)?
            .ok_or_else(|| TableError::NotFound(name.into()))?;

        tx.alter_table_access(self.database_address, table_id, access)
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

    fn begin_tx(&self) -> Self::Tx {
        let timer = Instant::now();

        let committed_state_shared_lock = self.committed_state.read_arc();
        let lock_wait_time = timer.elapsed();
        Self::Tx {
            committed_state_shared_lock,
            lock_wait_time,
            timer,
        }
    }

    fn release_tx(&self, ctx: &ExecutionContext, tx: Self::Tx) {
        tx.release(ctx);
    }
}

impl TxDatastore for Locking {
    type Iter<'a> = Iter<'a> where Self: 'a;
    type IterByColEq<'a, 'r> = IterByColRange<'a, &'r AlgebraicValue> where Self: 'a;
    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>> = IterByColRange<'a, R> where Self: 'a;

    fn iter_tx<'a>(&'a self, ctx: &'a ExecutionContext, tx: &'a Self::Tx, table_id: TableId) -> Result<Self::Iter<'a>> {
        tx.iter(ctx, table_id)
    }

    fn iter_by_col_range_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<Self::IterByColRange<'a, R>> {
        tx.iter_by_col_range(ctx, table_id, cols.into(), range)
    }

    fn iter_by_col_eq_tx<'a, 'r>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>> {
        tx.iter_by_col_eq(ctx, table_id, cols, value)
    }

    fn table_id_exists_tx(&self, tx: &Self::Tx, table_id: &TableId) -> bool {
        tx.table_name(*table_id).is_some()
    }

    fn table_id_from_name_tx(&self, tx: &Self::Tx, table_name: &str) -> Result<Option<TableId>> {
        tx.table_id_from_name(table_name, self.database_address)
    }

    fn table_name_from_id_tx<'a>(&'a self, tx: &'a Self::Tx, table_id: TableId) -> Result<Option<Cow<'a, str>>> {
        Ok(tx.table_name(table_id).map(Cow::Borrowed))
    }

    fn schema_for_table_tx(&self, tx: &Self::Tx, table_id: TableId) -> Result<Arc<TableSchema>> {
        tx.schema_for_table(&ExecutionContext::internal(self.database_address), table_id)
    }

    fn get_all_tables_tx(&self, ctx: &ExecutionContext, tx: &Self::Tx) -> Result<Vec<Arc<TableSchema>>> {
        self.iter_tx(ctx, tx, ST_TABLE_ID)?
            .map(|row_ref| {
                let table_id = row_ref.read_col(StTableFields::TableId)?;
                self.schema_for_table_tx(tx, table_id)
            })
            .collect()
    }

    fn metadata(&self, ctx: &ExecutionContext, tx: &Self::Tx) -> Result<Option<Metadata>> {
        self.iter_tx(ctx, tx, ST_MODULE_ID)?
            .next()
            .map(metadata_from_row)
            .transpose()
    }

    fn program(&self, ctx: &ExecutionContext, tx: &Self::Tx) -> Result<Option<Program>> {
        self.iter_tx(ctx, tx, ST_MODULE_ID)?
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
    fn create_table_mut_tx(&self, tx: &mut Self::MutTx, schema: TableSchema) -> Result<TableId> {
        tx.create_table(schema, self.database_address)
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
        tx.row_type_for_table(table_id, self.database_address)
    }

    /// IMPORTANT! This function is relatively expensive, and much more
    /// expensive than `row_type_for_table_mut_tx`.  Prefer
    /// `row_type_for_table_mut_tx` if you only need to access the `ProductType`
    /// of the table.
    fn schema_for_table_mut_tx(&self, tx: &Self::MutTx, table_id: TableId) -> Result<Arc<TableSchema>> {
        tx.schema_for_table(&ExecutionContext::internal(self.database_address), table_id)
    }

    /// This function is relatively expensive because it needs to be
    /// transactional, however we don't expect to be dropping tables very often.
    fn drop_table_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId) -> Result<()> {
        tx.drop_table(table_id, self.database_address)
    }

    fn rename_table_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId, new_name: &str) -> Result<()> {
        tx.rename_table(table_id, new_name, self.database_address)
    }

    fn table_id_from_name_mut_tx(&self, tx: &Self::MutTx, table_name: &str) -> Result<Option<TableId>> {
        tx.table_id_from_name(table_name, self.database_address)
    }

    fn table_name_from_id_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
    ) -> Result<Option<Cow<'a, str>>> {
        tx.table_name_from_id(ctx, table_id)
            .map(|opt| opt.map(|s| Cow::Owned(s.into())))
    }

    fn create_index_mut_tx(&self, tx: &mut Self::MutTx, index_schema: IndexSchema, is_unique: bool) -> Result<IndexId> {
        let ctx = &ExecutionContext::internal(self.database_address);
        tx.create_index(ctx, index_schema, is_unique)
    }

    fn drop_index_mut_tx(&self, tx: &mut Self::MutTx, index_id: IndexId) -> Result<()> {
        tx.drop_index(index_id, self.database_address)
    }

    fn index_id_from_name_mut_tx(&self, tx: &Self::MutTx, index_name: &str) -> Result<Option<IndexId>> {
        tx.index_id_from_name(index_name, self.database_address)
    }

    fn get_next_sequence_value_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> Result<i128> {
        tx.get_next_sequence_value(seq_id, self.database_address)
    }

    fn create_sequence_mut_tx(&self, tx: &mut Self::MutTx, sequence_schema: SequenceSchema) -> Result<SequenceId> {
        tx.create_sequence(sequence_schema, self.database_address)
    }

    fn drop_sequence_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> Result<()> {
        tx.drop_sequence(seq_id, self.database_address)
    }

    fn sequence_id_from_name_mut_tx(&self, tx: &Self::MutTx, sequence_name: &str) -> Result<Option<SequenceId>> {
        tx.sequence_id_from_name(sequence_name, self.database_address)
    }

    fn drop_constraint_mut_tx(&self, tx: &mut Self::MutTx, constraint_id: ConstraintId) -> Result<()> {
        let ctx = &ExecutionContext::internal(self.database_address);
        tx.drop_constraint(ctx, constraint_id)
    }

    fn constraint_id_from_name(&self, tx: &Self::MutTx, constraint_name: &str) -> Result<Option<ConstraintId>> {
        let ctx = &ExecutionContext::internal(self.database_address);
        tx.constraint_id_from_name(ctx, constraint_name)
    }

    fn iter_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
    ) -> Result<Self::Iter<'a>> {
        tx.iter(ctx, table_id)
    }

    fn iter_by_col_range_mut_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<Self::IterByColRange<'a, R>> {
        tx.iter_by_col_range(ctx, table_id, cols.into(), range)
    }

    fn iter_by_col_eq_mut_tx<'a, 'r>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>> {
        tx.iter_by_col_eq(ctx, table_id, cols.into(), value)
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
        mut row: ProductValue,
    ) -> Result<(AlgebraicValue, RowRef<'a>)> {
        let (gens, row_ref) = tx.insert(table_id, &mut row, self.database_address)?;
        Ok((gens, row_ref.collapse()))
    }

    fn table_id_exists_mut_tx(&self, tx: &Self::MutTx, table_id: &TableId) -> bool {
        tx.table_name(*table_id).is_some()
    }

    fn metadata_mut_tx(&self, tx: &Self::MutTx) -> Result<Option<Metadata>> {
        let ctx = ExecutionContext::internal(self.database_address);
        tx.iter(&ctx, ST_MODULE_ID)?.next().map(metadata_from_row).transpose()
    }

    fn update_program(&self, tx: &mut Self::MutTx, program_kind: ModuleKind, program: Program) -> Result<()> {
        let ctx = ExecutionContext::internal(self.database_address);
        let old = tx
            .iter(&ctx, ST_MODULE_ID)?
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
                tx.insert(ST_MODULE_ID, &mut row.into(), self.database_address)
                    .map(drop)
            }

            None => Err(anyhow!("database {} improperly initialized: no metadata", self.database_address).into()),
        }
    }
}

/// This utility is responsible for recording all transaction metrics.
pub(super) fn record_metrics(
    ctx: &ExecutionContext,
    tx_timer: Instant,
    lock_wait_time: Duration,
    committed: bool,
    tx_data: Option<&TxData>,
    committed_state: Option<&CommittedState>,
) {
    let workload = &ctx.workload();
    let db = &ctx.database();
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

    /// Update table rows and table size gauges,
    /// and sets them to zero if no table is present.
    fn update_table_gauges(db: &Address, table_id: &TableId, table_name: &str, table: Option<&Table>) {
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
            update_table_gauges(db, table_id, table_name, committed_state.get_table(*table_id));
            // Increment rows inserted counter
            DB_METRICS
                .rdb_num_rows_inserted
                .with_label_values(workload, db, reducer, &table_id.0, table_name)
                .inc_by(inserts.len() as u64);
        }
        for (table_id, table_name, deletes) in tx_data.deletes_with_table_name() {
            update_table_gauges(db, table_id, table_name, committed_state.get_table(*table_id));
            // Increment rows deleted counter
            DB_METRICS
                .rdb_num_rows_deleted
                .with_label_values(workload, db, reducer, &table_id.0, table_name)
                .inc_by(deletes.len() as u64);
        }
    }
}

impl MutTx for Locking {
    type MutTx = MutTxId;

    /// Note: We do not use the isolation level here because this implementation
    /// guarantees the highest isolation level, Serializable.
    fn begin_mut_tx(&self, _isolation_level: IsolationLevel) -> Self::MutTx {
        let timer = Instant::now();

        let committed_state_write_lock = self.committed_state.write_arc();
        let sequence_state_lock = self.sequence_state.lock_arc();
        let lock_wait_time = timer.elapsed();
        MutTxId {
            committed_state_write_lock,
            sequence_state_lock,
            tx_state: TxState::default(),
            lock_wait_time,
            timer,
        }
    }

    fn rollback_mut_tx(&self, ctx: &ExecutionContext, tx: Self::MutTx) {
        tx.rollback(ctx);
    }

    fn commit_mut_tx(&self, ctx: &ExecutionContext, tx: Self::MutTx) -> Result<Option<TxData>> {
        Ok(Some(tx.commit(ctx)))
    }

    #[cfg(test)]
    fn commit_mut_tx_for_test(&self, tx: Self::MutTx) -> crate::db::datastore::Result<Option<TxData>> {
        self.commit_mut_tx(&ExecutionContext::default(), tx)
    }

    #[cfg(test)]
    fn rollback_mut_tx_for_test(&self, tx: Self::MutTx) {
        self.rollback_mut_tx(&ExecutionContext::default(), tx)
    }
}

impl Locking {
    pub fn rollback_mut_tx_downgrade(&self, ctx: &ExecutionContext, tx: MutTxId) -> TxId {
        tx.rollback_downgrade(ctx)
    }

    pub fn commit_mut_tx_downgrade(&self, ctx: &ExecutionContext, tx: MutTxId) -> Result<Option<(TxData, TxId)>> {
        Ok(Some(tx.commit_downgrade(ctx)))
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
    database_address: Address,
    committed_state: Arc<RwLock<CommittedState>>,
    progress: RefCell<F>,
}

impl<F> Replay<F> {
    fn using_visitor<T>(&self, f: impl FnOnce(&mut ReplayVisitor<F>) -> T) -> T {
        let mut committed_state = self.committed_state.write_arc();
        let mut visitor = ReplayVisitor {
            database_address: &self.database_address,
            committed_state: &mut committed_state,
            progress: &mut *self.progress.borrow_mut(),
        };
        f(&mut visitor)
    }

    pub(crate) fn next_tx_offset(&self) -> u64 {
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
    database_address: &'a Address,
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
        let schema = self
            .committed_state
            .schema_for_table(&ExecutionContext::default(), table_id)?;
        ProductValue::decode(schema.get_row_type(), reader)?;
        Ok(())
    }

    fn visit_insert<'a, R: BufReader<'a>>(
        &mut self,
        table_id: TableId,
        reader: &mut R,
    ) -> std::result::Result<Self::Row, Self::Error> {
        let schema = self
            .committed_state
            .schema_for_table(&ExecutionContext::default(), table_id)?;
        let row = ProductValue::decode(schema.get_row_type(), reader)?;

        self.committed_state
            .replay_insert(table_id, &schema, &row)
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
            .with_label_values(self.database_address, &table_id.into(), &schema.table_name)
            .inc();

        Ok(row)
    }

    fn visit_delete<'a, R: BufReader<'a>>(
        &mut self,
        table_id: TableId,
        reader: &mut R,
    ) -> std::result::Result<Self::Row, Self::Error> {
        let schema = self
            .committed_state
            .schema_for_table(&ExecutionContext::default(), table_id)?;
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
            .with_label_values(self.database_address, &table_id.into(), &table_name)
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
        database_address: read_addr_from_col(row, StModuleFields::DatabaseAddress)?,
        owner_identity: read_identity_from_col(row, StModuleFields::OwnerIdentity)?,
        program_hash: read_hash_from_col(row, StModuleFields::ProgramHash)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::datastore::system_tables::{
        system_tables, StColumnRow, StConstraintData, StConstraintFields, StConstraintRow, StIndexAlgorithm,
        StIndexFields, StIndexRow, StScheduledFields, StSequenceFields, StSequenceRow, StTableRow, StVarFields,
        StVarValue, ST_CLIENT_NAME, ST_COLUMN_ID, ST_COLUMN_NAME, ST_CONSTRAINT_ID, ST_CONSTRAINT_NAME, ST_INDEX_ID,
        ST_INDEX_NAME, ST_MODULE_NAME, ST_RESERVED_SEQUENCE_RANGE, ST_SCHEDULED_ID, ST_SCHEDULED_NAME, ST_SEQUENCE_ID,
        ST_SEQUENCE_NAME, ST_TABLE_NAME, ST_VAR_ID, ST_VAR_NAME,
    };
    use crate::db::datastore::traits::{IsolationLevel, MutTx};
    use crate::db::datastore::Result;
    use crate::error::{DBError, IndexError};
    use itertools::Itertools;
    use pretty_assertions::assert_eq;
    use spacetimedb_lib::address::Address;
    use spacetimedb_lib::db::auth::{StAccess, StTableType};
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_lib::resolved_type_via_v9;
    use spacetimedb_primitives::{col_list, ColId, ScheduleId};
    use spacetimedb_sats::{product, AlgebraicType, GroundSpacetimeType};
    use spacetimedb_schema::def::{BTreeAlgorithm, ConstraintData, IndexAlgorithm, UniqueConstraintData};
    use spacetimedb_schema::schema::{ColumnSchema, ConstraintSchema, IndexSchema, SequenceSchema};
    use spacetimedb_table::table::UniqueConstraintViolation;

    /// For the first user-created table, sequences in the system tables start
    /// from this value.
    const FIRST_NON_SYSTEM_ID: u32 = ST_RESERVED_SEQUENCE_RANGE + 1;

    /// Utility to query the system tables and return their concrete table row
    pub struct SystemTableQuery<'a> {
        db: &'a MutTxId,
        ctx: &'a ExecutionContext,
    }

    fn query_st_tables<'a>(ctx: &'a ExecutionContext, tx: &'a MutTxId) -> SystemTableQuery<'a> {
        SystemTableQuery { db: tx, ctx }
    }

    impl SystemTableQuery<'_> {
        pub fn scan_st_tables(&self) -> Result<Vec<StTableRow>> {
            Ok(self
                .db
                .iter(self.ctx, ST_TABLE_ID)?
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
                .iter_by_col_eq(self.ctx, ST_TABLE_ID, cols.into(), value)?
                .map(|row| StTableRow::try_from(row).unwrap())
                .sorted_by_key(|x| x.table_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_columns(&self) -> Result<Vec<StColumnRow>> {
            Ok(self
                .db
                .iter(self.ctx, ST_COLUMN_ID)?
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
                .iter_by_col_eq(self.ctx, ST_COLUMN_ID, cols.into(), value)?
                .map(|row| StColumnRow::try_from(row).unwrap())
                .sorted_by_key(|x| (x.table_id, x.col_pos))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_constraints(&self) -> Result<Vec<StConstraintRow>> {
            Ok(self
                .db
                .iter(self.ctx, ST_CONSTRAINT_ID)?
                .map(|row| StConstraintRow::try_from(row).unwrap())
                .sorted_by_key(|x| x.constraint_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_sequences(&self) -> Result<Vec<StSequenceRow>> {
            Ok(self
                .db
                .iter(self.ctx, ST_SEQUENCE_ID)?
                .map(|row| StSequenceRow::try_from(row).unwrap())
                .sorted_by_key(|x| (x.table_id, x.sequence_id))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_indexes(&self) -> Result<Vec<StIndexRow>> {
            Ok(self
                .db
                .iter(self.ctx, ST_INDEX_ID)?
                .map(|row| StIndexRow::try_from(row).unwrap())
                .sorted_by_key(|x| x.index_id)
                .collect::<Vec<_>>())
        }
    }

    fn u32_str_u32(a: u32, b: &str, c: u32) -> ProductValue {
        product![a, b, c]
    }

    fn get_datastore() -> Result<Locking> {
        Locking::bootstrap(Address::zero())
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

    #[rustfmt::skip]
    fn basic_table_schema_cols() -> [ColRow<'static>; 3] {
        let table = FIRST_NON_SYSTEM_ID;
        [
            ColRow { table, pos: 0, name: "id", ty: AlgebraicType::U32 },
            ColRow { table, pos: 1, name: "name", ty: AlgebraicType::String },
            ColRow { table, pos: 2, name: "age", ty: AlgebraicType::U32 },
        ]
    }

    fn basic_table_schema() -> TableSchema {
        TableSchema::new(
            TableId::SENTINEL,
            "Foo".into(),
            map_array(basic_table_schema_cols()),
            vec![
                IndexSchema {
                    index_id: IndexId::SENTINEL,
                    table_id: TableId::SENTINEL,
                    index_name: "id_idx".into(),
                    index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![0] }),
                },
                IndexSchema {
                    index_id: IndexId::SENTINEL,
                    table_id: TableId::SENTINEL,
                    index_name: "name_idx".into(),
                    index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![1] }),
                },
            ],
            vec![
                ConstraintSchema {
                    table_id: TableId::SENTINEL,
                    constraint_id: ConstraintId::SENTINEL,
                    constraint_name: "id_constraint".into(),
                    data: ConstraintData::Unique(UniqueConstraintData {
                        columns: col_list![0].into(),
                    }),
                },
                ConstraintSchema {
                    table_id: TableId::SENTINEL,
                    constraint_id: ConstraintId::SENTINEL,
                    constraint_name: "name_constraint".into(),
                    data: ConstraintData::Unique(UniqueConstraintData {
                        columns: col_list![1].into(),
                    }),
                },
            ],
            vec![SequenceSchema {
                sequence_id: SequenceId::SENTINEL,
                table_id: TableId::SENTINEL,
                col_pos: 0.into(),
                sequence_name: "id_sequence".into(),
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
                IndexRow { id: seq_start,     table, col: ColList::new(0.into()), name: "id_idx", },
                IndexRow { id: seq_start + 1, table, col: ColList::new(1.into()), name: "name_idx", },
            ]),
            map_array([
                ConstraintRow { constraint_id: seq_start,     table_id: table, unique_columns: col(0), constraint_name: "id_constraint" },
                ConstraintRow { constraint_id: seq_start + 1, table_id: table, unique_columns: col(1), constraint_name: "name_constraint" }
            ]),
             map_array([
                SequenceRow { id: seq_start, table, col_pos: 0, name: "id_sequence", start: 1 }
            ]),
            StTableType::User,
            StAccess::Public,
            None,
            None
        )
    }

    fn setup_table() -> ResultTest<(Locking, MutTxId, TableId)> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        Ok((datastore, tx, table_id))
    }

    fn all_rows(datastore: &Locking, tx: &MutTxId, table_id: TableId) -> Vec<ProductValue> {
        datastore
            .iter_mut_tx(&ExecutionContext::default(), tx, table_id)
            .unwrap()
            .map(|r| r.to_product_value().clone())
            .collect()
    }

    fn all_rows_tx(tx: &TxId, table_id: TableId) -> Vec<ProductValue> {
        tx.iter(&ExecutionContext::default(), table_id)
            .unwrap()
            .map(|r| r.to_product_value().clone())
            .collect()
    }

    #[test]
    fn test_bootstrapping_sets_up_tables() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);
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

            ColRow { table: ST_MODULE_ID.into(), pos: 0, name: "database_address", ty: AlgebraicType::bytes() },
            ColRow { table: ST_MODULE_ID.into(), pos: 1, name: "owner_identity", ty: AlgebraicType::bytes() },
            ColRow { table: ST_MODULE_ID.into(), pos: 2, name: "program_kind", ty: AlgebraicType::U8 },
            ColRow { table: ST_MODULE_ID.into(), pos: 3, name: "program_hash", ty: AlgebraicType::bytes() },
            ColRow { table: ST_MODULE_ID.into(), pos: 4, name: "program_bytes", ty: AlgebraicType::bytes() },
            ColRow { table: ST_MODULE_ID.into(), pos: 5, name: "module_version", ty: AlgebraicType::String },

            ColRow { table: ST_CLIENT_ID.into(), pos: 0, name: "identity", ty: AlgebraicType::bytes()},
            ColRow { table: ST_CLIENT_ID.into(), pos: 1, name: "address", ty: AlgebraicType::bytes()},

            ColRow { table: ST_VAR_ID.into(), pos: 0, name: "name", ty: AlgebraicType::String },
            ColRow { table: ST_VAR_ID.into(), pos: 1, name: "value", ty: resolved_type_via_v9::<StVarValue>() },

            ColRow { table: ST_SCHEDULED_ID.into(), pos: 0, name: "schedule_id", ty: ScheduleId::get_type() },
            ColRow { table: ST_SCHEDULED_ID.into(), pos: 1, name: "table_id", ty: TableId::get_type() },
            ColRow { table: ST_SCHEDULED_ID.into(), pos: 2, name: "reducer_name", ty: AlgebraicType::String },
            ColRow { table: ST_SCHEDULED_ID.into(), pos: 3, name: "schedule_name", ty: AlgebraicType::String },
        ]));
        #[rustfmt::skip]
        assert_eq!(query.scan_st_indexes()?, map_array([
            IndexRow { id: 1, table: ST_TABLE_ID.into(), col: col(0), name: "idx_st_table_table_id_unique", },
            IndexRow { id: 2, table: ST_TABLE_ID.into(), col: col(1), name: "idx_st_table_table_name_unique", },
            IndexRow { id: 3, table: ST_COLUMN_ID.into(), col: col_list![0, 1], name: "idx_st_column_table_id_col_pos_unique", },
            IndexRow { id: 4, table: ST_SEQUENCE_ID.into(), col: col(0), name: "idx_st_sequence_sequence_id_unique", },
            IndexRow { id: 5, table: ST_INDEX_ID.into(), col: col(0), name: "idx_st_index_index_id_unique", },
            IndexRow { id: 6, table: ST_CONSTRAINT_ID.into(), col: col(0), name: "idx_st_constraint_constraint_id_unique", },
            IndexRow { id: 7, table: ST_CLIENT_ID.into(), col: col_list![0, 1], name: "idx_st_client_identity_address_unique", },
            IndexRow { id: 8, table: ST_VAR_ID.into(), col: col(0), name: "idx_st_var_name_unique", },
            IndexRow { id: 9, table: ST_SCHEDULED_ID.into(), col: col(0), name: "idx_st_scheduled_schedule_id_unique", },
            IndexRow { id: 10, table: ST_SCHEDULED_ID.into(), col: col(1), name: "idx_st_scheduled_table_id_unique", },
        ]));
        let start = FIRST_NON_SYSTEM_ID as i128;
        #[rustfmt::skip]
        assert_eq!(query.scan_st_sequences()?, map_array_fn(
            [
                SequenceRow { id: 1, table: ST_TABLE_ID.into(), col_pos: 0, name: "seq_st_table_table_id", start },
                SequenceRow { id: 5, table: ST_SEQUENCE_ID.into(), col_pos: 0, name: "seq_st_sequence_sequence_id", start },
                SequenceRow { id: 2, table: ST_INDEX_ID.into(), col_pos: 0, name: "seq_st_index_index_id", start },
                SequenceRow { id: 3, table: ST_CONSTRAINT_ID.into(), col_pos: 0, name: "seq_st_constraint_constraint_id", start },
                SequenceRow { id: 4, table: ST_SCHEDULED_ID.into(), col_pos: 0, name: "seq_st_scheduled_schedule_id", start },
            ],
            |row| StSequenceRow {
                allocated: ST_RESERVED_SEQUENCE_RANGE as i128,
                ..StSequenceRow::from(row)
            }
        ));
        #[rustfmt::skip]
        assert_eq!(query.scan_st_constraints()?, map_array([
            ConstraintRow { constraint_id: 1, table_id: ST_TABLE_ID.into(), unique_columns: col(0), constraint_name: "ct_st_table_table_id_unique" },
            ConstraintRow { constraint_id: 2, table_id: ST_TABLE_ID.into(), unique_columns: col(1), constraint_name: "ct_st_table_table_name_unique" },
            ConstraintRow { constraint_id: 3, table_id: ST_COLUMN_ID.into(), unique_columns: col_list![0, 1], constraint_name: "ct_st_column_table_id_col_pos_unique" },
            ConstraintRow { constraint_id: 4, table_id: ST_SEQUENCE_ID.into(), unique_columns: col(0), constraint_name: "ct_st_sequence_sequence_id_unique" },
            ConstraintRow { constraint_id: 5, table_id: ST_INDEX_ID.into(), unique_columns: col(0), constraint_name: "ct_st_index_index_id_unique" },
            ConstraintRow { constraint_id: 6, table_id: ST_CONSTRAINT_ID.into(), unique_columns: col(0), constraint_name: "ct_st_constraint_constraint_id_unique" },
            ConstraintRow { constraint_id: 7, table_id: ST_CLIENT_ID.into(), unique_columns: col_list![0, 1], constraint_name: "ct_st_client_identity_address_unique" },
            ConstraintRow { constraint_id: 8, table_id: ST_VAR_ID.into(), unique_columns: col(0), constraint_name: "ct_st_var_name_unique" },
            ConstraintRow { constraint_id: 9, table_id: ST_SCHEDULED_ID.into(), unique_columns: col(0), constraint_name: "ct_st_scheduled_schedule_id_unique" },
            ConstraintRow { constraint_id: 10, table_id: ST_SCHEDULED_ID.into(), unique_columns: col(1), constraint_name: "ct_st_scheduled_table_id_unique" },
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

        datastore.rollback_mut_tx_for_test(tx);
        Ok(())
    }

    #[test]
    fn test_create_table_pre_commit() -> ResultTest<()> {
        let (_, tx, table_id) = setup_table()?;
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

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
        datastore.commit_mut_tx_for_test(tx)?;
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

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
        datastore.rollback_mut_tx_for_test(tx);
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        assert!(
            !datastore.table_id_exists_mut_tx(&tx, &table_id),
            "Table should not exist"
        );
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let table_rows = query.scan_st_tables_by_col(ColId(0), &table_id.into())?;
        assert_eq!(table_rows, []);
        let column_rows = query.scan_st_columns_by_col(ColId(0), &table_id.into())?;
        assert_eq!(column_rows, []);
        Ok(())
    }

    fn verify_schemas_consistent(tx: &mut MutTxId, table_id: TableId) {
        let ctx = &ExecutionContext::default();
        let s1 = tx.get_schema(table_id).expect("should exist");
        let s2 = tx.schema_for_table_raw(ctx, table_id).expect("should exist");
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
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        verify_schemas_consistent(&mut tx, table_id);
        let schema = &*datastore.schema_for_table_mut_tx(&tx, table_id)?;
        #[rustfmt::skip]
        assert_eq!(schema, &basic_table_schema_created(table_id));
        Ok(())
    }

    #[test]
    fn test_schema_for_table_alter_indexes() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx_for_test(tx)?;

        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
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
        datastore.commit_mut_tx_for_test(tx)?;

        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
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
                index_name: "id_index".into(),
                index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![0] }),
            },
            true,
        )?;

        verify_schemas_consistent(&mut tx, table_id);

        let expected_indexes = [IndexRow {
            id: ST_RESERVED_SEQUENCE_RANGE + dropped_indexes + 1,
            table: FIRST_NON_SYSTEM_ID,
            col: col_list![0],
            name: "id_index",
        }]
        .map(Into::into);
        assert_eq!(
            datastore.schema_for_table_mut_tx(&tx, table_id)?.indexes,
            expected_indexes,
            "created index should be present in schema pre-commit"
        );

        datastore.commit_mut_tx_for_test(tx)?;

        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        assert_eq!(
            datastore.schema_for_table_mut_tx(&tx, table_id)?.indexes,
            expected_indexes,
            "created index should be present in schema post-commit"
        );

        datastore.commit_mut_tx_for_test(tx)?;

        Ok(())
    }

    #[test]
    fn test_schema_for_table_rollback() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.rollback_mut_tx_for_test(tx);
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let schema = datastore.schema_for_table_mut_tx(&tx, table_id);
        assert!(schema.is_err());
        Ok(())
    }

    #[test]
    fn test_insert_pre_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_insert_wrong_schema_pre_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = product!(0, "Foo");
        assert!(datastore.insert_mut_tx(&mut tx, table_id, row).is_err());
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![]);
        Ok(())
    }

    #[test]
    fn test_insert_post_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, u32_str_u32(0, "Foo", 18))?;
        datastore.commit_mut_tx_for_test(tx)?;
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_insert_post_rollback() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        let row = u32_str_u32(15, "Foo", 18); // 15 is ignored.
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.rollback_mut_tx_for_test(tx);
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![]);
        Ok(())
    }

    #[test]
    fn test_insert_commit_delete_insert() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let created_row = u32_str_u32(1, "Foo", 18);
        let num_deleted = datastore.delete_by_rel_mut_tx(&mut tx, table_id, [created_row]);
        assert_eq!(num_deleted, 1);
        assert_eq!(all_rows(&datastore, &tx, table_id).len(), 0);
        let created_row = u32_str_u32(1, "Foo", 19);
        datastore.insert_mut_tx(&mut tx, table_id, created_row)?;
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 19)]);
        Ok(())
    }

    #[test]
    fn test_insert_delete_insert_delete_insert() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(1, "Foo", 18); // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
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
            datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
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
        datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
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
        datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
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
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
        datastore.rollback_mut_tx_for_test(tx);
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(2, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_create_index_pre_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx_for_test(tx)?;

        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx_for_test(tx)?;

        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let index_def = IndexSchema {
            index_id: IndexId::SENTINEL,
            table_id,
            index_name: "age_idx".into(),
            index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![2] }),
        };
        // TODO: it's slightly incorrect to create an index with `is_unique: true` without creating a corresponding constraint.
        // But the `Table` crate allows it for now.
        datastore.create_index_mut_tx(&mut tx, index_def, true)?;
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);
        let seq_start = FIRST_NON_SYSTEM_ID;
        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 1, table: ST_TABLE_ID.into(), col: col(0), name: "idx_st_table_table_id_unique", },
            IndexRow { id: 2, table: ST_TABLE_ID.into(), col: col(1), name: "idx_st_table_table_name_unique", },
            IndexRow { id: 3, table: ST_COLUMN_ID.into(), col: col_list![0, 1], name: "idx_st_column_table_id_col_pos_unique", },
            IndexRow { id: 4, table: ST_SEQUENCE_ID.into(), col: col(0), name: "idx_st_sequence_sequence_id_unique", },
            IndexRow { id: 5, table: ST_INDEX_ID.into(), col: col(0), name: "idx_st_index_index_id_unique", },
            IndexRow { id: 6, table: ST_CONSTRAINT_ID.into(), col: col(0), name: "idx_st_constraint_constraint_id_unique", },
            IndexRow { id: 7, table: ST_CLIENT_ID.into(), col: col_list![0, 1], name: "idx_st_client_identity_address_unique", },
            IndexRow { id: 8, table: ST_VAR_ID.into(), col: col(0), name: "idx_st_var_name_unique", },
            IndexRow { id: 9, table: ST_SCHEDULED_ID.into(), col: col(0), name: "idx_st_scheduled_schedule_id_unique", },
            IndexRow { id: 10, table: ST_SCHEDULED_ID.into(), col: col(1), name: "idx_st_scheduled_table_id_unique", },
            IndexRow { id: seq_start,     table: FIRST_NON_SYSTEM_ID, col: col(0), name: "id_idx",  },
            IndexRow { id: seq_start + 1, table: FIRST_NON_SYSTEM_ID, col: col(1), name: "name_idx",  },
            IndexRow { id: seq_start + 2, table: FIRST_NON_SYSTEM_ID, col: col(2), name: "age_idx",  },
        ].map(Into::into));
        let row = u32_str_u32(0, "Bar", 18); // 0 will be ignored.
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
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
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let index_def = IndexSchema {
            index_id: IndexId::SENTINEL,
            table_id,
            index_name: "age_idx".into(),
            index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![2] }),
        };
        datastore.create_index_mut_tx(&mut tx, index_def, true)?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let seq_start = FIRST_NON_SYSTEM_ID;
        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 1, table: ST_TABLE_ID.into(), col: col(0), name: "idx_st_table_table_id_unique", },
            IndexRow { id: 2, table: ST_TABLE_ID.into(), col: col(1), name: "idx_st_table_table_name_unique", },
            IndexRow { id: 3, table: ST_COLUMN_ID.into(), col: col_list![0, 1], name: "idx_st_column_table_id_col_pos_unique", },
            IndexRow { id: 4, table: ST_SEQUENCE_ID.into(), col: col(0), name: "idx_st_sequence_sequence_id_unique", },
            IndexRow { id: 5, table: ST_INDEX_ID.into(), col: col(0), name: "idx_st_index_index_id_unique", },
            IndexRow { id: 6, table: ST_CONSTRAINT_ID.into(), col: col(0), name: "idx_st_constraint_constraint_id_unique", },
            IndexRow { id: 7, table: ST_CLIENT_ID.into(), col: col_list![0, 1], name: "idx_st_client_identity_address_unique", },
            IndexRow { id: 8, table: ST_VAR_ID.into(), col: col(0), name: "idx_st_var_name_unique", },
            IndexRow { id: 9, table: ST_SCHEDULED_ID.into(), col: col(0), name: "idx_st_scheduled_schedule_id_unique", },
            IndexRow { id: 10, table: ST_SCHEDULED_ID.into(), col: col(1), name: "idx_st_scheduled_table_id_unique", },
            IndexRow { id: seq_start    , table: FIRST_NON_SYSTEM_ID, col: col(0), name: "id_idx" },
            IndexRow { id: seq_start + 1, table: FIRST_NON_SYSTEM_ID, col: col(1), name: "name_idx" },
            IndexRow { id: seq_start + 2, table: FIRST_NON_SYSTEM_ID, col: col(2), name: "age_idx" },
        ].map(Into::into));
        let row = u32_str_u32(0, "Bar", 18); // 0 will be ignored.
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
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
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let index_def = IndexSchema {
            index_id: IndexId::SENTINEL,
            table_id,
            index_name: "age_idx".into(),
            index_algorithm: IndexAlgorithm::BTree(BTreeAlgorithm { columns: col_list![2] }),
        };
        datastore.create_index_mut_tx(&mut tx, index_def, true)?;

        datastore.rollback_mut_tx_for_test(tx);
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let seq_start = FIRST_NON_SYSTEM_ID;
        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 1, table: ST_TABLE_ID.into(), col: col(0), name: "idx_st_table_table_id_unique", },
            IndexRow { id: 2, table: ST_TABLE_ID.into(), col: col(1), name: "idx_st_table_table_name_unique", },
            IndexRow { id: 3, table: ST_COLUMN_ID.into(), col: col_list![0, 1], name: "idx_st_column_table_id_col_pos_unique", },
            IndexRow { id: 4, table: ST_SEQUENCE_ID.into(), col: col(0), name: "idx_st_sequence_sequence_id_unique", },
            IndexRow { id: 5, table: ST_INDEX_ID.into(), col: col(0), name: "idx_st_index_index_id_unique", },
            IndexRow { id: 6, table: ST_CONSTRAINT_ID.into(), col: col(0), name: "idx_st_constraint_constraint_id_unique", },
            IndexRow { id: 7, table: ST_CLIENT_ID.into(), col: col_list![0, 1], name: "idx_st_client_identity_address_unique", },
            IndexRow { id: 8, table: ST_VAR_ID.into(), col: col(0), name: "idx_st_var_name_unique", },
            IndexRow { id: 9, table: ST_SCHEDULED_ID.into(), col: col(0), name: "idx_st_scheduled_schedule_id_unique", },
            IndexRow { id: 10, table: ST_SCHEDULED_ID.into(), col: col(1), name: "idx_st_scheduled_table_id_unique", },
            IndexRow { id: seq_start,     table: FIRST_NON_SYSTEM_ID, col: col(0), name: "id_idx" },
            IndexRow { id: seq_start + 1, table: FIRST_NON_SYSTEM_ID, col: col(1), name: "name_idx" },
        ].map(Into::into));
        let row = u32_str_u32(0, "Bar", 18); // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
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
        let row = datastore.insert_mut_tx(&mut tx, table_id, row)?.1.to_product_value();
        datastore.commit_mut_tx_for_test(tx)?;

        let all_rows_col_0_eq_1 = |tx: &MutTxId| {
            datastore
                .iter_by_col_eq_mut_tx(
                    &ExecutionContext::default(),
                    tx,
                    table_id,
                    ColId(0),
                    &AlgebraicValue::U32(1),
                )
                .unwrap()
                .map(|row_ref| row_ref.to_product_value())
                .collect::<Vec<_>>()
        };

        // Update the db with the same actual value for that row, in a new tx.
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
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
        let reinserted_row = datastore
            .insert_mut_tx(&mut tx, table_id, row.clone())?
            .1
            .to_product_value();
        assert_eq!(reinserted_row, row);

        // The actual test: we should be able to iterate again, while still in the
        // second transaction, and see exactly one row.
        assert_eq!(all_rows_col_0_eq_1(&tx).len(), 1);

        datastore.commit_mut_tx_for_test(tx)?;

        Ok(())
    }

    #[test]
    /// Test that two read-only TXes can operate concurrently without deadlock or blocking,
    /// and that both observe correct results for a simple table scan.
    fn test_read_only_tx_shared_lock() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row1 = u32_str_u32(1, "Foo", 18);
        datastore.insert_mut_tx(&mut tx, table_id, row1.clone())?;
        let row2 = u32_str_u32(2, "Bar", 20);
        datastore.insert_mut_tx(&mut tx, table_id, row2.clone())?;
        datastore.commit_mut_tx_for_test(tx)?;

        // create multiple read only tx, and use them together.
        let read_tx_1 = datastore.begin_tx();
        let read_tx_2 = datastore.begin_tx();
        let rows = &[row1, row2];
        assert_eq!(&all_rows_tx(&read_tx_2, table_id), rows);
        assert_eq!(&all_rows_tx(&read_tx_1, table_id), rows);
        read_tx_2.release(&ExecutionContext::default());
        read_tx_1.release(&ExecutionContext::default());
        Ok(())
    }

    // TODO: Add the following tests
    // - Create index with unique constraint and immediately insert a row that violates the constraint before committing.
    // - Create a tx that inserts 2000 rows with an auto_inc column
    // - Create a tx that inserts 2000 rows with an auto_inc column and then rolls back
    // - Test creating sequences pre_commit, post_commit, post_rollback
}
