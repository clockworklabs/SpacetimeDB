use super::{
    committed_state::CommittedState,
    mut_tx::MutTxId,
    sequence::SequencesState,
    state_view::{Iter, IterByColRange, StateView},
    tx::TxId,
    tx_state::TxState,
};
use crate::{
    address::Address,
    db::{
        datastore::{
            system_tables::{
                Epoch, StModuleFields, StModuleRow, StTableFields, ST_MODULE_ID, ST_TABLES_ID, WASM_MODULE,
            },
            traits::{
                DataRow, IsolationLevel, MutProgrammable, MutTx, MutTxDatastore, Programmable, Tx, TxData, TxDatastore,
            },
        },
        db_metrics::{DB_METRICS, MAX_TX_CPU_TIME},
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
use spacetimedb_sats::{hash::Hash, AlgebraicValue, DataKey, ProductType, ProductValue};
use spacetimedb_table::{indexes::RowPointer, table::RowRef};
use std::ops::RangeBounds;
use std::sync::Arc;
use std::time::Instant;
use std::{borrow::Cow, time::Duration};

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
    committed_state: Arc<RwLock<CommittedState>>,
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
        committed_state.build_missing_tables()?;
        committed_state.build_indexes()?;
        committed_state.build_sequence_state(&mut sequence_state)?;
        Ok(())
    }

    /// n.b. (Tyler) We actually **do not** want to check constraints at replay
    /// time because not only is it a pain, but actually **subtly wrong** the
    /// way we have it implemented. It's wrong because the actual constraints of
    /// the database may change as different transactions are added to the
    /// schema and you would actually have to change your indexes and
    /// constraints as you replayed the log. This we are not currently doing
    /// (we're building all the non-bootstrapped indexes at the end after
    /// replaying), and thus aren't implementing constraint checking correctly
    /// as it stands.
    ///
    /// However, the above is all rendered moot anyway because we don't need to
    /// check constraints while replaying if we just assume that they were all
    /// checked prior to the transaction committing in the first place.
    ///
    /// Note also that operation/mutation ordering **does not** matter for
    /// operations inside a transaction of the message log assuming we only ever
    /// insert **OR** delete a unique row in one transaction. If we ever insert
    /// **AND** delete then order **does** matter. The issue caused by checking
    /// constraints for each operation while replaying does not imply that order
    /// matters. Ordering of operations would **only** matter if you wanted to
    /// view the state of the database as of a partially applied transaction. We
    /// never actually want to do this, because after a transaction has been
    /// committed, it is assumed that all operations happen instantaneously and
    /// atomically at the timestamp of the transaction. The only time that we
    /// actually want to view the state of a database while a transaction is
    /// partially applied is while the transaction is running **before** it
    /// commits. Thus, we only care about operation ordering while the
    /// transaction is running, but we do not care about it at all in the
    /// context of the commit log.
    ///
    /// Not caring about the order in the log, however, requires that we **do
    /// not** check index constraints during replay of transaction operatoins.
    /// We **could** check them in between transactions if we wanted to update
    /// the indexes and constraints as they changed during replay, but that is
    /// unnecessary.
    pub fn replay_transaction(&self, transaction: &Transaction, odb: &dyn ObjectDB) -> Result<()> {
        let mut committed_state = self.committed_state.write_arc();
        for write in &transaction.writes {
            let table_id = TableId(write.set_id);
            let schema = committed_state
                .schema_for_table(&ExecutionContext::default(), table_id)?
                .into_owned();
            let table_name = schema.table_name.clone();
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
                        .unwrap_or_else(|e| {
                            panic!(
                                "Error deleting row {:?} during transaction {:?} playback: {:?}",
                                &row, committed_state.next_tx_offset, e
                            );
                        });
                    // NOTE: the `rdb_num_table_rows` metric is used by the query optimizer,
                    // and therefore has performance implications and must not be disabled.
                    DB_METRICS
                        .rdb_num_table_rows
                        .with_label_values(&self.database_address, &table_id.into(), &table_name)
                        .dec();
                }
                Operation::Insert => {
                    committed_state
                        .replay_insert(table_id, &schema, &row)
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to insert row {:?} during transaction {:?} playback: {:?}",
                                &row, committed_state.next_tx_offset, e
                            );
                        });
                    // NOTE: the `rdb_num_table_rows` metric is used by the query optimizer,
                    // and therefore has performance implications and must not be disabled.
                    DB_METRICS
                        .rdb_num_table_rows
                        .with_label_values(&self.database_address, &table_id.into(), &table_name)
                        .inc();
                }
            }
        }
        committed_state.next_tx_offset += 1;
        Ok(())
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

    fn iter_by_col_eq_tx<'a, 'r>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>> {
        tx.iter_by_col_eq(ctx, &table_id, cols, value)
    }

    fn table_id_exists_tx(&self, tx: &Self::Tx, table_id: &TableId) -> bool {
        tx.table_exists(table_id).is_some()
    }

    fn table_id_from_name_tx(&self, tx: &Self::Tx, table_name: &str) -> Result<Option<TableId>> {
        tx.table_id_from_name(table_name, self.database_address)
    }

    fn table_name_from_id_tx<'a>(&'a self, tx: &'a Self::Tx, table_id: TableId) -> Result<Option<Cow<'a, str>>> {
        Ok(tx.table_exists(&table_id).map(Cow::Borrowed))
    }

    fn schema_for_table_tx<'tx>(&self, tx: &'tx Self::Tx, table_id: TableId) -> Result<Cow<'tx, TableSchema>> {
        tx.schema_for_table(&ExecutionContext::internal(self.database_address), table_id)
    }

    fn get_all_tables_tx<'tx>(&self, ctx: &ExecutionContext, tx: &'tx Self::Tx) -> Result<Vec<Cow<'tx, TableSchema>>> {
        self.iter_tx(ctx, tx, ST_TABLES_ID)?
            .map(|row_ref| {
                let table_id = row_ref.read_col(StTableFields::TableId)?;
                self.schema_for_table_tx(tx, table_id)
            })
            .collect()
    }
}

