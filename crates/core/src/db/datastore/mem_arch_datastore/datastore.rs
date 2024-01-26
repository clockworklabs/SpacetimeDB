use super::{
    committed_state::CommittedState,
    indexes::RowPointer,
    mut_tx::MutTxId,
    sequence::SequencesState,
    state_view::{Iter, IterByColRange, StateView},
    table::RowRef,
    tx::TxId,
    tx_state::TxState,
};
use crate::{
    address::Address,
    db::{
        datastore::{
            system_tables::{self, StModuleRow, StTableRow, ST_MODULE_ID, ST_TABLES_ID, WASM_MODULE},
            traits::{DataRow, MutProgrammable, MutTx, MutTxDatastore, Programmable, Tx, TxData, TxDatastore},
        },
        messages::{transaction::Transaction, write::Operation},
        ostorage::ObjectDB,
    },
    error::DBError,
    execution_context::ExecutionContext,
};
use anyhow::anyhow;
use parking_lot::{Mutex, RwLock};
use spacetimedb_primitives::{ColList, ConstraintId, IndexId, SequenceId, TableId};
use spacetimedb_sats::db::def::{IndexDef, SequenceDef, TableDef, TableSchema};
use spacetimedb_sats::{AlgebraicValue, DataKey, ProductType, ProductValue};
use std::borrow::Cow;
use std::ops::RangeBounds;
use std::sync::Arc;
use std::time::Instant;

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
pub struct MemArchPrototype {
    /// The state of the database up to the point of the last committed transaction.
    committed_state: Arc<RwLock<CommittedState>>,
    /// The state of sequence generation in this database.
    sequence_state: Arc<Mutex<SequencesState>>,
    /// The address of this database.
    database_address: Address,
}

impl MemArchPrototype {
    pub fn new(database_address: Address) -> Self {
        Self {
            committed_state: Arc::new(RwLock::new(CommittedState::new())),
            sequence_state: Arc::new(Mutex::new(SequencesState::default())),
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
        committed_state.build_missing_tables()?;
        committed_state.build_indexes()?;
        committed_state.build_sequence_state(&mut sequence_state)?;
        Ok(())
    }

    pub fn replay_transaction(&self, transaction: &Transaction, odb: &dyn ObjectDB) -> Result<()> {
        let mut committed_state = self.committed_state.write_arc();
        for write in &transaction.writes {
            let table_id = TableId(write.set_id);
            let schema = committed_state
                .schema_for_table(&ExecutionContext::default(), table_id)?
                .into_owned();
            let row_type = schema.get_row_type();

            let decode_row = |mut data: &[u8], source: &str| {
                ProductValue::decode(row_type, &mut data).unwrap_or_else(|e| {
                    panic!(
                        "Couldn't decode product value from {}: `{}`. Expected row type: {:?}",
                        source, e, row_type
                    )
                })
            };

            let data_key_to_av = |data_key| match data_key {
                DataKey::Data(data) => decode_row(&data, "message log"),

                DataKey::Hash(hash) => {
                    let data = odb.get(hash).unwrap_or_else(|| {
                        panic!("Object {hash} referenced from transaction not present in object DB");
                    });
                    decode_row(&data, "object DB")
                }
            };

            let row = data_key_to_av(write.data_key);

            match write.operation {
                Operation::Delete => {
                    committed_state
                        .replay_delete_by_rel(table_id, &row)
                        .expect("Error deleting row while replaying transaction")
                    // METRICS
                    //     .rdb_num_table_rows
                    //     .with_label_values(&self.database_address, &table_id.into())
                    //     .dec();
                }
                Operation::Insert => {
                    committed_state.with_table_and_blob_store_or_create_ref_schema(
                        table_id,
                        &schema,
                        |table, blob_store| {
                            table.insert(blob_store, &row).unwrap_or_else(|e| {
                                panic!("Failed to insert during transaction playback: {:?}", e);
                            });
                        },
                    );

                    // METRICS
                    //     .rdb_num_table_rows
                    //     .with_label_values(&self.database_address, &table_id.into())
                    //     .inc();
                }
            }
        }
        Ok(())
    }
}

impl DataRow for MemArchPrototype {
    type RowId = RowPointer;
    type DataRef<'a> = RowRef<'a>;

    fn view_product_value<'a>(&self, data_ref: Self::DataRef<'a>) -> Cow<'a, ProductValue> {
        Cow::Owned(data_ref.read_row())
    }
}

impl Tx for MemArchPrototype {
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
        tx.release(ctx)
    }
}

impl TxDatastore for MemArchPrototype {
    type Iter<'a> = Iter<'a> where Self: 'a;
    type IterByColEq<'a> = IterByColRange<'a, AlgebraicValue> where Self: 'a;
    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>> = IterByColRange<'a, R> where Self: 'a;

    fn iter_tx<'a>(&'a self, ctx: &'a ExecutionContext, tx: &'a Self::Tx, table_id: TableId) -> Result<Self::Iter<'a>> {
        tx.iter(ctx, &table_id)
    }

    fn iter_by_col_range_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<Self::IterByColRange<'a, R>> {
        tx.iter_by_col_range(ctx, &table_id, cols.into(), range)
    }

    fn iter_by_col_eq_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a>> {
        tx.iter_by_col_eq(ctx, &table_id, cols.into(), value)
    }

    fn table_id_exists_tx(&self, tx: &Self::Tx, table_id: &TableId) -> bool {
        tx.table_exists(table_id).is_some()
    }

    fn table_id_from_name_tx(&self, tx: &Self::Tx, table_name: &str) -> Result<Option<TableId>> {
        tx.table_id_from_name(table_name, self.database_address)
    }

    fn schema_for_table_tx<'tx>(&self, tx: &'tx Self::Tx, table_id: TableId) -> Result<Cow<'tx, TableSchema>> {
        tx.schema_for_table(&ExecutionContext::internal(self.database_address), table_id)
    }

    fn get_all_tables_tx<'tx>(&self, ctx: &ExecutionContext, tx: &'tx Self::Tx) -> Result<Vec<Cow<'tx, TableSchema>>> {
        self.iter_tx(ctx, tx, ST_TABLES_ID)?
            .map(|row_ref| {
                let data = row_ref.read_row();
                let row = StTableRow::try_from(&data)?;
                self.schema_for_table_tx(tx, row.table_id)
            })
            .collect()
    }
}