impl MutTxDatastore for Locking {
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

    fn iter_by_col_eq_mut_tx<'a, 'r>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>> {
        tx.iter_by_col_eq(ctx, &table_id, cols.into(), value)
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
    ) -> Result<ProductValue> {
        tx.insert(table_id, &mut row, self.database_address)?;
        Ok(row)
    }

    fn table_id_exists_mut_tx(&self, tx: &Self::MutTx, table_id: &TableId) -> bool {
        tx.table_exists(table_id).is_some()
    }
}

#[cfg(feature = "metrics")]
pub(crate) fn record_metrics(ctx: &ExecutionContext, tx_timer: Instant, lock_wait_time: Duration, committed: bool) {
    let workload = &ctx.workload();
    let db = &ctx.database();
    let reducer = ctx.reducer_name();
    let elapsed_time = tx_timer.elapsed();
    let cpu_time = elapsed_time - lock_wait_time;

    let elapsed_time = elapsed_time.as_secs_f64();
    let cpu_time = cpu_time.as_secs_f64();
    // Note, we record empty transactions in our metrics.
    // That is, transactions that don't write any rows to the commit log.
    DB_METRICS
        .rdb_num_txns
        .with_label_values(workload, db, reducer, &committed)
        .inc();
    DB_METRICS
        .rdb_txn_cpu_time_sec
        .with_label_values(workload, db, reducer)
        .observe(cpu_time);
    DB_METRICS
        .rdb_txn_elapsed_time_sec
        .with_label_values(workload, db, reducer)
        .observe(elapsed_time);

    let mut guard = MAX_TX_CPU_TIME.lock().unwrap();
    let max_cpu_time = *guard
        .entry((*db, *workload, reducer.to_owned()))
        .and_modify(|max| {
            if cpu_time > *max {
                *max = cpu_time;
            }
        })
        .or_insert_with(|| cpu_time);

    drop(guard);
    DB_METRICS
        .rdb_txn_cpu_time_sec_max
        .with_label_values(workload, db, reducer)
        .set(max_cpu_time);
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
        let lock_wait_time = tx.lock_wait_time;
        let timer = tx.timer;
        // TODO(cloutiertyler): We should probably track the tx.rollback() time separately.
        tx.rollback();

        // Record metrics for the transaction at the very end right before we drop
        // the MutTx and release the lock.
        #[cfg(feature = "metrics")]
        record_metrics(ctx, timer, lock_wait_time, false);
    }

    fn commit_mut_tx(&self, ctx: &ExecutionContext, tx: Self::MutTx) -> Result<Option<TxData>> {
        let lock_wait_time = tx.lock_wait_time;
        let timer = tx.timer;
        // TODO(cloutiertyler): We should probably track the tx.commit() time separately.
        let res = tx.commit(ctx);

        // Record metrics for the transaction at the very end right before we drop
        // the MutTx and release the lock.
        #[cfg(feature = "metrics")]
        record_metrics(ctx, timer, lock_wait_time, true);
        Ok(Some(res))
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

impl Programmable for Locking {
    fn program_hash(&self, tx: &TxId) -> Result<Option<spacetimedb_sats::hash::Hash>> {
        tx.iter(&ExecutionContext::internal(self.database_address), &ST_MODULE_ID)?
            .next()
            .map(|row| StModuleRow::try_from(row).map(|st| st.program_hash))
            .transpose()
    }
}

impl MutProgrammable for Locking {
    type FencingToken = u128;

    fn set_program_hash(&self, tx: &mut MutTxId, fence: Self::FencingToken, hash: Hash) -> Result<()> {
        let ctx = ExecutionContext::internal(self.database_address);
        let mut iter = tx.iter(&ctx, &ST_MODULE_ID)?;
        if let Some(row_ref) = iter.next() {
            let epoch = row_ref.read_col::<u128>(StModuleFields::Epoch)?;
            if fence <= epoch {
                return Err(anyhow!("stale fencing token: {}, storage is at epoch: {}", fence, epoch).into());
            }

            // Note the borrow checker requires that we explictly drop the iterator.
            // That is, before we delete and insert.
            // This is because datastore iterators write to the metric store when dropped.
            // Hence if we don't explicitly drop here,
            // there will be another immutable borrow of self after the two mutable borrows below.
            drop(iter);

            tx.delete(ST_MODULE_ID, row_ref.pointer())?;
            tx.insert(
                ST_MODULE_ID,
                &mut ProductValue::from(&StModuleRow {
                    program_hash: hash,
                    kind: WASM_MODULE,
                    epoch: Epoch(fence),
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
                epoch: Epoch(fence),
            }),
            self.database_address,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::datastore::system_tables::{
        system_tables, StColumnRow, StConstraintRow, StIndexRow, StSequenceRow, StTableRow, ST_COLUMNS_ID,
        ST_CONSTRAINTS_ID, ST_INDEXES_ID, ST_SEQUENCES_ID,
    };
    use crate::db::datastore::traits::{IsolationLevel, MutTx};
    use crate::db::datastore::Result;
    use crate::error::{DBError, IndexError};
    use itertools::Itertools;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_primitives::{col_list, ColId, Constraints};
    use spacetimedb_sats::db::auth::{StAccess, StTableType};
    use spacetimedb_sats::db::def::{
        ColumnDef, ColumnSchema, ConstraintSchema, IndexSchema, IndexType, SequenceSchema,
    };
    use spacetimedb_sats::{product, AlgebraicType};
    use spacetimedb_table::table::UniqueConstraintViolation;

    /// Utility to query the system tables and return their concrete table row
    pub struct SystemTableQuery<'a> {
        db: &'a MutTxId,
        ctx: &'a ExecutionContext,
    }

    fn query_st_tables<'a>(ctx: &'a ExecutionContext, tx: &'a MutTxId) -> SystemTableQuery<'a> {
        SystemTableQuery { db: tx, ctx }
    }

    impl SystemTableQuery<'_> {
        pub fn scan_st_tables(&self) -> Result<Vec<StTableRow<String>>> {
            Ok(self
                .db
                .iter(self.ctx, &ST_TABLES_ID)?
                .map(|row| StTableRow::try_from(row).unwrap())
                .sorted_by_key(|x| x.table_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_tables_by_col(
            &self,
            cols: impl Into<ColList>,
            value: &AlgebraicValue,
        ) -> Result<Vec<StTableRow<String>>> {
            Ok(self
                .db
                .iter_by_col_eq(self.ctx, &ST_TABLES_ID, cols.into(), value)?
                .map(|row| StTableRow::try_from(row).unwrap())
                .sorted_by_key(|x| x.table_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_columns(&self) -> Result<Vec<StColumnRow<String>>> {
            Ok(self
                .db
                .iter(self.ctx, &ST_COLUMNS_ID)?
                .map(|row| StColumnRow::try_from(row).unwrap())
                .sorted_by_key(|x| (x.table_id, x.col_pos))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_columns_by_col(
            &self,
            cols: impl Into<ColList>,
            value: &AlgebraicValue,
        ) -> Result<Vec<StColumnRow<String>>> {
            Ok(self
                .db
                .iter_by_col_eq(self.ctx, &ST_COLUMNS_ID, cols.into(), value)?
                .map(|row| StColumnRow::try_from(row).unwrap())
                .sorted_by_key(|x| (x.table_id, x.col_pos))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_constraints(&self) -> Result<Vec<StConstraintRow<String>>> {
            Ok(self
                .db
                .iter(self.ctx, &ST_CONSTRAINTS_ID)?
                .map(|row| StConstraintRow::try_from(row).unwrap())
                .sorted_by_key(|x| x.constraint_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_sequences(&self) -> Result<Vec<StSequenceRow<String>>> {
            Ok(self
                .db
                .iter(self.ctx, &ST_SEQUENCES_ID)?
                .map(|row| StSequenceRow::try_from(row).unwrap())
                .sorted_by_key(|x| (x.table_id, x.sequence_id))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_indexes(&self) -> Result<Vec<StIndexRow<String>>> {
            Ok(self
                .db
                .iter(self.ctx, &ST_INDEXES_ID)?
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

    fn col(col: u32) -> ColList {
        col.into()
    }

    fn map_array<A, B: From<A>, const N: usize>(a: [A; N]) -> Vec<B> {
        a.map(Into::into).into()
    }

    struct IndexRow<'a> {
        id: u32,
        table: u32,
        col: ColList,
        name: &'a str,
        unique: bool,
    }
    impl From<IndexRow<'_>> for StIndexRow<String> {
        fn from(value: IndexRow<'_>) -> Self {
            Self {
                index_id: value.id.into(),
                table_id: value.table.into(),
                columns: value.col,
                index_name: value.name.into(),
                is_unique: value.unique,
                index_type: IndexType::BTree,
            }
        }
    }

    struct TableRow<'a> {
        id: u32,
        name: &'a str,
        ty: StTableType,
        access: StAccess,
    }
    impl From<TableRow<'_>> for StTableRow<String> {
        fn from(value: TableRow<'_>) -> Self {
            Self {
                table_id: value.id.into(),
                table_name: value.name.into(),
                table_type: value.ty,
                table_access: value.access,
            }
        }
    }

    struct ColRow<'a> {
        table: u32,
        pos: u32,
        name: &'a str,
        ty: AlgebraicType,
    }
    impl From<ColRow<'_>> for StColumnRow<String> {
        fn from(value: ColRow<'_>) -> Self {
            Self {
                table_id: value.table.into(),
                col_pos: value.pos.into(),
                col_name: value.name.into(),
                col_type: value.ty,
            }
        }
    }
    impl From<ColRow<'_>> for ColumnSchema {
        fn from(value: ColRow<'_>) -> Self {
            Self {
                table_id: value.table.into(),
                col_pos: value.pos.into(),
                col_name: value.name.to_string(),
                col_type: value.ty,
            }
        }
    }
    impl From<ColRow<'_>> for ColumnDef {
        fn from(value: ColRow<'_>) -> Self {
            Self {
                col_name: value.name.to_string(),
                col_type: value.ty,
            }
        }
    }

    struct SequenceRow<'a> {
        id: u32,
        name: &'a str,
        table: u32,
        col_pos: u32,
        start: i128,
    }
    impl From<SequenceRow<'_>> for StSequenceRow<String> {
        fn from(value: SequenceRow<'_>) -> Self {
            Self {
                sequence_id: value.id.into(),
                sequence_name: value.name.to_string(),
                table_id: value.table.into(),
                col_pos: value.col_pos.into(),
                increment: 1,
                start: value.start,
                min_value: 1,
                max_value: 170141183460469231731687303715884105727,
                allocated: 4096,
            }
        }
    }

    impl From<SequenceRow<'_>> for SequenceSchema {
        fn from(value: SequenceRow<'_>) -> Self {
            Self {
                sequence_id: value.id.into(),
                sequence_name: value.name.to_string(),
                table_id: value.table.into(),
                col_pos: value.col_pos.into(),
                increment: 1,
                start: value.start,
                min_value: 1,
                max_value: 170141183460469231731687303715884105727,
                allocated: 4096,
            }
        }
    }

    struct IdxSchema<'a> {
        id: u32,
        table: u32,
        col: u32,
        name: &'a str,
        unique: bool,
    }
    impl From<IdxSchema<'_>> for IndexSchema {
        fn from(value: IdxSchema<'_>) -> Self {
            Self {
                index_id: value.id.into(),
                table_id: value.table.into(),
                columns: ColId(value.col).into(),
                index_name: value.name.to_string(),
                is_unique: value.unique,
                index_type: IndexType::BTree,
            }
        }
    }

    struct ConstraintRow<'a> {
        constraint_id: u32,
        constraint_name: &'a str,
        constraints: Constraints,
        table_id: u32,
        columns: ColList,
    }
    impl From<ConstraintRow<'_>> for StConstraintRow<String> {
        fn from(value: ConstraintRow<'_>) -> Self {
            Self {
                constraint_id: value.constraint_id.into(),
                constraint_name: value.constraint_name.into(),
                constraints: value.constraints,
                table_id: value.table_id.into(),
                columns: value.columns,
            }
        }
    }

    impl From<ConstraintRow<'_>> for ConstraintSchema {
        fn from(value: ConstraintRow<'_>) -> Self {
            Self {
                constraint_id: value.constraint_id.into(),
                constraint_name: value.constraint_name.into(),
                constraints: value.constraints,
                table_id: value.table_id.into(),
                columns: value.columns,
            }
        }
    }

    #[rustfmt::skip]
    fn basic_table_schema_cols() -> [ColRow<'static>; 3] {
        [
            ColRow { table: 6, pos: 0, name: "id", ty: AlgebraicType::U32 },
            ColRow { table: 6, pos: 1, name: "name", ty: AlgebraicType::String },
            ColRow { table: 6, pos: 2, name: "age", ty: AlgebraicType::U32 },
        ]
    }

    fn basic_table_schema() -> TableDef {
        TableDef::new("Foo".into(), map_array(basic_table_schema_cols()))
            .with_indexes(vec![
                IndexDef {
                    columns: ColList::new(0.into()),
                    index_name: "id_idx".into(),
                    is_unique: true,
                    index_type: IndexType::BTree,
                },
                IndexDef {
                    columns: ColList::new(1.into()),
                    index_name: "name_idx".into(),
                    is_unique: true,
                    index_type: IndexType::BTree,
                },
            ])
            .with_column_sequence(ColId(0))
    }

    #[rustfmt::skip]
    fn basic_table_schema_created(table_id: TableId) -> TableSchema {
        TableSchema::new(
            table_id,
            "Foo".into(),
            map_array(basic_table_schema_cols()),
             map_array([
                IdxSchema { id: 6, table: 6, col: 0, name: "id_idx", unique: true },
                IdxSchema { id: 7, table: 6, col: 1, name: "name_idx", unique: true },
            ]),
            map_array([
                ConstraintRow { constraint_id: 6, table_id: 6, columns: col(0), constraints: Constraints::unique(), constraint_name: "ct_Foo_id_idx_unique" },
                ConstraintRow { constraint_id: 7, table_id: 6, columns: col(1), constraints: Constraints::unique(), constraint_name: "ct_Foo_name_idx_unique" }
            ]),
             map_array([
                SequenceRow { id: 4, table: 6, col_pos: 0, name: "seq_Foo_id", start: 1 }
            ]),
            StTableType::User,
            StAccess::Public,
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

    //TODO(shub), begin_tx is not yet implemented for Tx, creating this utility for tests.
    fn begin_tx(db: &Locking) -> TxId {
        let timer = Instant::now();

        let committed_state_shared_lock = db.committed_state.read_arc();
        let lock_wait_time = timer.elapsed();
        TxId {
            committed_state_shared_lock,
            lock_wait_time,
            timer,
        }
    }

    fn all_rows_tx(tx: &TxId, table_id: TableId) -> Vec<ProductValue> {
        tx.iter(&ExecutionContext::default(), &table_id)
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
            TableRow { id: 0, name: "st_table", ty: StTableType::System, access: StAccess::Public },
            TableRow { id: 1, name: "st_columns", ty: StTableType::System, access: StAccess::Public },
            TableRow { id: 2, name: "st_sequence", ty: StTableType::System, access: StAccess::Public },
            TableRow { id: 3, name: "st_indexes", ty: StTableType::System, access: StAccess::Public },
            TableRow { id: 4, name: "st_constraints", ty: StTableType::System, access: StAccess::Public },
            TableRow { id: 5, name: "st_module", ty: StTableType::System, access: StAccess::Public },
        ]));
        #[rustfmt::skip]
        assert_eq!(query.scan_st_columns()?, map_array([
            ColRow { table: 0, pos: 0, name: "table_id", ty: AlgebraicType::U32 },
            ColRow { table: 0, pos: 1, name: "table_name", ty: AlgebraicType::String },
            ColRow { table: 0, pos: 2, name: "table_type", ty: AlgebraicType::String },
            ColRow { table: 0, pos: 3, name: "table_access", ty: AlgebraicType::String },

            ColRow { table: 1, pos: 0, name: "table_id", ty: AlgebraicType::U32 },
            ColRow { table: 1, pos: 1, name: "col_pos", ty: AlgebraicType::U32 },
            ColRow { table: 1, pos: 2, name: "col_name", ty: AlgebraicType::String },
            ColRow { table: 1, pos: 3, name: "col_type", ty: AlgebraicType::bytes() },

            ColRow { table: 2, pos: 0, name: "sequence_id", ty: AlgebraicType::U32 },
            ColRow { table: 2, pos: 1, name: "sequence_name", ty: AlgebraicType::String },
            ColRow { table: 2, pos: 2, name: "table_id", ty: AlgebraicType::U32 },
            ColRow { table: 2, pos: 3, name: "col_pos", ty: AlgebraicType::U32 },
            ColRow { table: 2, pos: 4, name: "increment", ty: AlgebraicType::I128 },
            ColRow { table: 2, pos: 5, name: "start", ty: AlgebraicType::I128 },
            ColRow { table: 2, pos: 6, name: "min_value", ty: AlgebraicType::I128 },
            ColRow { table: 2, pos: 7, name: "max_value", ty: AlgebraicType::I128 },
            ColRow { table: 2, pos: 8, name: "allocated", ty: AlgebraicType::I128 },

            ColRow { table: 3, pos: 0, name: "index_id", ty: AlgebraicType::U32 },
            ColRow { table: 3, pos: 1, name: "table_id", ty: AlgebraicType::U32 },
            ColRow { table: 3, pos: 2, name: "index_name", ty: AlgebraicType::String },
            ColRow { table: 3, pos: 3, name: "columns", ty: AlgebraicType::array(AlgebraicType::U32) },
            ColRow { table: 3, pos: 4, name: "is_unique", ty: AlgebraicType::Bool },
            ColRow { table: 3, pos: 5, name: "index_type", ty: AlgebraicType::U8 },

            ColRow { table: 4, pos: 0, name: "constraint_id", ty: AlgebraicType::U32 },
            ColRow { table: 4, pos: 1, name: "constraint_name", ty: AlgebraicType::String },
            ColRow { table: 4, pos: 2, name: "constraints", ty: AlgebraicType::U8 },
            ColRow { table: 4, pos: 3, name: "table_id", ty: AlgebraicType::U32 },
            ColRow { table: 4, pos: 4, name: "columns", ty: AlgebraicType::array(AlgebraicType::U32) },

            ColRow { table: 5, pos: 0, name: "program_hash", ty: AlgebraicType::array(AlgebraicType::U8) },
            ColRow { table: 5, pos: 1, name: "kind", ty: AlgebraicType::U8 },
            ColRow { table: 5, pos: 2, name: "epoch", ty: AlgebraicType::U128 },
        ]));
        #[rustfmt::skip]
        assert_eq!(query.scan_st_indexes()?, map_array([
            IndexRow { id: 0, table: 0, col: col(0), name: "idx_st_table_table_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 1, table: 0, col: col(1), name: "idx_st_table_table_name_unique", unique: true },
            IndexRow { id: 2, table: 1, col: col_list![0, 1], name: "idx_st_columns_table_id_col_pos_unique", unique: true },
            IndexRow { id: 3, table: 2, col: col(0), name: "idx_st_sequence_sequence_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 4, table: 3, col: col(0), name: "idx_st_indexes_index_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 5, table: 4, col: col(0), name: "idx_st_constraints_constraint_id_primary_key_auto_unique", unique: true },
        ]));
        #[rustfmt::skip]
        assert_eq!(query.scan_st_sequences()?, map_array([
            SequenceRow { id: 0, table: 0, col_pos: 0, name: "seq_st_table_table_id_primary_key_auto",  start: 6 },
            SequenceRow { id: 3, table: 2, col_pos: 0, name: "seq_st_sequence_sequence_id_primary_key_auto", start: 4 },
            SequenceRow { id: 1, table: 3, col_pos: 0, name: "seq_st_indexes_index_id_primary_key_auto",  start: 6 },
            SequenceRow { id: 2, table: 4, col_pos: 0, name: "seq_st_constraints_constraint_id_primary_key_auto", start: 6 },
        ]));
        #[rustfmt::skip]
        assert_eq!(query.scan_st_constraints()?, map_array([
            ConstraintRow { constraint_id: 0, table_id: 0, columns: col(0), constraints: Constraints::primary_key_auto(), constraint_name: "ct_st_table_table_id_primary_key_auto" },
            ConstraintRow { constraint_id: 1, table_id: 0, columns: col(1), constraints: Constraints::unique(), constraint_name: "ct_st_table_table_name_unique" },
            ConstraintRow { constraint_id: 2, table_id: 1, columns: col_list![0, 1], constraints: Constraints::unique(), constraint_name: "ct_st_columns_table_id_col_pos_unique" },
            ConstraintRow { constraint_id: 3, table_id: 2, columns: col(0), constraints: Constraints::primary_key_auto(), constraint_name: "ct_st_sequence_sequence_id_primary_key_auto" },
            ConstraintRow { constraint_id: 4, table_id: 3, columns: col(0), constraints: Constraints::primary_key_auto(), constraint_name: "ct_st_indexes_index_id_primary_key_auto" },
            ConstraintRow { constraint_id: 5, table_id: 4, columns: col(0), constraints: Constraints::primary_key_auto(), constraint_name: "ct_st_constraints_constraint_id_primary_key_auto" },
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
            TableRow { id: 6, name: "Foo", ty: StTableType::User, access: StAccess::Public }
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
            TableRow { id: 6, name: "Foo", ty: StTableType::User, access: StAccess::Public }
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

    #[test]
    fn test_schema_for_table_pre_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        let schema = &*datastore.schema_for_table_mut_tx(&tx, table_id)?;
        #[rustfmt::skip]
        assert_eq!(schema, &basic_table_schema_created(table_id));
        Ok(())
    }

    #[test]
    fn test_schema_for_table_post_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx_for_test(tx)?;
        let tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
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
        let schema = datastore.schema_for_table_mut_tx(&tx, table_id)?.into_owned();

        for index in &*schema.indexes {
            datastore.drop_index_mut_tx(&mut tx, index.index_id)?;
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

        datastore.create_index_mut_tx(
            &mut tx,
            schema.table_id,
            IndexDef::btree("id_idx".into(), ColId(0), true),
        )?;

        let expected_indexes = [IdxSchema {
            id: 8,
            table: 6,
            col: 0,
            name: "id_idx",
            unique: true,
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
        let index_def = IndexDef::btree("age_idx".into(), ColId(2), true);
        datastore.create_index_mut_tx(&mut tx, table_id, index_def)?;
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 0, table: 0, col: col(0), name: "idx_st_table_table_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 1, table: 0, col: col(1), name: "idx_st_table_table_name_unique", unique: true },
            IndexRow { id: 2, table: 1, col: col_list![0, 1], name: "idx_st_columns_table_id_col_pos_unique", unique: true },
            IndexRow { id: 3, table: 2, col: col(0), name: "idx_st_sequence_sequence_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 4, table: 3, col: col(0), name: "idx_st_indexes_index_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 5, table: 4, col: col(0), name: "idx_st_constraints_constraint_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 6, table: 6, col: col(0), name: "id_idx", unique: true },
            IndexRow { id: 7, table: 6, col: col(1), name: "name_idx", unique: true },
            IndexRow { id: 8, table: 6, col: col(2), name: "age_idx", unique: true },
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
    fn test_create_index_post_commit() -> ResultTest<()> {
        let (datastore, mut tx, table_id) = setup_table()?;
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let index_def = IndexDef::btree("age_idx".into(), ColId(2), true);
        datastore.create_index_mut_tx(&mut tx, table_id, index_def)?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 0, table: 0, col: col(0), name: "idx_st_table_table_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 1, table: 0, col: col(1), name: "idx_st_table_table_name_unique", unique: true },
            IndexRow { id: 2, table: 1, col: col_list![0, 1], name: "idx_st_columns_table_id_col_pos_unique", unique: true },
            IndexRow { id: 3, table: 2, col: col(0), name: "idx_st_sequence_sequence_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 4, table: 3, col: col(0), name: "idx_st_indexes_index_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 5, table: 4, col: col(0), name: "idx_st_constraints_constraint_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 6, table: 6, col: col(0), name: "id_idx", unique: true },
            IndexRow { id: 7, table: 6, col: col(1), name: "name_idx", unique: true },
            IndexRow { id: 8, table: 6, col: col(2), name: "age_idx", unique: true },
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
        let index_def = IndexDef::btree("age_idx".into(), ColId(2), true);
        datastore.create_index_mut_tx(&mut tx, table_id, index_def)?;
        datastore.rollback_mut_tx_for_test(tx);
        let mut tx = datastore.begin_mut_tx(IsolationLevel::Serializable);
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 0, table: 0, col: col(0), name: "idx_st_table_table_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 1, table: 0, col: col(1), name: "idx_st_table_table_name_unique", unique: true },
            IndexRow { id: 2, table: 1, col: col_list![0, 1], name: "idx_st_columns_table_id_col_pos_unique", unique: true },
            IndexRow { id: 3, table: 2, col: col(0), name: "idx_st_sequence_sequence_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 4, table: 3, col: col(0), name: "idx_st_indexes_index_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 5, table: 4, col: col(0), name: "idx_st_constraints_constraint_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 6, table: 6, col: col(0), name: "id_idx", unique: true },
            IndexRow { id: 7, table: 6, col: col(1), name: "name_idx", unique: true },
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
                                             // Because of autoinc columns, we will get a slightly different
                                             // value than the one we inserted.
        let row = datastore.insert_mut_tx(&mut tx, table_id, row)?;
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
        // Iterate over all rows with the value 1 (from the autoinc) in column 0.
        let rows = all_rows_col_0_eq_1(&tx);
        assert_eq!(rows.len(), 1);
        assert_eq!(row, rows[0]);
        // Delete the row.
        let count_deleted = datastore.delete_by_rel_mut_tx(&mut tx, table_id, rows);
        assert_eq!(count_deleted, 1);

        // We shouldn't see the row when iterating now that it's deleted.
        assert_eq!(all_rows_col_0_eq_1(&tx).len(), 0);

        // Reinsert the row.
        let reinserted_row = datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
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
        let read_tx_1 = begin_tx(&datastore);
        let read_tx_2 = begin_tx(&datastore);
        let rows = &[row1, row2];
        assert_eq!(&all_rows_tx(&read_tx_2, table_id), rows);
        assert_eq!(&all_rows_tx(&read_tx_1, table_id), rows);
        read_tx_2.release(&ExecutionContext::default());
        read_tx_1.release(&ExecutionContext::default());
        Ok(())
    }

    // TODO: Add the following tests
    // - Create index with unique constraint and immediately insert a row that violates the constraint before committing.
    // - Create a tx that inserts 2000 rows with an autoinc column
    // - Create a tx that inserts 2000 rows with an autoinc column and then rolls back
    // - Test creating sequences pre_commit, post_commit, post_rollback
}