impl MutTxDatastore for MemArchPrototype {
    fn create_table_mut_tx(&self, tx: &mut Self::MutTx, schema: TableDef) -> Result<TableId> {
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
    fn row_type_for_table_mut_tx<'tx>(&self, tx: &'tx Self::MutTx, table_id: TableId) -> Result<Cow<'tx, ProductType>> {
        tx.row_type_for_table(table_id, self.database_address)
    }

    /// IMPORTANT! This function is relatively expensive, and much more
    /// expensive than `row_type_for_table_mut_tx`.  Prefer
    /// `row_type_for_table_mut_tx` if you only need to access the `ProductType`
    /// of the table.
    fn schema_for_table_mut_tx<'tx>(&self, tx: &'tx Self::MutTx, table_id: TableId) -> Result<Cow<'tx, TableSchema>> {
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
        tx.table_name_from_id(ctx, table_id).map(|opt| opt.map(Cow::Owned))
    }

    fn create_index_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId, index: IndexDef) -> Result<IndexId> {
        tx.create_index(table_id, index, self.database_address)
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

    fn create_sequence_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId, seq: SequenceDef) -> Result<SequenceId> {
        tx.create_sequence(table_id, seq, self.database_address)
    }

    fn drop_sequence_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> Result<()> {
        tx.drop_sequence(seq_id, self.database_address)
    }

    fn sequence_id_from_name_mut_tx(&self, tx: &Self::MutTx, sequence_name: &str) -> Result<Option<SequenceId>> {
        tx.sequence_id_from_name(sequence_name, self.database_address)
    }

    fn drop_constraint_mut_tx(&self, tx: &mut Self::MutTx, constraint_id: ConstraintId) -> Result<()> {
        tx.drop_constraint(constraint_id, self.database_address)
    }

    fn constraint_id_from_name(&self, tx: &Self::MutTx, constraint_name: &str) -> Result<Option<ConstraintId>> {
        tx.constraint_id_from_name(constraint_name, self.database_address)
    }

    fn iter_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
    ) -> Result<Self::Iter<'a>> {
        tx.iter(ctx, &table_id)
    }

    fn iter_by_col_range_mut_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        range: R,
    ) -> Result<Self::IterByColRange<'a, R>> {
        tx.iter_by_col_range(ctx, &table_id, cols.into(), range)
    }

    fn iter_by_col_eq_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a>> {
        tx.iter_by_col_eq(ctx, &table_id, cols.into(), value)
    }

    fn get_mut_tx<'a>(
        &self,
        tx: &'a Self::MutTx,
        table_id: TableId,
        row_ptr: &'a Self::RowId,
    ) -> Result<Option<Self::DataRef<'a>>> {
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
        relations: impl IntoIterator<Item = ProductValue>,
    ) -> u32 {
        let mut num_deleted = 0;
        for rel in relations {
            match tx.delete_by_rel(table_id, &rel) {
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
    ) -> Result<ProductValue> {
        tx.insert(table_id, &mut row, self.database_address)?;
        Ok(row)
    }

    fn table_id_exists_mut_tx(&self, tx: &Self::MutTx, table_id: &TableId) -> bool {
        tx.table_exists(table_id).is_some()
    }
}

impl MutTx for MemArchPrototype {
    type MutTx = MutTxId;

    fn begin_mut_tx(&self) -> Self::MutTx {
        let timer = Instant::now();

        let committed_state_write_lock = self.committed_state.write_arc();
        let sequence_state_lock = self.sequence_state.lock_arc();
        let lock_wait_time = timer.elapsed();
        MutTxId {
            committed_state_write_lock,
            sequence_state_lock,
            tx_state: TxState::new(),
            lock_wait_time,
            timer,
        }
    }

    fn rollback_mut_tx(&self, _ctx: &ExecutionContext, tx: Self::MutTx) {
        // let workload = &ctx.workload();
        // let db = &ctx.database();
        // let reducer = ctx.reducer_name().unwrap_or_default();
        // let elapsed_time = tx.timer.elapsed();
        // let cpu_time = elapsed_time - tx.lock_wait_time;
        // DB_METRICS
        //     .rdb_num_txns
        //     .with_label_values(workload, db, reducer, &false)
        //     .inc();
        // DB_METRICS
        //     .rdb_txn_cpu_time_sec
        //     .with_label_values(workload, db, reducer)
        //     .observe(cpu_time.as_secs_f64());
        // DB_METRICS
        //     .rdb_txn_elapsed_time_sec
        //     .with_label_values(workload, db, reducer)
        //     .observe(elapsed_time.as_secs_f64());
        tx.rollback();
    }

    fn commit_mut_tx(&self, _ctx: &ExecutionContext, tx: Self::MutTx) -> Result<Option<TxData>> {
        // let workload = &ctx.workload();
        // let db = &ctx.database();
        // let reducer = ctx.reducer_name().unwrap_or_default();
        // let elapsed_time = tx.timer.elapsed();
        // let cpu_time = elapsed_time - tx.lock_wait_time;

        // let elapsed_time = elapsed_time.as_secs_f64();
        // let cpu_time = cpu_time.as_secs_f64();
        // Note, we record empty transactions in our metrics.
        // That is, transactions that don't write any rows to the commit log.
        // DB_METRICS
        //     .rdb_num_txns
        //     .with_label_values(workload, db, reducer, &true)
        //     .inc();
        // DB_METRICS
        //     .rdb_txn_cpu_time_sec
        //     .with_label_values(workload, db, reducer)
        //     .observe(cpu_time);
        // DB_METRICS
        //     .rdb_txn_elapsed_time_sec
        //     .with_label_values(workload, db, reducer)
        //     .observe(elapsed_time);

        // fn hash(a: &WorkloadType, b: &Address, c: &str) -> u64 {
        //     use std::hash::Hash;
        //     let mut hasher = DefaultHasher::new();
        //     a.hash(&mut hasher);
        //     b.hash(&mut hasher);
        //     c.hash(&mut hasher);
        //     hasher.finish()
        // }

        // let mut guard = MAX_TX_CPU_TIME.lock().unwrap();
        // let max_cpu_time = *guard
        //     .entry(hash(workload, db, reducer))
        //     .and_modify(|max| {
        //         if cpu_time > *max {
        //             *max = cpu_time;
        //         }
        //     })
        //     .or_insert_with(|| cpu_time);

        // drop(guard);
        // DB_METRICS
        //     .rdb_txn_cpu_time_sec_max
        //     .with_label_values(workload, db, reducer)
        //     .set(max_cpu_time);

        Ok(Some(tx.commit()))
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

impl Programmable for MemArchPrototype {
    fn program_hash(&self, tx: &TxId) -> Result<Option<spacetimedb_sats::hash::Hash>> {
        match tx
            .iter(&ExecutionContext::internal(self.database_address), &ST_MODULE_ID)?
            .next()
        {
            None => Ok(None),
            Some(data) => {
                let row = data.read_row();
                let row = StModuleRow::try_from(&row)?;
                Ok(Some(row.program_hash))
            }
        }
    }
}

impl MutProgrammable for MemArchPrototype {
    type FencingToken = u128;

    fn set_program_hash(
        &self,
        tx: &mut MutTxId,
        fence: Self::FencingToken,
        hash: spacetimedb_sats::hash::Hash,
    ) -> Result<()> {
        let ctx = ExecutionContext::internal(self.database_address);
        let mut iter = tx.iter(&ctx, &ST_MODULE_ID)?;
        if let Some(row_ref) = iter.next() {
            let row_pv = row_ref.read_row();
            let row = StModuleRow::try_from(&row_pv)?;
            if fence <= row.epoch.0 {
                return Err(anyhow!("stale fencing token: {}, storage is at epoch: {}", fence, row.epoch).into());
            }

            // Note the borrow checker requires that we explictly drop the iterator.
            // That is, before we delete and insert.
            // This is because datastore iterators write to the metric store when dropped.
            // Hence if we don't explicitly drop here,
            // there will be another immutable borrow of self after the two mutable borrows below.
            drop(iter);

            tx.delete_by_rel(ST_MODULE_ID, &row_pv)?;
            tx.insert(
                ST_MODULE_ID,
                &mut ProductValue::from(&StModuleRow {
                    program_hash: hash,
                    kind: WASM_MODULE,
                    epoch: system_tables::Epoch(fence),
                }),
                self.database_address,
            )?;
            return Ok(());
        }

        // Note the borrow checker requires that we explictly drop the iterator before we insert.
        // This is because datastore iterators write to the metric store when dropped.
        // Hence if we don't explicitly drop here,
        // there will be another immutable borrow of self after the mutable borrow of the insert.
        drop(iter);

        tx.insert(
            ST_MODULE_ID,
            &mut ProductValue::from(&StModuleRow {
                program_hash: hash,
                kind: WASM_MODULE,
                epoch: system_tables::Epoch(fence),
            }),
            self.database_address,
        )?;
        Ok(())
    }
}
