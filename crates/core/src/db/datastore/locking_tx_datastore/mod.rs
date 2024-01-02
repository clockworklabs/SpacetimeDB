mod btree_index;
mod sequence;
mod table;

use itertools::Itertools;
use nonempty::NonEmpty;
use parking_lot::{
    lock_api::{ArcMutexGuard, ArcRwLockReadGuard},
    Mutex, RawMutex,
};

use self::{
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    sequence::Sequence,
    table::Table,
};
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::ops::Deref;
use std::time::{Duration, Instant};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::RangeBounds,
    sync::Arc,
    vec,
};

use super::{
    system_tables::{
        system_tables, StColumnRow, StIndexRow, StSequenceRow, StTableRow, SystemTable, ST_COLUMNS_ID, ST_INDEXES_ID,
        ST_SEQUENCES_ID, ST_TABLES_ID,
    },
    traits::{self, DataRow, MutTx, MutTxDatastore, TxData, TxDatastore},
};
use crate::db::datastore::system_tables::{
    st_constraints_schema, st_module_schema, table_name_is_system, StColumnFields, StConstraintFields, StConstraintRow,
    StIndexFields, StModuleRow, StSequenceFields, StTableFields, ST_CONSTRAINTS_ID, ST_MODULE_ID, WASM_MODULE,
};
use crate::db::db_metrics::{DB_METRICS, MAX_TX_CPU_TIME};
use crate::execution_context::{ExecutionContext, WorkloadType};
use crate::{db::datastore::system_tables, error::IndexError};
use crate::{
    db::datastore::traits::{TxOp, TxRecord},
    db::{
        datastore::system_tables::{st_columns_schema, st_indexes_schema, st_sequences_schema, st_table_schema},
        messages::{transaction::Transaction, write::Operation},
        ostorage::ObjectDB,
    },
    error::{DBError, TableError},
};
use anyhow::anyhow;
use parking_lot::{lock_api::ArcRwLockWriteGuard, RawRwLock, RwLock};
use spacetimedb_lib::{metrics::METRICS, Address};
use spacetimedb_primitives::*;
use spacetimedb_sats::data_key::{DataKey, ToDataKey};
use spacetimedb_sats::db::def::*;
use spacetimedb_sats::db::error::SchemaErrors;
use spacetimedb_sats::hash::Hash;
use spacetimedb_sats::relation::RelValue;
use spacetimedb_sats::{
    db::auth::{StAccess, StTableType},
    AlgebraicType, AlgebraicValue, ProductType, ProductValue,
};
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SequenceError {
    #[error("Sequence with name `{0}` already exists.")]
    Exist(String),
    #[error("Sequence `{0}`: The increment is 0, and this means the sequence can't advance.")]
    IncrementIsZero(String),
    #[error("Sequence `{0}`: The min_value {1} must < max_value {2}.")]
    MinMax(String, i128, i128),
    #[error("Sequence `{0}`: The start value {1} must be >= min_value {2}.")]
    MinStart(String, i128, i128),
    #[error("Sequence `{0}`: The start value {1} must be <= min_value {2}.")]
    MaxStart(String, i128, i128),
    #[error("Sequence `{0}` failed to decode value from Sled (not a u128).")]
    SequenceValue(String),
    #[error("Sequence ID `{0}` not found.")]
    NotFound(SequenceId),
    #[error("Sequence applied to a non-integer field. Column `{col}` is of type {{found.to_sats()}}.")]
    NotInteger { col: String, found: AlgebraicType },
    #[error("Sequence ID `{0}` still had no values left after allocation.")]
    UnableToAllocate(SequenceId),
    #[error("Autoinc constraint on table {0:?} spans more than one column: {1:?}")]
    MultiColumnAutoInc(TableId, NonEmpty<ColId>),
}

const SEQUENCE_PREALLOCATION_AMOUNT: i128 = 4_096;

/// A `DataRef` represents a row stored in a table.
///
/// A table row always has a [`DataKey`] associated with it.
/// This is in contrast to rows that are materialized during query execution
/// which may or may not have an associated `DataKey`.
#[derive(Copy, Clone)]
pub struct DataRef<'a> {
    id: &'a DataKey,
    data: &'a ProductValue,
}

impl<'a> DataRef<'a> {
    fn new(id: &'a RowId, data: &'a ProductValue) -> Self {
        let id = &id.0;
        Self { id, data }
    }

    pub fn view(self) -> &'a ProductValue {
        self.data
    }

    pub fn id(self) -> &'a DataKey {
        self.id
    }

    pub fn to_rel_value(self) -> RelValue {
        RelValue::new(self.data.clone(), Some(*self.id))
    }
}

// Type aliases for lock gaurds
type SharedWriteGuard<T> = ArcRwLockWriteGuard<RawRwLock, T>;
type SharedMutexGuard<T> = ArcMutexGuard<RawMutex, T>;
type SharedReadGuard<T> = ArcRwLockReadGuard<RawRwLock, T>;

#[allow(dead_code)]
pub struct TxId {
    committed_state_shared_lock: SharedReadGuard<CommittedState>,
    lock_wait_time: Duration,
    timer: Instant,
}

#[allow(dead_code)]
impl TxId {
    fn table_id_from_name(&self, table_name: &str, database_address: Address) -> super::Result<Option<TableId>> {
        self.iter_by_col_eq(
            &ExecutionContext::internal(database_address),
            &ST_TABLES_ID,
            NonEmpty::new(StTableFields::TableName.col_id()),
            AlgebraicValue::String(table_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| TableId(*row.view().elements[0].as_u32().unwrap()))
        })
    }

    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: &TableId) -> super::Result<Iter<'a>> {
        self.committed_state_shared_lock.iter(ctx, table_id)
    }

    fn table_exists(&self, table_id: &TableId) -> bool {
        self.committed_state_shared_lock.table_exists(table_id)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: NonEmpty<ColId>,
        range: R,
    ) -> super::Result<IterByColRange<'a, R>> {
        match self.committed_state_shared_lock.index_seek(table_id, &cols, &range) {
            Some(committed_rows) => Ok(IterByColRange::CommittedIndex(CommittedIndexIter {
                ctx,
                table_id: *table_id,
                tx_state: None,
                committed_state: &self.committed_state_shared_lock,
                committed_rows,
                num_committed_rows_fetched: 0,
            })),
            None => self
                .committed_state_shared_lock
                .iter_by_col_range(ctx, table_id, cols, range),
        }
    }

    fn iter_by_col_eq<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: NonEmpty<ColId>,
        value: AlgebraicValue,
    ) -> super::Result<IterByColEq<'_>> {
        self.iter_by_col_range(ctx, table_id, cols, value)
    }

    fn schema_for_table(&self, table_id: TableId) -> super::Result<Cow<'_, TableSchema>> {
        let ctx = ExecutionContext::default();
        if let Some(schema) = self.committed_state_shared_lock.get_schema(&table_id) {
            return Ok(Cow::Borrowed(schema));
        }

        // Look up the table_name for the table in question.
        let table_id_col = NonEmpty::new(StTableFields::TableId.col_id());

        // let table_id_col = NonEmpty::new(.col_id());
        let value: AlgebraicValue = table_id.into();
        let rows = self
            .iter_by_col_eq(&ctx, &ST_TABLES_ID, table_id_col, table_id.into())?
            .collect::<Vec<_>>();
        let row = rows
            .first()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let el = StTableRow::try_from(row.view())?;
        let table_name = el.table_name.to_owned();
        let table_id = el.table_id;

        // Look up the columns for the table in question.
        let mut columns = self
            .iter_by_col_eq(
                &ctx,
                &ST_COLUMNS_ID,
                NonEmpty::new(StColumnFields::TableId.col_id()),
                value,
            )?
            .map(|row| {
                let el = StColumnRow::try_from(row.view())?;
                Ok(ColumnSchema {
                    table_id: el.table_id,
                    col_pos: el.col_pos,
                    col_name: el.col_name.into(),
                    col_type: el.col_type,
                })
            })
            .collect::<super::Result<Vec<_>>>()?;

        columns.sort_by_key(|col| col.col_pos);

        // Look up the constraints for the table in question.
        let mut constraints = Vec::new();
        for data_ref in self.iter_by_col_eq(
            &ctx,
            &ST_CONSTRAINTS_ID,
            NonEmpty::new(StConstraintFields::TableId.col_id()),
            table_id.into(),
        )? {
            let row = data_ref.view();

            let el = StConstraintRow::try_from(row)?;
            let constraint_schema = ConstraintSchema {
                constraint_id: el.constraint_id,
                constraint_name: el.constraint_name.to_string(),
                constraints: el.constraints,
                table_id: el.table_id,
                columns: el.columns,
            };
            constraints.push(constraint_schema);
        }

        // Look up the sequences for the table in question.
        let mut sequences = Vec::new();
        for data_ref in self.iter_by_col_eq(
            &ctx,
            &ST_SEQUENCES_ID,
            NonEmpty::new(StSequenceFields::TableId.col_id()),
            AlgebraicValue::U32(table_id.into()),
        )? {
            let row = data_ref.view();

            let el = StSequenceRow::try_from(row)?;
            let sequence_schema = SequenceSchema {
                sequence_id: el.sequence_id,
                sequence_name: el.sequence_name.to_string(),
                table_id: el.table_id,
                col_pos: el.col_pos,
                increment: el.increment,
                start: el.start,
                min_value: el.min_value,
                max_value: el.max_value,
                allocated: el.allocated,
            };
            sequences.push(sequence_schema);
        }

        // Look up the indexes for the table in question.
        let mut indexes = Vec::new();
        for data_ref in self.iter_by_col_eq(
            &ctx,
            &ST_INDEXES_ID,
            NonEmpty::new(StIndexFields::TableId.col_id()),
            table_id.into(),
        )? {
            let row = data_ref.view();

            let el = StIndexRow::try_from(row)?;
            let index_schema = IndexSchema {
                table_id: el.table_id,
                columns: el.columns,
                index_name: el.index_name.into(),
                is_unique: el.is_unique,
                index_id: el.index_id,
                index_type: el.index_type,
            };
            indexes.push(index_schema);
        }

        Ok(Cow::Owned(TableSchema::new(
            table_id,
            table_name,
            columns,
            indexes,
            constraints,
            sequences,
            el.table_type,
            el.table_access,
        )))
    }

    fn release(self, ctx: &ExecutionContext) {
        let workload = &ctx.workload();
        let db = &ctx.database();
        let reducer = ctx.reducer_name().unwrap_or_default();
        let elapsed_time = self.timer.elapsed();
        let cpu_time = elapsed_time - self.lock_wait_time;
        DB_METRICS
            .rdb_num_txns
            .with_label_values(workload, db, reducer, &false)
            .inc();
        DB_METRICS
            .rdb_txn_cpu_time_sec
            .with_label_values(workload, db, reducer)
            .observe(cpu_time.as_secs_f64());
        DB_METRICS
            .rdb_txn_elapsed_time_sec
            .with_label_values(workload, db, reducer)
            .observe(elapsed_time.as_secs_f64());
    }
}

/// Represents a Mutable transaction. Holds locks for its duration
///
/// The initialization of this struct is sensitive because improper
/// handling can lead to deadlocks. Therefore, it is strongly recommended to use
/// `Locking::begin_mut_tx()` for instantiation to ensure safe acquisition of locks.
pub struct MutTxId {
    tx_state: TxState,
    committed_state_write_lock: SharedWriteGuard<CommittedState>,
    sequence_state_lock: SharedMutexGuard<SequencesState>,
    memory_lock: SharedMutexGuard<BTreeMap<DataKey, Arc<Vec<u8>>>>,
    lock_wait_time: Duration,
    timer: Instant,
}

struct CommittedState {
    tables: HashMap<TableId, Table>,
}

impl CommittedState {
    fn new() -> Self {
        Self { tables: HashMap::new() }
    }

    fn get_or_create_table(&mut self, table_id: TableId, schema: TableSchema) -> &mut Table {
        self.tables.entry(table_id).or_insert_with(|| Table::new(schema))
    }

    fn get_table(&mut self, table_id: &TableId) -> Option<&mut Table> {
        self.tables.get_mut(table_id)
    }

    fn merge(&mut self, tx_state: TxState, memory: BTreeMap<DataKey, Arc<Vec<u8>>>) -> TxData {
        let mut tx_data = TxData { records: vec![] };
        for (table_id, table) in tx_state.insert_tables {
            let commit_table = self.get_or_create_table(table_id, table.schema.clone());
            // The schema may have been modified in the transaction.
            commit_table.schema = table.schema;

            tx_data.records.extend(table.rows.into_iter().map(|(row_id, row)| {
                commit_table.insert(row_id, row.clone());
                let pv = row;
                let bytes = match row_id.0 {
                    DataKey::Data(data) => Arc::new(data.to_vec()),
                    DataKey::Hash(_) => memory.get(&row_id.0).unwrap().clone(),
                };
                TxRecord {
                    op: TxOp::Insert(bytes),
                    table_id,
                    key: row_id.0,
                    product_value: pv,
                }
            }));

            // Add all newly created indexes to the committed state
            for (_, index) in table.indexes {
                if !commit_table.indexes.contains_key(&index.cols) {
                    commit_table.insert_index(index);
                }
            }
        }
        for (table_id, row_ids) in tx_state.delete_tables {
            // NOTE: it is possible that the delete_tables contain a row in a table
            // that was created in the current transaction and not committed yet.
            // These delete row operations should be skipped here. e.g.
            //
            // 1. Start a transaction
            // 2. Create a table (table_id = 1)
            // 3. Insert a row (row_id = 1) into table 1
            // 4. Delete row 1 from table 1
            // 5. Commit the transaction
            if let Some(table) = self.get_table(&table_id) {
                for row_id in row_ids {
                    if let Some(pv) = table.delete(&row_id) {
                        tx_data.records.push(TxRecord {
                            op: TxOp::Delete,
                            table_id,
                            key: row_id.0,
                            product_value: pv,
                        })
                    }
                }
            }
        }
        tx_data
    }

    pub fn index_seek<'a>(
        &'a self,
        table_id: &TableId,
        cols: &NonEmpty<ColId>,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        if let Some(table) = self.tables.get(table_id) {
            table.index_seek(cols, range)
        } else {
            None
        }
    }

    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: &TableId) -> super::Result<Iter<'a>> {
        if self.table_exists(table_id) {
            return Ok(Iter::new(ctx, *table_id, None, self));
        }
        Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into())
    }

    fn table_exists(&self, table_id: &TableId) -> bool {
        self.tables.contains_key(table_id)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: NonEmpty<ColId>,
        range: R,
    ) -> super::Result<IterByColRange<'a, R>> {
        Ok(IterByColRange::Scan(ScanIterByColRange {
            range,
            cols,
            scan_iter: self.iter(ctx, table_id)?,
        }))
    }

    fn iter_by_col_eq<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: NonEmpty<ColId>,
        value: AlgebraicValue,
    ) -> super::Result<IterByColEq<'_>> {
        self.iter_by_col_range(ctx, table_id, cols, value)
    }

    fn bootstrap_system_tables(&mut self, database_address: Address) -> Result<(), DBError> {
        let mut sequences_start: HashMap<TableId, i128> = HashMap::with_capacity(10);

        // Insert the table row into st_tables, creating st_tables if it's missing
        let st_tables = self.get_or_create_table(ST_TABLES_ID, st_table_schema());

        // Insert the table row into `st_tables` for all system tables
        for schema in system_tables() {
            let table_id = schema.table_id;
            // Reset the row count metric for this system table
            METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &table_id.0)
                .set(0);

            let row = StTableRow {
                table_id,
                table_name: schema.table_name,
                table_type: StTableType::System,
                table_access: StAccess::Public,
            };
            let row = ProductValue::from(row);
            let data_key = row.to_data_key();
            st_tables.rows.insert(RowId(data_key), row);

            *sequences_start.entry(ST_TABLES_ID).or_default() += 1;
        }

        // Insert the columns into `st_columns`
        let st_columns = self.get_or_create_table(ST_COLUMNS_ID, st_columns_schema());

        for col in system_tables().into_iter().flat_map(|x| x.columns().to_vec()) {
            let row = StColumnRow {
                table_id: col.table_id,
                col_pos: col.col_pos,
                col_name: col.col_name.clone(),
                col_type: col.col_type.clone(),
            };
            let row = ProductValue::from(row);
            let data_key = row.to_data_key();

            st_columns.rows.insert(RowId(data_key), row);
            // Increment row count for st_columns
            METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &ST_COLUMNS_ID.into())
                .inc();
        }

        // Insert the FK sorted by table/column so it show together when queried.

        // Insert constraints into `st_constraints`
        let st_constraints = self.get_or_create_table(ST_CONSTRAINTS_ID, st_constraints_schema());

        for (i, constraint) in system_tables()
            .iter()
            .flat_map(|x| &x.constraints)
            .sorted_by_key(|x| (x.table_id, x.columns.clone()))
            .enumerate()
        {
            let row = StConstraintRow {
                constraint_id: i.into(),
                columns: constraint.columns.clone(),
                constraint_name: constraint.constraint_name.clone(),
                constraints: constraint.constraints,
                table_id: constraint.table_id,
            };
            let row = ProductValue::from(row);
            let data_key = row.to_data_key();
            st_constraints.rows.insert(RowId(data_key), row);
            // Increment row count for st_constraints
            METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &ST_CONSTRAINTS_ID.into())
                .inc();

            *sequences_start.entry(ST_CONSTRAINTS_ID).or_default() += 1;
        }

        // Insert the indexes into `st_indexes`
        let st_indexes = self.get_or_create_table(ST_INDEXES_ID, st_indexes_schema());

        for (i, index) in system_tables()
            .iter()
            .flat_map(|x| &x.indexes)
            .sorted_by_key(|x| (&x.table_id, &x.columns))
            .enumerate()
        {
            let row = StIndexRow {
                index_id: i.into(),
                table_id: index.table_id,
                index_type: index.index_type,
                columns: index.columns.clone(),
                index_name: index.index_name.clone(),
                is_unique: index.is_unique,
            };
            let row = ProductValue::from(row);
            let data_key = row.to_data_key();
            st_indexes.rows.insert(RowId(data_key), row);
            // Increment row count for st_indexes
            METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &ST_INDEXES_ID.into())
                .inc();

            *sequences_start.entry(ST_INDEXES_ID).or_default() += 1;
        }

        // We don't add the row here but with `MutProgrammable::set_program_hash`, but we need to register the table
        // in the internal state.
        self.get_or_create_table(ST_MODULE_ID, st_module_schema());

        let st_sequences = self.get_or_create_table(ST_SEQUENCES_ID, st_sequences_schema());

        // We create sequences last to get right the starting number
        // so, we don't sort here
        for (i, col) in system_tables().into_iter().flat_map(|x| x.sequences).enumerate() {
            //Is required to advance the start position before insert the row
            *sequences_start.entry(ST_SEQUENCES_ID).or_default() += 1;

            let row = StSequenceRow {
                sequence_id: i.into(),
                sequence_name: col.sequence_name,
                table_id: col.table_id,
                col_pos: col.col_pos,
                increment: col.increment,
                start: *sequences_start.get(&col.table_id).unwrap_or(&col.start),
                min_value: col.min_value,
                max_value: col.max_value,
                allocated: col.allocated,
            };
            let row = ProductValue::from(row);
            let data_key = row.to_data_key();
            st_sequences.rows.insert(RowId(data_key), row);
            // Increment row count for st_sequences
            METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &ST_SEQUENCES_ID.into())
                .inc();
        }

        Ok(())
    }

    fn build_sequence_state(&mut self, sequence_state: &mut SequencesState) -> super::Result<()> {
        let st_sequences = self.tables.get(&ST_SEQUENCES_ID).unwrap();
        for row in st_sequences.scan_rows() {
            let sequence = StSequenceRow::try_from(row)?;
            // TODO: The system tables have initialized their value already, but this is wrong:
            // If we exceed  `SEQUENCE_PREALLOCATION_AMOUNT` we will get a unique violation
            let is_system_table = self
                .tables
                .get(&sequence.table_id)
                .map_or(false, |x| x.schema.table_type == StTableType::System);

            let schema = sequence.to_owned().into();

            let mut seq = Sequence::new(schema);
            // Now we need to recover the last allocation value.
            if !is_system_table && seq.value < sequence.allocated + 1 {
                seq.value = sequence.allocated + 1;
            }

            sequence_state.sequences.insert(sequence.sequence_id, seq);
        }
        Ok(())
    }

    fn build_indexes(&mut self) -> super::Result<()> {
        let st_indexes = self.tables.get(&ST_INDEXES_ID).unwrap();
        let rows = st_indexes.scan_rows().cloned().collect::<Vec<_>>();
        for row in rows {
            let index_row = StIndexRow::try_from(&row)?;
            let table = self.get_table(&index_row.table_id).unwrap();
            let mut index = BTreeIndex::new(
                index_row.index_id,
                index_row.table_id,
                index_row.columns.clone(),
                index_row.index_name.into(),
                index_row.is_unique,
            );
            index.build_from_rows(table.scan_rows())?;
            table.indexes.insert(index_row.columns, index);
        }
        Ok(())
    }

    /// After replaying all old transactions, tables which have rows will
    /// have been created in memory, but tables with no rows will not have
    /// been created. This function ensures that they are created.
    fn build_missing_tables(&mut self) -> super::Result<()> {
        let st_tables = self.tables.get(&ST_TABLES_ID).unwrap();
        let rows = st_tables.scan_rows().cloned().collect::<Vec<_>>();
        for row in rows {
            let table_row = StTableRow::try_from(&row)?;
            let table_id = table_row.table_id;
            if self.get_table(&table_id).is_none() {
                let schema = self.schema_for_table(table_id)?.into_owned();
                self.tables.insert(table_id, Table::new(schema));
            }
        }
        Ok(())
    }

    fn table_rows(&mut self, table_id: TableId, schema: TableSchema) -> &mut indexmap::IndexMap<RowId, ProductValue> {
        &mut self.tables.entry(table_id).or_insert_with(|| Table::new(schema)).rows
    }

    /// Retrieves the table schema for bootstrapping, using system_tables in `CommittedState`.
    ///
    /// This method is specific to the bootstrapping phase and `Iter` used in it does not consider
    /// transaction data or indexes, unlike `MutTxId::schema_for_table()`.
    /// This is required as bootstrapping is not a transaction.
    // NOTE: It is essential to keep this function in sync with the
    // `Self::create_table`, as it must reflect the same steps used
    // to create database objects.
    /// Return the [TableSchema] of the supplied `table_id` from the internal [Self::tx_state] if exist OR
    /// query the store and add it to the internal [Self::tx_state], then return it.
    #[tracing::instrument(skip_all)]
    fn schema_for_table(&self, table_id: TableId) -> super::Result<Cow<'_, TableSchema>> {
        let ctx = ExecutionContext::default();
        if let Some(schema) = self.get_schema(&table_id) {
            return Ok(Cow::Borrowed(schema));
        }

        // Look up the table_name for the table in question.
        let table_id_col = NonEmpty::new(StTableFields::TableId.col_id());

        // let table_id_col = NonEmpty::new(.col_id());
        let value: AlgebraicValue = table_id.into();
        let rows = self
            .iter_by_col_eq(&ctx, &ST_TABLES_ID, table_id_col, table_id.into())?
            .collect::<Vec<_>>();
        let row = rows
            .first()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let el = StTableRow::try_from(row.view())?;
        let table_name = el.table_name.to_owned();
        let table_id = el.table_id;

        // Look up the columns for the table in question.
        let mut columns = self
            .iter_by_col_eq(
                &ctx,
                &ST_COLUMNS_ID,
                NonEmpty::new(StColumnFields::TableId.col_id()),
                value,
            )?
            .map(|row| {
                let el = StColumnRow::try_from(row.view())?;
                Ok(ColumnSchema {
                    table_id: el.table_id,
                    col_pos: el.col_pos,
                    col_name: el.col_name.into(),
                    col_type: el.col_type,
                })
            })
            .collect::<super::Result<Vec<_>>>()?;

        columns.sort_by_key(|col| col.col_pos);

        // Look up the constraints for the table in question.
        let mut constraints = Vec::new();
        for data_ref in self.iter_by_col_eq(
            &ctx,
            &ST_CONSTRAINTS_ID,
            NonEmpty::new(StConstraintFields::TableId.col_id()),
            table_id.into(),
        )? {
            let row = data_ref.view();

            let el = StConstraintRow::try_from(row)?;
            let constraint_schema = ConstraintSchema {
                constraint_id: el.constraint_id,
                constraint_name: el.constraint_name.to_string(),
                constraints: el.constraints,
                table_id: el.table_id,
                columns: el.columns,
            };
            constraints.push(constraint_schema);
        }

        // Look up the sequences for the table in question.
        let mut sequences = Vec::new();
        for data_ref in self.iter_by_col_eq(
            &ctx,
            &ST_SEQUENCES_ID,
            NonEmpty::new(StSequenceFields::TableId.col_id()),
            AlgebraicValue::U32(table_id.into()),
        )? {
            let row = data_ref.view();

            let el = StSequenceRow::try_from(row)?;
            let sequence_schema = SequenceSchema {
                sequence_id: el.sequence_id,
                sequence_name: el.sequence_name.to_string(),
                table_id: el.table_id,
                col_pos: el.col_pos,
                increment: el.increment,
                start: el.start,
                min_value: el.min_value,
                max_value: el.max_value,
                allocated: el.allocated,
            };
            sequences.push(sequence_schema);
        }

        // Look up the indexes for the table in question.
        let mut indexes = Vec::new();
        for data_ref in self.iter_by_col_eq(
            &ctx,
            &ST_INDEXES_ID,
            NonEmpty::new(StIndexFields::TableId.col_id()),
            table_id.into(),
        )? {
            let row = data_ref.view();

            let el = StIndexRow::try_from(row)?;
            let index_schema = IndexSchema {
                table_id: el.table_id,
                columns: el.columns,
                index_name: el.index_name.into(),
                is_unique: el.is_unique,
                index_id: el.index_id,
                index_type: el.index_type,
            };
            indexes.push(index_schema);
        }

        Ok(Cow::Owned(TableSchema::new(
            table_id,
            table_name,
            columns,
            indexes,
            constraints,
            sequences,
            el.table_type,
            el.table_access,
        )))
    }

    fn get_schema(&self, table_id: &TableId) -> Option<&TableSchema> {
        self.tables.get(table_id).map(|table| table.get_schema())
    }
}

/// `TxState` tracks all of the modifications made during a particular transaction.
/// Rows inserted during a transaction will be added to insert_tables, and similarly,
/// rows deleted in the transaction will be added to delete_tables.
///
/// Note that the state of a row at the beginning of a transaction is not tracked here,
/// but rather in the `CommittedState` structure.
///
/// Note that because a transaction may have several operations performed on the same
/// row, it is not the case that a call to insert a row guarantees that the row
/// will be present in `insert_tables`. Rather, a row will be present in `insert_tables`
/// if the cummulative effect of all the calls results in the row being inserted during
/// this transaction. The same holds for delete tables.
///
/// For a concrete example, suppose a row is already present in a table at the start
/// of a transaction. A call to delete that row will enter it into `delete_tables`.
/// A subsequent call to reinsert that row will not put it into `insert_tables`, but
/// instead remove it from `delete_tables`, as the cummulative effect is to do nothing.
///
/// This data structure also tracks modifications beyond inserting and deleting rows.
/// In particular, creating indexes and sequences is tracked by `insert_tables`.
///
/// This means that we have the following invariants, within `TxState` and also
/// the corresponding `CommittedState`:
///   - any row in `insert_tables` must not be in the associated `CommittedState`
///   - any row in `delete_tables` must be in the associated `CommittedState`
///   - any row cannot be in both `insert_tables` and `delete_tables`
struct TxState {
    //NOTE: Need to preserve order to correctly restore the db after reopen
    /// For each table,  additions have
    insert_tables: BTreeMap<TableId, Table>,
    delete_tables: BTreeMap<TableId, BTreeSet<RowId>>,
}

/// Represents whether a row has been previously committed, inserted
/// or deleted this transaction, or simply not present at all.
enum RowState<'a> {
    /// The row is present in the table because it was inserted
    /// in a previously committed transaction.
    Committed(&'a ProductValue),
    /// The row is present because it has been inserted in the
    /// current transaction.
    Insert(&'a ProductValue),
    /// The row is absent because it has been deleted in the
    /// current transaction.
    Delete,
    /// The row is not present in the table.
    Absent,
}

impl TxState {
    pub fn new() -> Self {
        Self {
            insert_tables: BTreeMap::new(),
            delete_tables: BTreeMap::new(),
        }
    }

    /// Returns the state of `row_id` within the table identified by `table_id`.
    #[tracing::instrument(skip_all)]
    pub fn get_row_op(&self, table_id: &TableId, row_id: &RowId) -> RowState {
        if let Some(true) = self.delete_tables.get(table_id).map(|set| set.contains(row_id)) {
            return RowState::Delete;
        }
        let Some(table) = self.insert_tables.get(table_id) else {
            return RowState::Absent;
        };
        table.get_row(row_id).map(RowState::Insert).unwrap_or(RowState::Absent)
    }

    #[tracing::instrument(skip_all)]
    pub fn get_row(&self, table_id: &TableId, row_id: &RowId) -> Option<&ProductValue> {
        if Some(true) == self.delete_tables.get(table_id).map(|set| set.contains(row_id)) {
            return None;
        }
        let Some(table) = self.insert_tables.get(table_id) else {
            return None;
        };
        table.get_row(row_id)
    }

    pub fn get_insert_table_mut(&mut self, table_id: &TableId) -> Option<&mut Table> {
        self.insert_tables.get_mut(table_id)
    }

    pub fn get_insert_table(&self, table_id: &TableId) -> Option<&Table> {
        self.insert_tables.get(table_id)
    }

    pub fn get_or_create_delete_table(&mut self, table_id: TableId) -> &mut BTreeSet<RowId> {
        self.delete_tables.entry(table_id).or_default()
    }

    /// When there's an index on `cols`,
    /// returns an iterator over the [BTreeIndex] that yields all the `RowId`s
    /// that match the specified `value` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    ///
    /// For a unique index this will always yield at most one `RowId`.
    /// When there is no index this returns `None`.
    pub fn index_seek<'a>(
        &'a self,
        table_id: &TableId,
        cols: &NonEmpty<ColId>,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        self.insert_tables.get(table_id)?.index_seek(cols, range)
    }
}

struct SequencesState {
    sequences: HashMap<SequenceId, Sequence>,
}

impl SequencesState {
    pub fn new() -> Self {
        Self {
            sequences: HashMap::new(),
        }
    }

    pub fn get_sequence_mut(&mut self, seq_id: SequenceId) -> Option<&mut Sequence> {
        self.sequences.get_mut(&seq_id)
    }
}

impl MutTxId {
    fn drop_col_eq(
        &mut self,
        table_id: TableId,
        col_pos: ColId,
        value: AlgebraicValue,
        database_address: Address,
    ) -> super::Result<()> {
        let ctx = ExecutionContext::internal(database_address);
        let rows = self.iter_by_col_eq(&ctx, &table_id, col_pos, value)?;
        let ids_to_delete = rows.map(|row| RowId(*row.id())).collect::<Vec<_>>();
        if ids_to_delete.is_empty() {
            return Err(TableError::IdNotFound(SystemTable::st_columns, col_pos.0).into());
        }
        self.delete(&table_id, ids_to_delete);

        Ok(())
    }

    fn find_by_col_eq<'a, T>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: TableId,
        col_pos: ColId,
        value: AlgebraicValue,
    ) -> super::Result<Option<T>>
    where
        T: TryFrom<&'a ProductValue>,
        <T as TryFrom<&'a ProductValue>>::Error: Into<DBError>,
    {
        let mut rows = self.iter_by_col_eq(ctx, &table_id, col_pos, value)?;
        rows.next()
            .map(|row| T::try_from(row.view()).map_err(Into::into))
            .transpose()
    }

    #[tracing::instrument(skip_all)]
    fn get_next_sequence_value(&mut self, seq_id: SequenceId, database_address: Address) -> super::Result<i128> {
        {
            let Some(sequence) = self.sequence_state_lock.get_sequence_mut(seq_id) else {
                return Err(SequenceError::NotFound(seq_id).into());
            };

            // If there are allocated sequence values, return the new value, if it is not bigger than
            // the upper range of `sequence.allocated`
            if let Some(value) = sequence.gen_next_value().filter(|v| v < &sequence.allocated()) {
                return Ok(value);
            }
        }
        // Allocate new sequence values
        // If we're out of allocations, then update the sequence row in st_sequences to allocate a fresh batch of sequences.
        let ctx = ExecutionContext::internal(database_address);
        let old_seq_row = self
            .iter_by_col_eq(&ctx, &ST_SEQUENCES_ID, StSequenceFields::SequenceId, seq_id.into())?
            .last()
            .unwrap()
            .data;
        let (seq_row, old_seq_row_id) = {
            let old_seq_row_id = RowId(old_seq_row.to_data_key());
            let mut seq_row = StSequenceRow::try_from(old_seq_row)?.to_owned();

            let Some(sequence) = self.sequence_state_lock.get_sequence_mut(seq_id) else {
                return Err(SequenceError::NotFound(seq_id).into());
            };
            seq_row.allocated = sequence.nth_value(SEQUENCE_PREALLOCATION_AMOUNT as usize);
            sequence.set_allocation(seq_row.allocated);
            (seq_row, old_seq_row_id)
        };

        self.delete(&ST_SEQUENCES_ID, [old_seq_row_id]);
        self.insert(ST_SEQUENCES_ID, ProductValue::from(seq_row), database_address)?;

        let Some(sequence) = self.sequence_state_lock.get_sequence_mut(seq_id) else {
            return Err(SequenceError::NotFound(seq_id).into());
        };
        if let Some(value) = sequence.gen_next_value() {
            return Ok(value);
        }
        Err(SequenceError::UnableToAllocate(seq_id).into())
    }

    fn create_sequence(
        &mut self,
        table_id: TableId,
        seq: SequenceDef,
        database_address: Address,
    ) -> super::Result<SequenceId> {
        log::trace!(
            "SEQUENCE CREATING: {} for table: {} and col: {}",
            seq.sequence_name,
            table_id,
            seq.col_pos
        );

        // Insert the sequence row into st_sequences
        // NOTE: Because st_sequences has a unique index on sequence_name, this will
        // fail if the table already exists.
        let sequence_row = StSequenceRow {
            sequence_id: 0.into(), // autogen'd
            sequence_name: seq.sequence_name,
            table_id,
            col_pos: seq.col_pos,
            allocated: seq.allocated,
            increment: seq.increment,
            start: seq.start.unwrap_or(1),
            min_value: seq.min_value.unwrap_or(1),
            max_value: seq.max_value.unwrap_or(i128::MAX),
        };
        let row = sequence_row.into();
        let result = self.insert(ST_SEQUENCES_ID, row, database_address)?;
        // TODO(centril): `result` is already owned, so pass that in.
        let sequence_row = StSequenceRow::try_from(&result)?.to_owned();
        let sequence_id = sequence_row.sequence_id;

        let schema: SequenceSchema = sequence_row.into();
        let insert_table = self.get_insert_table_mut(schema.table_id)?;
        insert_table.schema.update_sequence(schema.clone());
        self.sequence_state_lock
            .sequences
            .insert(schema.sequence_id, Sequence::new(schema));

        log::trace!("SEQUENCE CREATED: id = {}", sequence_id);

        Ok(sequence_id)
    }

    fn get_insert_table_mut(&mut self, table_id: TableId) -> super::Result<&mut Table> {
        self.tx_state
            .get_insert_table_mut(&table_id)
            .ok_or_else(|| TableError::IdNotFoundState(table_id).into())
    }

    fn drop_sequence(&mut self, sequence_id: SequenceId, database_address: Address) -> super::Result<()> {
        let ctx = ExecutionContext::internal(database_address);

        let st: StSequenceRow<&str> = self
            .find_by_col_eq(
                &ctx,
                ST_SEQUENCES_ID,
                StSequenceFields::SequenceId.col_id(),
                sequence_id.into(),
            )?
            .unwrap();

        let table_id = st.table_id;

        self.drop_col_eq(
            ST_SEQUENCES_ID,
            StSequenceFields::SequenceId.col_id(),
            sequence_id.into(),
            database_address,
        )?;

        self.sequence_state_lock.sequences.remove(&sequence_id);
        if let Some(insert_table) = self.tx_state.get_insert_table_mut(&table_id) {
            insert_table.schema.remove_sequence(sequence_id);
        }
        Ok(())
    }

    fn sequence_id_from_name(&self, seq_name: &str, database_address: Address) -> super::Result<Option<SequenceId>> {
        self.iter_by_col_eq(
            &ExecutionContext::internal(database_address),
            &ST_SEQUENCES_ID,
            StSequenceFields::SequenceName,
            AlgebraicValue::String(seq_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next().map(|row| {
                let id = row.view().elements[0].as_u32().unwrap();
                (*id).into()
            })
        })
    }

    fn create_constraint(
        &mut self,
        table_id: TableId,
        constraint: ConstraintDef,
        database_address: Address,
    ) -> super::Result<ConstraintId> {
        log::trace!(
            "CONSTRAINT CREATING: {} for table: {} and cols: {:?}",
            constraint.constraint_name,
            table_id,
            constraint.columns
        );

        // Verify we have 1 column if need `auto_inc`
        if constraint.constraints.has_autoinc() && constraint.columns.len() != 1 {
            return Err(SequenceError::MultiColumnAutoInc(table_id, constraint.columns).into());
        };

        // Insert the constraint row into st_constraint
        // NOTE: Because st_constraint has a unique index on constraint_name, this will
        // fail if the table already exists.
        let constraint_row = StConstraintRow {
            constraint_id: 0.into(), // autogen'd
            columns: constraint.columns.clone(),
            constraint_name: constraint.constraint_name.clone(),
            constraints: constraint.constraints,
            table_id,
        };

        let row = ProductValue::from(constraint_row);
        let result = self.insert(ST_CONSTRAINTS_ID, row, database_address)?;
        let constraint_row = StConstraintRow::try_from(&result)?;
        let constraint_id = constraint_row.constraint_id;

        let mut constraint = ConstraintSchema::from_def(table_id, constraint);
        constraint.constraint_id = constraint_id;
        let insert_table = self.get_insert_table_mut(constraint.table_id)?;
        let constraint_name = constraint.constraint_name.clone();
        insert_table.schema.update_constraint(constraint);

        log::trace!("CONSTRAINT CREATED: {}", constraint_name);

        Ok(constraint_id)
    }

    fn drop_constraint(&mut self, constraint_id: ConstraintId, database_address: Address) -> super::Result<()> {
        let ctx = ExecutionContext::internal(database_address);

        let st: StConstraintRow<&str> = self
            .find_by_col_eq(
                &ctx,
                ST_CONSTRAINTS_ID,
                StConstraintFields::ConstraintId.col_id(),
                constraint_id.into(),
            )?
            .unwrap();

        let table_id = st.table_id;

        self.drop_col_eq(
            ST_CONSTRAINTS_ID,
            StConstraintFields::ConstraintId.col_id(),
            constraint_id.into(),
            database_address,
        )?;

        if let Some(insert_table) = self.tx_state.get_insert_table_mut(&table_id) {
            insert_table.schema.remove_constraint(constraint_id);
        }

        Ok(())
    }

    fn constraint_id_from_name(
        &self,
        constraint_name: &str,
        database_address: Address,
    ) -> super::Result<Option<ConstraintId>> {
        let ctx = ExecutionContext::internal(database_address);

        Ok(self
            .find_by_col_eq::<StConstraintRow<&str>>(
                &ctx,
                ST_CONSTRAINTS_ID,
                StConstraintFields::ConstraintName.col_id(),
                AlgebraicValue::String(constraint_name.to_owned()),
            )?
            .map(|x| x.constraint_id))
    }

    fn validate_table(table_schema: &TableDef) -> super::Result<()> {
        if table_name_is_system(&table_schema.table_name) {
            return Err(TableError::System(table_schema.table_name.clone()).into());
        }

        table_schema
            .clone()
            .into_schema(0.into())
            .validated()
            .map_err(|err| DBError::Schema(SchemaErrors(err)))?;

        Ok(())
    }

    // NOTE: It is essential to keep this function in sync with the
    // `Self::schema_for_table`, as it must reflect the same steps used
    // to create database objects when querying for information about the table.
    fn create_table(&mut self, table_schema: TableDef, database_address: Address) -> super::Result<TableId> {
        log::trace!("TABLE CREATING: {}", table_schema.table_name);

        Self::validate_table(&table_schema)?;

        // Insert the table row into `st_tables`
        // NOTE: Because `st_tables` has a unique index on `table_name`, this will
        // fail if the table already exists.
        let row = StTableRow {
            table_id: ST_TABLES_ID,
            table_name: table_schema.table_name.clone(),
            table_type: table_schema.table_type,
            table_access: table_schema.table_access,
        };
        let table_id = StTableRow::try_from(&self.insert(ST_TABLES_ID, row.into(), database_address)?)?.table_id;

        // Generate the full definition of the table, with the generated indexes, constraints, sequences...
        let table_schema = table_schema.into_schema(table_id);

        // Insert the columns into `st_columns`
        for col in table_schema.columns() {
            let row = StColumnRow {
                table_id,
                col_pos: col.col_pos,
                col_name: col.col_name.clone(),
                col_type: col.col_type.clone(),
            };
            self.insert(ST_COLUMNS_ID, row.into(), database_address)?;
        }

        // Create the in memory representation of the table
        // NOTE: This should be done before creating the indexes
        let mut schema_internal = table_schema.clone();
        // Remove the adjacent object that has an unset `id = 0`, they will be created below with the correct `id`
        schema_internal.clear_adjacent_schemas();

        self.create_table_internal(table_id, schema_internal)?;

        // Insert constraints into `st_constraints`
        for constraint in table_schema.constraints {
            self.create_constraint(constraint.table_id, constraint.into(), database_address)?;
        }

        // Insert sequences into `st_sequences`
        for seq in table_schema.sequences {
            self.create_sequence(seq.table_id, seq.into(), database_address)?;
        }

        // Create the indexes for the table
        for index in table_schema.indexes {
            self.create_index(table_id, index.into(), database_address)?;
        }

        log::trace!("TABLE CREATED: {}, table_id:{table_id}", table_schema.table_name);

        Ok(table_id)
    }

    fn create_table_internal(&mut self, table_id: TableId, schema: TableSchema) -> super::Result<()> {
        self.tx_state.insert_tables.insert(table_id, Table::new(schema));
        Ok(())
    }

    fn row_type_for_table(&self, table_id: TableId, database_address: Address) -> super::Result<Cow<'_, ProductType>> {
        // Fetch the `ProductType` from the in memory table if it exists.
        // The `ProductType` is invalidated if the schema of the table changes.
        if let Some(row_type) = self.get_row_type(&table_id) {
            return Ok(Cow::Borrowed(row_type));
        }

        // Look up the columns for the table in question.
        // NOTE: This is quite an expensive operation, although we only need
        // to do this in situations where there is not currently an in memory
        // representation of a table. This would happen in situations where
        // we have created the table in the database, but have not yet
        // represented in memory or inserted any rows into it.
        Ok(match self.schema_for_table(table_id, database_address)? {
            Cow::Borrowed(x) => Cow::Borrowed(x.get_row_type()),
            Cow::Owned(x) => Cow::Owned(x.into_row_type()),
        })
    }

    // NOTE: It is essential to keep this function in sync with the
    // `Self::create_table`, as it must reflect the same steps used
    // to create database objects.
    /// Return the [TableSchema] of the supplied `table_id` from the internal [Self::tx_state] if exist OR
    /// query the store and add it to the internal [Self::tx_state], then return it.
    #[tracing::instrument(skip_all)]
    fn schema_for_table(&self, table_id: TableId, database_address: Address) -> super::Result<Cow<'_, TableSchema>> {
        if let Some(schema) = self.get_schema(&table_id) {
            return Ok(Cow::Borrowed(schema));
        }

        let ctx = ExecutionContext::internal(database_address);

        // Look up the table_name for the table in question.
        // TODO(george): As part of the bootstrapping process, we add a bunch of rows
        // and only at very end do we patch things up and create table metadata, indexes,
        // and so on. Early parts of that process insert rows, and need the schema to do
        // so. We can't just call `iter_by_col_range` here as that would attempt to use the
        // index which we haven't created yet. So instead we just manually Scan here.
        let value: AlgebraicValue = table_id.into();
        let rows = IterByColRange::Scan(ScanIterByColRange {
            range: value,
            cols: StTableFields::TableId.into(),
            scan_iter: self.iter(&ctx, &ST_TABLES_ID)?,
        })
        .collect::<Vec<_>>();
        assert!(rows.len() <= 1, "Expected at most one row in st_tables for table_id");

        let row = rows
            .first()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let el = StTableRow::try_from(row.view())?;
        let table_name = el.table_name.to_owned();
        let table_id = el.table_id;

        // Look up the columns for the table in question.
        let mut columns = Vec::new();
        for data_ref in self.iter_by_col_eq(&ctx, &ST_COLUMNS_ID, StTableFields::TableId, table_id.into())? {
            let row = data_ref.view();

            let el = StColumnRow::try_from(row)?;
            let col_schema = ColumnSchema {
                table_id: el.table_id,
                col_pos: el.col_pos,
                col_name: el.col_name.into(),
                col_type: el.col_type,
            };
            columns.push(col_schema);
        }

        columns.sort_by_key(|col| col.col_pos);

        // Look up the constraints for the table in question.
        let mut constraints = Vec::new();
        for data_ref in self.iter_by_col_eq(&ctx, &ST_CONSTRAINTS_ID, StConstraintFields::TableId, table_id.into())? {
            let row = data_ref.view();

            let el = StConstraintRow::try_from(row)?;
            let constraint_schema = ConstraintSchema {
                constraint_id: el.constraint_id,
                constraint_name: el.constraint_name.to_string(),
                constraints: el.constraints,
                table_id: el.table_id,
                columns: el.columns,
            };
            constraints.push(constraint_schema);
        }

        // Look up the sequences for the table in question.
        let mut sequences = Vec::new();
        for data_ref in self.iter_by_col_eq(
            &ctx,
            &ST_SEQUENCES_ID,
            StSequenceFields::TableId,
            AlgebraicValue::U32(table_id.into()),
        )? {
            let row = data_ref.view();

            let el = StSequenceRow::try_from(row)?;
            let sequence_schema = SequenceSchema {
                sequence_id: el.sequence_id,
                sequence_name: el.sequence_name.to_string(),
                table_id: el.table_id,
                col_pos: el.col_pos,
                increment: el.increment,
                start: el.start,
                min_value: el.min_value,
                max_value: el.max_value,
                allocated: el.allocated,
            };
            sequences.push(sequence_schema);
        }

        // Look up the indexes for the table in question.
        let mut indexes = Vec::new();
        for data_ref in self.iter_by_col_eq(&ctx, &ST_INDEXES_ID, StIndexFields::TableId, table_id.into())? {
            let row = data_ref.view();

            let el = StIndexRow::try_from(row)?;
            let index_schema = IndexSchema {
                table_id: el.table_id,
                columns: el.columns,
                index_name: el.index_name.into(),
                is_unique: el.is_unique,
                index_id: el.index_id,
                index_type: el.index_type,
            };
            indexes.push(index_schema);
        }

        Ok(Cow::Owned(TableSchema::new(
            table_id,
            table_name,
            columns,
            indexes,
            constraints,
            sequences,
            el.table_type,
            el.table_access,
        )))
    }

    fn drop_table(&mut self, table_id: TableId, database_address: Address) -> super::Result<()> {
        let schema = self.schema_for_table(table_id, database_address)?.into_owned();

        for row in schema.indexes {
            self.drop_index(row.index_id, database_address)?;
        }

        for row in schema.sequences {
            self.drop_sequence(row.sequence_id, database_address)?;
        }

        for row in schema.constraints {
            self.drop_constraint(row.constraint_id, database_address)?;
        }

        // Drop the table and their columns
        self.drop_col_eq(
            ST_TABLES_ID,
            StTableFields::TableId.col_id(),
            table_id.into(),
            database_address,
        )?;
        self.drop_col_eq(
            ST_COLUMNS_ID,
            StColumnFields::TableId.col_id(),
            table_id.into(),
            database_address,
        )?;

        // Delete the table and its rows and indexes from memory.
        // TODO: This needs to not remove it from the committed state, because it can still be rolled back.
        // We will have to store the deletion in the TxState and then apply it to the CommittedState in commit.

        // NOT use unwrap
        self.committed_state_write_lock.tables.remove(&table_id);
        Ok(())
    }

    fn rename_table(&mut self, table_id: TableId, new_name: &str, database_address: Address) -> super::Result<()> {
        let ctx = ExecutionContext::internal(database_address);

        let st: StTableRow<&str> = self
            .find_by_col_eq(&ctx, ST_TABLES_ID, StTableFields::TableId.col_id(), table_id.into())?
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let mut st = st.to_owned();

        let row_ids = RowId(ProductValue::from(st.to_owned()).to_data_key());

        self.delete(&ST_TABLES_ID, [row_ids]);
        // Update the table's name in st_tables.
        st.table_name = new_name.to_string();
        self.insert(ST_TABLES_ID, st.to_owned().into(), database_address)?;
        Ok(())
    }

    fn table_id_from_name(&self, table_name: &str, database_address: Address) -> super::Result<Option<TableId>> {
        self.iter_by_col_eq(
            &ExecutionContext::internal(database_address),
            &ST_TABLES_ID,
            StTableFields::TableName,
            AlgebraicValue::String(table_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| TableId(*row.view().elements[0].as_u32().unwrap()))
        })
    }

    fn table_name_from_id<'a>(&'a self, ctx: &'a ExecutionContext, table_id: TableId) -> super::Result<Option<&str>> {
        self.iter_by_col_eq(ctx, &ST_TABLES_ID, StTableFields::TableId, table_id.into())
            .map(|mut iter| {
                iter.next()
                    .map(|row| row.view().elements[1].as_string().unwrap().deref())
            })
    }

    fn create_index(
        &mut self,
        table_id: TableId,
        index: IndexDef,
        database_address: Address,
    ) -> super::Result<IndexId> {
        log::trace!(
            "INDEX CREATING: {} for table: {} and col(s): {:?}",
            index.index_name,
            table_id,
            index.columns
        );
        if !self.table_exists(&table_id) {
            return Err(TableError::IdNotFoundState(table_id).into());
        }

        // Insert the index row into st_indexes
        // NOTE: Because st_indexes has a unique index on index_name, this will
        // fail if the index already exists.
        let row = StIndexRow {
            index_id: 0.into(), // Autogen'd
            table_id,
            index_type: index.index_type,
            index_name: index.index_name.clone(),
            columns: index.columns.clone(),
            is_unique: index.is_unique,
        };
        let index_id = StIndexRow::try_from(&self.insert(ST_INDEXES_ID, row.into(), database_address)?)?.index_id;

        let mut index = IndexSchema::from_def(table_id, index);
        index.index_id = index_id;
        let index_name = index.index_name.clone();
        let columns = index.columns.clone();
        self.create_index_internal(index, database_address)?;

        log::trace!(
            "INDEX CREATED: {} for table: {} and col(s): {:?}",
            index_name,
            table_id,
            columns
        );
        Ok(index_id)
    }

    fn create_index_internal(&mut self, index: IndexSchema, database_address: Address) -> super::Result<()> {
        let index_id = index.index_id;

        let insert_table = if let Some(insert_table) = self.tx_state.get_insert_table_mut(&index.table_id) {
            insert_table
        } else {
            let schema = self.schema_for_table(index.table_id, database_address)?.into_owned();
            self.tx_state.insert_tables.insert(index.table_id, Table::new(schema));

            self.tx_state.get_insert_table_mut(&index.table_id).unwrap()
        };

        let mut insert_index = BTreeIndex::new(
            index_id,
            index.table_id,
            index.columns.clone(),
            index.index_name.to_string(),
            index.is_unique,
        );
        insert_index.build_from_rows(insert_table.scan_rows())?;

        // NOTE: Also add all the rows in the already committed table to the index.
        if let Some(committed_table) = self.committed_state_write_lock.get_table(&index.table_id) {
            insert_index.build_from_rows(committed_table.scan_rows())?;
        }

        insert_table.schema.indexes.push(IndexSchema {
            table_id: index.table_id,
            columns: index.columns.clone(),
            index_name: index.index_name.clone(),
            is_unique: index.is_unique,
            index_id,
            index_type: index.index_type,
        });

        insert_table.indexes.insert(index.columns, insert_index);
        Ok(())
    }

    fn drop_index(&mut self, index_id: IndexId, database_address: Address) -> super::Result<()> {
        log::trace!("INDEX DROPPING: {}", index_id);
        let ctx = ExecutionContext::internal(database_address);

        let st: StIndexRow<&str> = self
            .find_by_col_eq(&ctx, ST_INDEXES_ID, StIndexFields::IndexId.col_id(), index_id.into())?
            .unwrap();
        let table_id = st.table_id;

        // Remove the index from st_indexes.
        self.drop_col_eq(
            ST_INDEXES_ID,
            StIndexFields::IndexId.col_id(),
            index_id.into(),
            database_address,
        )?;

        let clear_indexes = |table: &mut Table| {
            let cols: Vec<_> = table
                .indexes
                .values()
                .filter(|i| i.index_id == index_id)
                .map(|i| i.cols.clone())
                .collect();

            for col in cols {
                table.schema.indexes.retain(|x| x.columns != col);
                table.indexes.remove(&col);
            }
        };

        for (_, table) in self.committed_state_write_lock.tables.iter_mut() {
            clear_indexes(table);
        }
        if let Some(insert_table) = self.tx_state.get_insert_table_mut(&table_id) {
            clear_indexes(insert_table);
        }

        log::trace!("INDEX DROPPED: {}", index_id);
        Ok(())
    }

    fn index_id_from_name(&self, index_name: &str, database_address: Address) -> super::Result<Option<IndexId>> {
        self.iter_by_col_eq(
            &ExecutionContext::internal(database_address),
            &ST_INDEXES_ID,
            StIndexFields::IndexName,
            AlgebraicValue::String(index_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| IndexId(*row.view().elements[0].as_u32().unwrap()))
        })
    }

    fn contains_row(&mut self, table_id: &TableId, row_id: &RowId) -> RowState<'_> {
        match self.tx_state.get_row_op(table_id, row_id) {
            RowState::Committed(_) => unreachable!("a row cannot be committed in a tx state"),
            RowState::Insert(pv) => return RowState::Insert(pv),
            RowState::Delete => return RowState::Delete,
            RowState::Absent => (),
        }
        match self
            .committed_state_write_lock
            .tables
            .get(table_id)
            .and_then(|table| table.rows.get(row_id))
        {
            Some(pv) => RowState::Committed(pv),
            None => RowState::Absent,
        }
    }

    fn table_exists(&self, table_id: &TableId) -> bool {
        self.tx_state.insert_tables.contains_key(table_id)
            || self.committed_state_write_lock.tables.contains_key(table_id)
    }

    fn algebraic_type_is_numeric(ty: &AlgebraicType) -> bool {
        matches!(*ty, |AlgebraicType::I8| AlgebraicType::U8
            | AlgebraicType::I16
            | AlgebraicType::U16
            | AlgebraicType::I32
            | AlgebraicType::U32
            | AlgebraicType::I64
            | AlgebraicType::U64
            | AlgebraicType::I128
            | AlgebraicType::U128)
    }

    fn sequence_value_to_algebraic_value(ty: &AlgebraicType, sequence_value: i128) -> AlgebraicValue {
        match *ty {
            AlgebraicType::I8 => (sequence_value as i8).into(),
            AlgebraicType::U8 => (sequence_value as u8).into(),
            AlgebraicType::I16 => (sequence_value as i16).into(),
            AlgebraicType::U16 => (sequence_value as u16).into(),
            AlgebraicType::I32 => (sequence_value as i32).into(),
            AlgebraicType::U32 => (sequence_value as u32).into(),
            AlgebraicType::I64 => (sequence_value as i64).into(),
            AlgebraicType::U64 => (sequence_value as u64).into(),
            AlgebraicType::I128 => sequence_value.into(),
            AlgebraicType::U128 => (sequence_value as u128).into(),
            _ => unreachable!("should have been prevented in `fn insert`"),
        }
    }

    #[tracing::instrument(skip_all)]
    fn insert(
        &mut self,
        table_id: TableId,
        mut row: ProductValue,
        database_address: Address,
    ) -> super::Result<ProductValue> {
        // TODO: Executing schema_for_table for every row insert is expensive.
        // However we ask for the schema in the [Table] struct instead.
        let schema = self.schema_for_table(table_id, database_address)?;
        let ctx = ExecutionContext::internal(database_address);

        let mut col_to_update = None;
        for seq in &schema.sequences {
            if !row.elements[usize::from(seq.col_pos)].is_numeric_zero() {
                continue;
            }
            for seq_row in self.iter_by_col_eq(&ctx, &ST_SEQUENCES_ID, StSequenceFields::TableId, table_id.into())? {
                let seq_row = seq_row.view();
                let seq_row = StSequenceRow::try_from(seq_row)?;
                if seq_row.col_pos != seq.col_pos {
                    continue;
                }

                col_to_update = Some((seq.col_pos, seq_row.sequence_id));
                break;
            }
        }

        if let Some((col_id, sequence_id)) = col_to_update {
            let col_idx = col_id.idx();
            let col = &schema.columns()[col_idx];
            if !Self::algebraic_type_is_numeric(&col.col_type) {
                return Err(SequenceError::NotInteger {
                    col: format!("{}.{}", &schema.table_name, &col.col_name),
                    found: col.col_type.clone(),
                }
                .into());
            }
            // At this point, we know this will be essentially a cheap copy.
            let col_ty = col.col_type.clone();
            let seq_val = self.get_next_sequence_value(sequence_id, database_address)?;
            row.elements[col_idx] = Self::sequence_value_to_algebraic_value(&col_ty, seq_val);
        }

        self.insert_row_internal(table_id, row.clone())?;
        Ok(row)
    }

    #[tracing::instrument(skip_all)]
    fn insert_row_internal(&mut self, table_id: TableId, row: ProductValue) -> super::Result<()> {
        let mut bytes = Vec::new();
        row.encode(&mut bytes);
        let data_key = DataKey::from_data(&bytes);
        let row_id = RowId(data_key);

        // If the table does exist in the tx state, we need to create it based on the table in the
        // committed state. If the table does not exist in the committed state, it doesn't exist
        // in the database.
        let insert_table = if let Some(table) = self.tx_state.get_insert_table(&table_id) {
            table
        } else {
            let Some(committed_table) = self.committed_state_write_lock.tables.get(&table_id) else {
                return Err(TableError::IdNotFoundState(table_id).into());
            };
            let table = Table {
                schema: committed_table.get_schema().clone(),
                indexes: committed_table
                    .indexes
                    .iter()
                    .map(|(cols, index)| {
                        (
                            cols.clone(),
                            BTreeIndex::new(
                                index.index_id,
                                index.table_id,
                                index.cols.clone(),
                                index.name.clone(),
                                index.is_unique,
                            ),
                        )
                    })
                    .collect(),
                rows: Default::default(),
            };
            self.tx_state.insert_tables.insert(table_id, table);
            self.tx_state.get_insert_table(&table_id).unwrap()
        };

        // Check unique constraints
        for index in insert_table.indexes.values() {
            if index.violates_unique_constraint(&row) {
                let value = row.project_not_empty(&index.cols).unwrap();
                return Err(Self::build_error_unique(index, insert_table, value).into());
            }
        }
        if let Some(table) = self.committed_state_write_lock.tables.get_mut(&table_id) {
            for index in table.indexes.values() {
                let value = index.get_fields(&row)?;
                let Some(violators) = index.get_rows_that_violate_unique_constraint(&value) else {
                    continue;
                };
                for row_id in violators {
                    if let Some(delete_table) = self.tx_state.delete_tables.get(&table_id) {
                        if !delete_table.contains(row_id) {
                            let value = row.project_not_empty(&index.cols).unwrap();
                            return Err(Self::build_error_unique(index, table, value).into());
                        }
                    } else {
                        let value = row.project_not_empty(&index.cols)?;
                        return Err(Self::build_error_unique(index, table, value).into());
                    }
                }
            }
        }

        // Now that we have checked all the constraints, we can perform the actual insertion.
        {
            // We have a few cases to consider, based on the history of this transaction, and
            // whether the row was already present or not at the start of this transaction.
            // 1. If the row was not originally present, and therefore also not deleted by
            //    this transaction, we will add it to `insert_tables`.
            // 2. If the row was originally present, but not deleted by this transaction,
            //    we should fail, as we would otherwise violate set semantics.
            // 3. If the row was originally present, and is currently going to be deleted
            //    by this transaction, we will remove it from `delete_tables`, and the
            //    cummulative effect will be to leave the row in place in the committed state.

            let delete_table = self.tx_state.get_or_create_delete_table(table_id);
            let row_was_previously_deleted = delete_table.remove(&row_id);

            // If the row was just deleted in this transaction and we are re-inserting it now,
            // we're done. Otherwise we have to add the row to the insert table, and into our memory.
            if row_was_previously_deleted {
                return Ok(());
            }

            let insert_table = self.tx_state.get_insert_table_mut(&table_id).unwrap();

            // TODO(cloutiertyler): should probably also check that all the columns are correct? Perf considerations.
            if insert_table.schema.columns().len() != row.elements.len() {
                return Err(TableError::RowInvalidType { table_id, row }.into());
            }

            insert_table.insert(row_id, row);

            match data_key {
                DataKey::Data(_) => (),
                DataKey::Hash(_) => {
                    self.memory_lock.insert(data_key, Arc::new(bytes));
                }
            };
        }

        Ok(())
    }

    fn build_error_unique(index: &BTreeIndex, table: &Table, value: AlgebraicValue) -> IndexError {
        IndexError::UniqueConstraintViolation {
            constraint_name: index.name.clone(),
            table_name: table.schema.table_name.clone(),
            cols: index
                .cols
                .iter()
                .map(|&x| table.schema.columns()[usize::from(x)].col_name.clone())
                .collect(),
            value,
        }
    }

    fn get<'a>(&'a self, table_id: &TableId, row_id: &'a RowId) -> super::Result<Option<DataRef<'a>>> {
        if !self.table_exists(table_id) {
            return Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into());
        }
        match self.tx_state.get_row_op(table_id, row_id) {
            RowState::Committed(_) => unreachable!("a row cannot be committed in a tx state"),
            RowState::Insert(row) => {
                return Ok(Some(DataRef::new(row_id, row)));
            }
            RowState::Delete => {
                return Ok(None);
            }
            RowState::Absent => {}
        }
        Ok(self
            .committed_state_write_lock
            .tables
            .get(table_id)
            .and_then(|table| table.get_row(row_id))
            .map(|row| DataRef::new(row_id, row)))
    }

    fn get_row_type(&self, table_id: &TableId) -> Option<&ProductType> {
        if let Some(row_type) = self
            .tx_state
            .insert_tables
            .get(table_id)
            .map(|table| table.get_row_type())
        {
            return Some(row_type);
        }
        self.committed_state_write_lock
            .tables
            .get(table_id)
            .map(|table| table.get_row_type())
    }

    fn get_schema(&self, table_id: &TableId) -> Option<&TableSchema> {
        if let Some(schema) = self
            .tx_state
            .insert_tables
            .get(table_id)
            .map(|table| table.get_schema())
        {
            return Some(schema);
        }
        self.committed_state_write_lock
            .tables
            .get(table_id)
            .map(|table| table.get_schema())
    }

    fn delete(&mut self, table_id: &TableId, row_ids: impl IntoIterator<Item = RowId>) -> u32 {
        row_ids
            .into_iter()
            .map(|row_id| self.delete_row_internal(table_id, &row_id) as u32)
            .sum()
    }

    fn delete_row_internal(&mut self, table_id: &TableId, row_id: &RowId) -> bool {
        match self.contains_row(table_id, row_id) {
            RowState::Committed(_) => {
                // If the row is present because of a previously committed transaction,
                // we need to add it to the appropriate delete_table.
                self.tx_state.get_or_create_delete_table(*table_id).insert(*row_id);
                // True because we did delete the row.
                true
            }
            RowState::Insert(_) => {
                // If the row is present because of a an insertion in this transaction,
                // we need to remove it from the appropriate insert_table.
                let insert_table = self.tx_state.get_insert_table_mut(table_id).unwrap();
                insert_table.delete(row_id);
                // True because we did delete a row.
                true
            }
            RowState::Delete | RowState::Absent => {
                // In either case, there's nothing to delete.
                false
            }
        }
    }

    fn delete_by_rel(&mut self, table_id: &TableId, relation: impl IntoIterator<Item = ProductValue>) -> u32 {
        self.delete(table_id, relation.into_iter().map(|pv| RowId(pv.to_data_key())))
    }

    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: &TableId) -> super::Result<Iter> {
        if self.table_exists(table_id) {
            return Ok(Iter::new(
                ctx,
                *table_id,
                Some(&self.tx_state),
                &self.committed_state_write_lock,
            ));
        }
        Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into())
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `cols` equates to `value`.
    fn iter_by_col_eq<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: impl Into<NonEmpty<ColId>>,
        value: AlgebraicValue,
    ) -> super::Result<IterByColEq<'_>> {
        self.iter_by_col_range(ctx, table_id, cols.into(), value)
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: NonEmpty<ColId>,
        range: R,
    ) -> super::Result<IterByColRange<R>> {
        // We have to index_seek in both the committed state and the current tx state.
        // First, we will check modifications in the current tx. It may be that the table
        // has not been modified yet in the current tx, in which case we will only search
        // the committed state. Finally, the table may not be indexed at all, in which case
        // we fall back to iterating the entire table.
        //
        // We need to check the tx_state first. In particular, it may be that the index
        // was only added in the current transaction.
        // TODO(george): It's unclear that we truly support dynamically creating an index
        // yet. In particular, I don't know if creating an index in a transaction and
        // rolling it back will leave the index in place.
        if let Some(inserted_rows) = self.tx_state.index_seek(table_id, &cols, &range) {
            // The current transaction has modified this table, and the table is indexed.
            Ok(IterByColRange::Index(IndexSeekIterMutTxId {
                ctx,
                table_id: *table_id,
                tx_state: &self.tx_state,
                inserted_rows,
                committed_rows: self.committed_state_write_lock.index_seek(table_id, &cols, &range),
                committed_state: &self.committed_state_write_lock,
                num_committed_rows_fetched: 0,
            }))
        } else {
            // Either the current transaction has not modified this table, or the table is not
            // indexed.
            match self.committed_state_write_lock.index_seek(table_id, &cols, &range) {
                Some(committed_rows) => Ok(IterByColRange::CommittedIndex(CommittedIndexIter {
                    ctx,
                    table_id: *table_id,
                    tx_state: Some(&self.tx_state),
                    committed_state: &self.committed_state_write_lock,
                    committed_rows,
                    num_committed_rows_fetched: 0,
                })),
                None => Ok(IterByColRange::Scan(ScanIterByColRange {
                    range,
                    cols,
                    scan_iter: self.iter(ctx, table_id)?,
                })),
            }
        }
    }

    fn commit(mut self) -> super::Result<Option<TxData>> {
        let memory: BTreeMap<DataKey, Arc<Vec<u8>>> = std::mem::take(&mut self.memory_lock);
        let tx_data = self.committed_state_write_lock.merge(self.tx_state, memory);
        Ok(Some(tx_data))
    }

    fn rollback(self) {
        // TODO: Check that no sequences exceed their allocation after the rollback.
    }
}

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
    /// All of the byte objects inserted in the current transaction.
    memory: Arc<Mutex<BTreeMap<DataKey, Arc<Vec<u8>>>>>,
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
            memory: Arc::new(Mutex::new(BTreeMap::new())),
            committed_state: Arc::new(RwLock::new(CommittedState::new())),
            sequence_state: Arc::new(Mutex::new(SequencesState::new())),
            database_address,
        }
    }

    /// IMPORTANT! This the most delicate function in the entire codebase.
    /// DO NOT CHANGE UNLESS YOU KNOW WHAT YOU'RE DOING!!!
    pub fn bootstrap(database_address: Address) -> Result<Self, DBError> {
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
    pub fn rebuild_state_after_replay(&self) -> Result<(), DBError> {
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

    pub fn replay_transaction(&self, transaction: &Transaction, odb: &dyn ObjectDB) -> Result<(), DBError> {
        let mut committed_state = self.committed_state.write_arc();
        for write in &transaction.writes {
            let table_id = TableId(write.set_id);
            let schema = committed_state.schema_for_table(table_id)?.into_owned();

            match write.operation {
                Operation::Delete => {
                    committed_state
                        .table_rows(table_id, schema)
                        .remove(&RowId(write.data_key));
                    METRICS
                        .rdb_num_table_rows
                        .with_label_values(&self.database_address, &table_id.into())
                        .dec();
                }
                Operation::Insert => {
                    let row_type = schema.get_row_type();
                    let product_value = match write.data_key {
                        DataKey::Data(data) => ProductValue::decode(row_type, &mut &data[..]).unwrap_or_else(|e| {
                            panic!(
                                "Couldn't decode product value from message log: `{}`. Expected row type: {:?}",
                                e, row_type
                            )
                        }),
                        DataKey::Hash(hash) => {
                            let data = odb.get(hash).unwrap_or_else(|| {
                                panic!("Object {hash} referenced from transaction not present in object DB");
                            });
                            ProductValue::decode(row_type, &mut &data[..]).unwrap_or_else(|e| {
                                panic!(
                                    "Couldn't decode product value {} from object DB: `{}`. Expected row type: {:?}",
                                    hash, e, row_type
                                )
                            })
                        }
                    };
                    committed_state
                        .table_rows(table_id, schema)
                        .insert(RowId(write.data_key), product_value);
                    METRICS
                        .rdb_num_table_rows
                        .with_label_values(&self.database_address, &table_id.into())
                        .inc();
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct RowId(pub(crate) DataKey);

impl DataRow for Locking {
    type RowId = RowId;
    type DataRef<'a> = DataRef<'a>;

    fn view_product_value<'a>(&self, data_ref: Self::DataRef<'a>) -> Cow<'a, ProductValue> {
        Cow::Borrowed(data_ref.data)
    }
}

impl traits::Tx for Locking {
    type Tx = MutTxId;

    fn begin_tx(&self) -> Self::Tx {
        self.begin_mut_tx()
    }

    fn release_tx(&self, ctx: &ExecutionContext, tx: Self::Tx) {
        self.rollback_mut_tx(ctx, tx)
    }
}

pub struct Iter<'a> {
    ctx: &'a ExecutionContext<'a>,
    table_id: TableId,
    tx_state: Option<&'a TxState>,
    committed_state: &'a CommittedState,
    stage: ScanStage<'a>,
    num_committed_rows_fetched: u64,
}

impl Drop for Iter<'_> {
    fn drop(&mut self) {
        DB_METRICS
            .rdb_num_rows_fetched
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_or_query(),
                &self.table_id.into(),
            )
            .inc_by(self.num_committed_rows_fetched);
    }
}

impl<'a> Iter<'a> {
    fn new(
        ctx: &'a ExecutionContext,
        table_id: TableId,
        tx_state: Option<&'a TxState>,
        committed_state: &'a CommittedState,
    ) -> Self {
        Self {
            ctx,
            table_id,
            tx_state,
            committed_state,
            stage: ScanStage::Start,
            num_committed_rows_fetched: 0,
        }
    }
}

enum ScanStage<'a> {
    Start,
    CurrentTx {
        iter: indexmap::map::Iter<'a, RowId, ProductValue>,
    },
    Committed {
        iter: indexmap::map::Iter<'a, RowId, ProductValue>,
    },
}

impl<'a> Iterator for Iter<'a> {
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        let table_id = self.table_id;

        // Moves the current scan stage to the current tx if rows were inserted in it.
        // Returns `None` otherwise.
        let maybe_stage_current_tx_inserts = |this: &mut Self| {
            let table = &this.tx_state?;
            let insert_table = table.insert_tables.get(&table_id)?;
            this.stage = ScanStage::CurrentTx {
                iter: insert_table.rows.iter(),
            };
            Some(())
        };

        // The finite state machine goes:
        //      Start --> CurrentTx ---\
        //        |         ^          |
        //        v         |          v
        //     Committed ---/------> Stop
        loop {
            match &mut self.stage {
                ScanStage::Start => {
                    let _span = tracing::debug_span!("ScanStage::Start").entered();
                    if let Some(table) = self.committed_state.tables.get(&table_id) {
                        // The committed state has changes for this table.
                        // Go through them in (1).
                        self.stage = ScanStage::Committed {
                            iter: table.rows.iter(),
                        };
                    } else {
                        // No committed changes, so look for inserts in the current tx in (2).
                        maybe_stage_current_tx_inserts(self);
                    }
                }
                ScanStage::Committed { iter } => {
                    // (1) Go through the committed state for this table.
                    let _span = tracing::debug_span!("ScanStage::Committed").entered();
                    for (row_id, row) in iter {
                        // Increment metric for number of committed rows scanned.
                        self.num_committed_rows_fetched += 1;
                        match self.tx_state {
                            Some(tx_state) => {
                                // Check the committed row's state in the current tx.
                                match tx_state.get_row_op(&table_id, row_id) {
                                    RowState::Committed(_) => unreachable!("a row cannot be committed in a tx state"),
                                    // Do nothing, via (3), we'll get it in the next stage (2).
                                    RowState::Insert(_) |
                                    // Skip it, it's been deleted.
                                    RowState::Delete => {}
                                    // There either are no state changes for the current tx (`None`),
                                    // or there are, but `row_id` specifically has not been changed.
                                    // Either way, the row is in the committed state
                                    // and hasn't been removed in the current tx,
                                    // so it exists and can be returned.
                                    RowState::Absent => return Some(DataRef::new(row_id, row)),
                                }
                            }
                            None => return Some(DataRef::new(row_id, row)),
                        }
                    }
                    // (3) We got here, so we must've exhausted the committed changes.
                    // Start looking in the current tx for inserts, if any, in (2).
                    maybe_stage_current_tx_inserts(self)?
                }
                ScanStage::CurrentTx { iter } => {
                    // (2) look for inserts in the current tx.
                    let _span = tracing::debug_span!("ScanStage::CurrentTx").entered();
                    return iter.next().map(|(id, row)| DataRef::new(id, row));
                }
            }
        }
    }
}

pub struct IndexSeekIterMutTxId<'a> {
    ctx: &'a ExecutionContext<'a>,
    table_id: TableId,
    tx_state: &'a TxState,
    committed_state: &'a CommittedState,
    inserted_rows: BTreeIndexRangeIter<'a>,
    committed_rows: Option<BTreeIndexRangeIter<'a>>,
    num_committed_rows_fetched: u64,
}

impl Drop for IndexSeekIterMutTxId<'_> {
    fn drop(&mut self) {
        // Increment number of index seeks
        DB_METRICS
            .rdb_num_index_seeks
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_or_query(),
                &self.table_id.0,
            )
            .inc();

        // Increment number of index keys scanned
        DB_METRICS
            .rdb_num_keys_scanned
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_or_query(),
                &self.table_id.0,
            )
            .inc_by(self.committed_rows.as_ref().map_or(0, |iter| iter.keys_scanned()));

        // Increment number of rows fetched
        DB_METRICS
            .rdb_num_rows_fetched
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_or_query(),
                &self.table_id.0,
            )
            .inc_by(self.num_committed_rows_fetched);
    }
}

impl<'a> Iterator for IndexSeekIterMutTxId<'a> {
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row_id) = self.inserted_rows.next() {
            return Some(DataRef::new(
                row_id,
                self.tx_state.get_row(&self.table_id, row_id).unwrap(),
            ));
        }

        if let Some(row_id) = self.committed_rows.as_mut().and_then(|i| {
            i.find(|row_id| {
                !self
                    .tx_state
                    .delete_tables
                    .get(&self.table_id)
                    .map_or(false, |table| table.contains(row_id))
            })
        }) {
            self.num_committed_rows_fetched += 1;
            return Some(get_committed_row(self.committed_state, &self.table_id, row_id));
        }

        None
    }
}

pub struct CommittedIndexIter<'a> {
    ctx: &'a ExecutionContext<'a>,
    table_id: TableId,
    tx_state: Option<&'a TxState>,
    committed_state: &'a CommittedState,
    committed_rows: BTreeIndexRangeIter<'a>,
    num_committed_rows_fetched: u64,
}

impl Drop for CommittedIndexIter<'_> {
    fn drop(&mut self) {
        DB_METRICS
            .rdb_num_index_seeks
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_or_query(),
                &self.table_id.0,
            )
            .inc();

        // Increment number of index keys scanned
        DB_METRICS
            .rdb_num_keys_scanned
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_or_query(),
                &self.table_id.0,
            )
            .inc_by(self.committed_rows.keys_scanned());

        // Increment number of rows fetched
        DB_METRICS
            .rdb_num_rows_fetched
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_or_query(),
                &self.table_id.0,
            )
            .inc_by(self.num_committed_rows_fetched);
    }
}

impl<'a> Iterator for CommittedIndexIter<'a> {
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row_id) = self.committed_rows.find(|row_id| match self.tx_state {
            Some(tx_state) => !tx_state
                .delete_tables
                .get(&self.table_id)
                .map_or(false, |table| table.contains(row_id)),
            None => true,
        }) {
            self.num_committed_rows_fetched += 1;
            return Some(get_committed_row(self.committed_state, &self.table_id, row_id));
        }

        None
    }
}

/// Retrieve a commited row.
///
/// Panics if `table_id` and `row_id` do not identify an actually present row.
#[tracing::instrument(skip_all)]
#[inline]
// N.B. This function is used in hot loops, so care is advised when changing it.
fn get_committed_row<'a>(state: &'a CommittedState, table_id: &TableId, row_id: &'a RowId) -> DataRef<'a> {
    DataRef::new(row_id, state.tables.get(table_id).unwrap().get_row(row_id).unwrap())
}

/// An [IterByColRange] for an individual column value.
pub type IterByColEq<'a> = IterByColRange<'a, AlgebraicValue>;

/// An iterator for a range of values in a column.
pub enum IterByColRange<'a, R: RangeBounds<AlgebraicValue>> {
    /// When the column in question does not have an index.
    Scan(ScanIterByColRange<'a, R>),

    /// When the column has an index, and the table
    /// has been modified this transaction.
    Index(IndexSeekIterMutTxId<'a>),

    /// When the column has an index, and the table
    /// has not been modified in this transaction.
    CommittedIndex(CommittedIndexIter<'a>),
}

impl<'a, R: RangeBounds<AlgebraicValue>> Iterator for IterByColRange<'a, R> {
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IterByColRange::Scan(range) => range.next(),
            IterByColRange::Index(range) => range.next(),
            IterByColRange::CommittedIndex(seek) => seek.next(),
        }
    }
}

pub struct ScanIterByColRange<'a, R: RangeBounds<AlgebraicValue>> {
    scan_iter: Iter<'a>,
    cols: NonEmpty<ColId>,
    range: R,
}

impl<'a, R: RangeBounds<AlgebraicValue>> Iterator for ScanIterByColRange<'a, R> {
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        for data_ref in &mut self.scan_iter {
            let row = data_ref.view();
            let value = row.project_not_empty(&self.cols).unwrap();
            if self.range.contains(&value) {
                return Some(data_ref);
            }
        }
        None
    }
}

impl TxDatastore for Locking {
    type Iter<'a> = Iter<'a> where Self: 'a;
    type IterByColEq<'a> = IterByColRange<'a, AlgebraicValue> where Self: 'a;
    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>> = IterByColRange<'a, R> where Self: 'a;

    fn iter_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::Tx,
        table_id: TableId,
    ) -> super::Result<Self::Iter<'a>> {
        tx.iter(ctx, &table_id)
    }

    fn iter_by_col_range_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<NonEmpty<ColId>>,
        range: R,
    ) -> super::Result<Self::IterByColRange<'a, R>> {
        tx.iter_by_col_range(ctx, &table_id, cols.into(), range)
    }

    fn iter_by_col_eq_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::Tx,
        table_id: TableId,
        cols: impl Into<NonEmpty<ColId>>,
        value: AlgebraicValue,
    ) -> super::Result<Self::IterByColEq<'a>> {
        tx.iter_by_col_eq(ctx, &table_id, cols.into(), value)
    }

    fn table_id_exists_tx(&self, tx: &Self::Tx, table_id: &TableId) -> bool {
        tx.table_exists(table_id)
    }

    fn table_id_from_name_tx(&self, tx: &Self::Tx, table_name: &str) -> super::Result<Option<TableId>> {
        tx.table_id_from_name(table_name, self.database_address)
    }

    fn schema_for_table_tx<'tx>(&self, tx: &'tx Self::Tx, table_id: TableId) -> super::Result<Cow<'tx, TableSchema>> {
        tx.schema_for_table(table_id, self.database_address)
    }

    fn get_all_tables_tx<'tx>(
        &self,
        ctx: &ExecutionContext,
        tx: &'tx Self::Tx,
    ) -> super::Result<Vec<Cow<'tx, TableSchema>>> {
        let mut tables = Vec::new();
        let table_rows = self.iter_tx(ctx, tx, ST_TABLES_ID)?.collect::<Vec<_>>();
        for data_ref in table_rows {
            let data = self.view_product_value(data_ref);
            let row = StTableRow::try_from(data.as_ref())?;
            tables.push(self.schema_for_table_tx(tx, row.table_id)?);
        }
        Ok(tables)
    }
}

impl traits::MutTx for Locking {
    type MutTx = MutTxId;

    fn begin_mut_tx(&self) -> Self::MutTx {
        let timer = Instant::now();

        let committed_state_write_lock = self.committed_state.write_arc();
        let sequence_state_lock = self.sequence_state.lock_arc();
        let memory_lock = self.memory.lock_arc();
        let lock_wait_time = timer.elapsed();
        MutTxId {
            committed_state_write_lock,
            sequence_state_lock,
            tx_state: TxState::new(),
            memory_lock,
            lock_wait_time,
            timer,
        }
    }

    fn rollback_mut_tx(&self, ctx: &ExecutionContext, tx: Self::MutTx) {
        let workload = &ctx.workload();
        let db = &ctx.database();
        let reducer = ctx.reducer_name().unwrap_or_default();
        let elapsed_time = tx.timer.elapsed();
        let cpu_time = elapsed_time - tx.lock_wait_time;
        DB_METRICS
            .rdb_num_txns
            .with_label_values(workload, db, reducer, &false)
            .inc();
        DB_METRICS
            .rdb_txn_cpu_time_sec
            .with_label_values(workload, db, reducer)
            .observe(cpu_time.as_secs_f64());
        DB_METRICS
            .rdb_txn_elapsed_time_sec
            .with_label_values(workload, db, reducer)
            .observe(elapsed_time.as_secs_f64());
        tx.rollback();
    }

    fn commit_mut_tx(&self, ctx: &ExecutionContext, tx: Self::MutTx) -> super::Result<Option<TxData>> {
        let workload = &ctx.workload();
        let db = &ctx.database();
        let reducer = ctx.reducer_name().unwrap_or_default();
        let elapsed_time = tx.timer.elapsed();
        let cpu_time = elapsed_time - tx.lock_wait_time;

        let elapsed_time = elapsed_time.as_secs_f64();
        let cpu_time = cpu_time.as_secs_f64();
        // Note, we record empty transactions in our metrics.
        // That is, transactions that don't write any rows to the commit log.
        DB_METRICS
            .rdb_num_txns
            .with_label_values(workload, db, reducer, &true)
            .inc();
        DB_METRICS
            .rdb_txn_cpu_time_sec
            .with_label_values(workload, db, reducer)
            .observe(cpu_time);
        DB_METRICS
            .rdb_txn_elapsed_time_sec
            .with_label_values(workload, db, reducer)
            .observe(elapsed_time);

        fn hash(a: &WorkloadType, b: &Address, c: &str) -> u64 {
            use std::hash::Hash;
            let mut hasher = DefaultHasher::new();
            a.hash(&mut hasher);
            b.hash(&mut hasher);
            c.hash(&mut hasher);
            hasher.finish()
        }

        let mut guard = MAX_TX_CPU_TIME.lock().unwrap();
        let max_cpu_time = *guard
            .entry(hash(workload, db, reducer))
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

        tx.commit()
    }

    #[cfg(test)]
    fn rollback_mut_tx_for_test(&self, tx: Self::MutTx) {
        tx.rollback();
    }

    #[cfg(test)]
    fn commit_mut_tx_for_test(&self, tx: Self::MutTx) -> super::Result<Option<TxData>> {
        tx.commit()
    }
}

impl MutTxDatastore for Locking {
    fn create_table_mut_tx(&self, tx: &mut Self::MutTx, schema: TableDef) -> super::Result<TableId> {
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
    fn row_type_for_table_mut_tx<'tx>(
        &self,
        tx: &'tx Self::MutTx,
        table_id: TableId,
    ) -> super::Result<Cow<'tx, ProductType>> {
        tx.row_type_for_table(table_id, self.database_address)
    }

    /// IMPORTANT! This function is relatively expensive, and much more
    /// expensive than `row_type_for_table_mut_tx`.  Prefer
    /// `row_type_for_table_mut_tx` if you only need to access the `ProductType`
    /// of the table.
    fn schema_for_table_mut_tx<'tx>(
        &self,
        tx: &'tx Self::MutTx,
        table_id: TableId,
    ) -> super::Result<Cow<'tx, TableSchema>> {
        tx.schema_for_table(table_id, self.database_address)
    }

    /// This function is relatively expensive because it needs to be
    /// transactional, however we don't expect to be dropping tables very often.
    fn drop_table_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId) -> super::Result<()> {
        tx.drop_table(table_id, self.database_address)
    }

    fn rename_table_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId, new_name: &str) -> super::Result<()> {
        tx.rename_table(table_id, new_name, self.database_address)
    }

    fn table_id_from_name_mut_tx(&self, tx: &Self::MutTx, table_name: &str) -> super::Result<Option<TableId>> {
        tx.table_id_from_name(table_name, self.database_address)
    }

    fn table_name_from_id_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
    ) -> super::Result<Option<&'a str>> {
        tx.table_name_from_id(ctx, table_id)
    }

    fn create_index_mut_tx(&self, tx: &mut Self::MutTx, table_id: TableId, index: IndexDef) -> super::Result<IndexId> {
        tx.create_index(table_id, index, self.database_address)
    }

    fn drop_index_mut_tx(&self, tx: &mut Self::MutTx, index_id: IndexId) -> super::Result<()> {
        tx.drop_index(index_id, self.database_address)
    }

    fn index_id_from_name_mut_tx(&self, tx: &Self::MutTx, index_name: &str) -> super::Result<Option<IndexId>> {
        tx.index_id_from_name(index_name, self.database_address)
    }

    fn get_next_sequence_value_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> super::Result<i128> {
        tx.get_next_sequence_value(seq_id, self.database_address)
    }

    fn create_sequence_mut_tx(
        &self,
        tx: &mut Self::MutTx,
        table_id: TableId,
        seq: SequenceDef,
    ) -> super::Result<SequenceId> {
        tx.create_sequence(table_id, seq, self.database_address)
    }

    fn drop_sequence_mut_tx(&self, tx: &mut Self::MutTx, seq_id: SequenceId) -> super::Result<()> {
        tx.drop_sequence(seq_id, self.database_address)
    }

    fn sequence_id_from_name_mut_tx(&self, tx: &Self::MutTx, sequence_name: &str) -> super::Result<Option<SequenceId>> {
        tx.sequence_id_from_name(sequence_name, self.database_address)
    }

    fn drop_constraint_mut_tx(&self, tx: &mut Self::MutTx, constraint_id: ConstraintId) -> super::Result<()> {
        tx.drop_constraint(constraint_id, self.database_address)
    }

    fn constraint_id_from_name(&self, tx: &Self::MutTx, constraint_name: &str) -> super::Result<Option<ConstraintId>> {
        tx.constraint_id_from_name(constraint_name, self.database_address)
    }

    fn iter_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
    ) -> super::Result<Self::Iter<'a>> {
        tx.iter(ctx, &table_id)
    }

    fn iter_by_col_range_mut_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<NonEmpty<ColId>>,
        range: R,
    ) -> super::Result<Self::IterByColRange<'a, R>> {
        tx.iter_by_col_range(ctx, &table_id, cols.into(), range)
    }

    fn iter_by_col_eq_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTx,
        table_id: TableId,
        cols: impl Into<NonEmpty<ColId>>,
        value: AlgebraicValue,
    ) -> super::Result<Self::IterByColEq<'a>> {
        tx.iter_by_col_eq(ctx, &table_id, cols, value)
    }

    fn get_mut_tx<'a>(
        &self,
        tx: &'a Self::MutTx,
        table_id: TableId,
        row_id: &'a Self::RowId,
    ) -> super::Result<Option<Self::DataRef<'a>>> {
        tx.get(&table_id, row_id)
    }

    fn delete_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTx,
        table_id: TableId,
        row_ids: impl IntoIterator<Item = Self::RowId>,
    ) -> u32 {
        tx.delete(&table_id, row_ids)
    }

    fn delete_by_rel_mut_tx(
        &self,
        tx: &mut Self::MutTx,
        table_id: TableId,
        relation: impl IntoIterator<Item = ProductValue>,
    ) -> u32 {
        tx.delete_by_rel(&table_id, relation)
    }

    fn insert_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTx,
        table_id: TableId,
        row: ProductValue,
    ) -> super::Result<ProductValue> {
        tx.insert(table_id, row, self.database_address)
    }
}

impl traits::Programmable for Locking {
    fn program_hash(&self, tx: &MutTxId) -> Result<Option<Hash>, DBError> {
        match tx
            .iter(&ExecutionContext::internal(self.database_address), &ST_MODULE_ID)?
            .next()
        {
            None => Ok(None),
            Some(data) => {
                let row = StModuleRow::try_from(data.view())?;
                Ok(Some(row.program_hash))
            }
        }
    }
}

impl traits::MutProgrammable for Locking {
    type FencingToken = u128;

    fn set_program_hash(&self, tx: &mut MutTxId, fence: Self::FencingToken, hash: Hash) -> Result<(), DBError> {
        let ctx = ExecutionContext::internal(self.database_address);
        let mut iter = tx.iter(&ctx, &ST_MODULE_ID)?;
        if let Some(data) = iter.next() {
            let row = StModuleRow::try_from(data.view())?;
            if fence <= row.epoch.0 {
                return Err(anyhow!("stale fencing token: {}, storage is at epoch: {}", fence, row.epoch).into());
            }

            // Note the borrow checker requires that we explictly drop the iterator.
            // That is, before we delete and insert.
            // This is because datastore iterators write to the metric store when dropped.
            // Hence if we don't explicitly drop here,
            // there will be another immutable borrow of self after the two mutable borrows below.
            drop(iter);

            tx.delete_by_rel(&ST_MODULE_ID, Some(ProductValue::from(&row)));
            tx.insert(
                ST_MODULE_ID,
                ProductValue::from(&StModuleRow {
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
            ProductValue::from(&StModuleRow {
                program_hash: hash,
                kind: WASM_MODULE,
                epoch: system_tables::Epoch(fence),
            }),
            self.database_address,
        )?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::datastore::Result;
    use crate::error::IndexError;
    use itertools::Itertools;
    use spacetimedb_lib::error::ResultTest;
    use spacetimedb_primitives::Constraints;
    use spacetimedb_sats::product;

    /// Utility to query the system tables and return their concrete table row
    pub struct SystemTableQuery<'a> {
        db: &'a MutTxId,
        ctx: &'a ExecutionContext<'a>,
    }

    fn query_st_tables<'a>(ctx: &'a ExecutionContext<'a>, tx: &'a MutTxId) -> SystemTableQuery<'a> {
        SystemTableQuery { db: tx, ctx }
    }

    impl SystemTableQuery<'_> {
        pub fn scan_st_tables(&self) -> Result<Vec<StTableRow<String>>> {
            Ok(self
                .db
                .iter(self.ctx, &ST_TABLES_ID)?
                .map(|x| StTableRow::try_from(x.view()).unwrap().to_owned())
                .sorted_by_key(|x| x.table_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_tables_by_col(
            &self,
            cols: impl Into<NonEmpty<ColId>>,
            value: AlgebraicValue,
        ) -> Result<Vec<StTableRow<String>>> {
            Ok(self
                .db
                .iter_by_col_eq(self.ctx, &ST_TABLES_ID, cols, value)?
                .map(|x| StTableRow::try_from(x.view()).unwrap().to_owned())
                .sorted_by_key(|x| x.table_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_columns(&self) -> Result<Vec<StColumnRow<String>>> {
            Ok(self
                .db
                .iter(self.ctx, &ST_COLUMNS_ID)?
                .map(|x| StColumnRow::try_from(x.view()).unwrap().to_owned())
                .sorted_by_key(|x| (x.table_id, x.col_pos))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_columns_by_col(
            &self,
            cols: impl Into<NonEmpty<ColId>>,
            value: AlgebraicValue,
        ) -> Result<Vec<StColumnRow<String>>> {
            Ok(self
                .db
                .iter_by_col_eq(self.ctx, &ST_COLUMNS_ID, cols, value)?
                .map(|x| StColumnRow::try_from(x.view()).unwrap().to_owned())
                .sorted_by_key(|x| (x.table_id, x.col_pos))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_constraints(&self) -> Result<Vec<StConstraintRow<String>>> {
            Ok(self
                .db
                .iter(self.ctx, &ST_CONSTRAINTS_ID)?
                .map(|x| StConstraintRow::try_from(x.view()).unwrap().to_owned())
                .sorted_by_key(|x| x.constraint_id)
                .collect::<Vec<_>>())
        }

        pub fn scan_st_sequences(&self) -> Result<Vec<StSequenceRow<String>>> {
            Ok(self
                .db
                .iter(self.ctx, &ST_SEQUENCES_ID)?
                .map(|x| StSequenceRow::try_from(x.view()).unwrap().to_owned())
                .sorted_by_key(|x| (x.table_id, x.sequence_id))
                .collect::<Vec<_>>())
        }

        pub fn scan_st_indexes(&self) -> Result<Vec<StIndexRow<String>>> {
            Ok(self
                .db
                .iter(self.ctx, &ST_INDEXES_ID)?
                .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
                .sorted_by_key(|x| x.index_id)
                .collect::<Vec<_>>())
        }
    }

    fn u32_str_u32(a: u32, b: &str, c: u32) -> ProductValue {
        product![a, b, c]
    }

    fn get_datastore() -> super::super::Result<Locking> {
        Locking::bootstrap(Address::zero())
    }

    fn col(col: u32) -> NonEmpty<u32> {
        NonEmpty::new(col)
    }

    fn cols(head: u32, tail: Vec<u32>) -> NonEmpty<u32> {
        NonEmpty::from((head, tail))
    }

    fn map_array<A, B: From<A>, const N: usize>(a: [A; N]) -> Vec<B> {
        a.map(Into::into).into()
    }

    struct IndexRow<'a> {
        id: u32,
        table: u32,
        col: NonEmpty<u32>,
        name: &'a str,
        unique: bool,
    }
    impl From<IndexRow<'_>> for StIndexRow<String> {
        fn from(value: IndexRow<'_>) -> Self {
            Self {
                index_id: value.id.into(),
                table_id: value.table.into(),
                columns: value.col.map(ColId),
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
        columns: NonEmpty<u32>,
    }
    impl From<ConstraintRow<'_>> for StConstraintRow<String> {
        fn from(value: ConstraintRow<'_>) -> Self {
            Self {
                constraint_id: value.constraint_id.into(),
                constraint_name: value.constraint_name.into(),
                constraints: value.constraints,
                table_id: value.table_id.into(),
                columns: value.columns.map(ColId),
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
                columns: value.columns.map(ColId),
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
                    columns: NonEmpty::new(0.into()),
                    index_name: "id_idx".into(),
                    is_unique: true,
                    index_type: IndexType::BTree,
                },
                IndexDef {
                    columns: NonEmpty::new(1.into()),
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
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        Ok((datastore, tx, table_id))
    }

    fn all_rows(datastore: &Locking, tx: &MutTxId, table_id: TableId) -> Vec<ProductValue> {
        datastore
            .iter_mut_tx(&ExecutionContext::default(), tx, table_id)
            .unwrap()
            .map(|r| r.view().clone())
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
            .map(|r| r.view().clone())
            .collect()
    }

    #[test]
    fn test_bootstrapping_sets_up_tables() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let tx = datastore.begin_mut_tx();
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
            IndexRow { id: 2, table: 1, col: cols(0, vec![1]), name: "idx_st_columns_table_id_col_pos_unique", unique: true },
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
            ConstraintRow { constraint_id: 2, table_id: 1, columns: cols(0, vec![1]), constraints: Constraints::unique(), constraint_name: "ct_st_columns_table_id_col_pos_unique" },
            ConstraintRow { constraint_id: 3, table_id: 2, columns: col(0), constraints: Constraints::primary_key_auto(), constraint_name: "ct_st_sequence_sequence_id_primary_key_auto" },
            ConstraintRow { constraint_id: 4, table_id: 3, columns: col(0), constraints: Constraints::primary_key_auto(), constraint_name: "ct_st_indexes_index_id_primary_key_auto" },
            ConstraintRow { constraint_id: 5, table_id: 4, columns: col(0), constraints: Constraints::primary_key_auto(), constraint_name: "ct_st_constraints_constraint_id_primary_key_auto" },
        ]));
        datastore.rollback_mut_tx_for_test(tx);
        Ok(())
    }

    #[test]
    fn test_create_table_pre_commit() -> ResultTest<()> {
        let (_, tx, table_id) = setup_table()?;
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let table_rows = query.scan_st_tables_by_col(ColId(0), table_id.into())?;
        #[rustfmt::skip]
        assert_eq!(table_rows, map_array([
            TableRow { id: 6, name: "Foo", ty: StTableType::User, access: StAccess::Public }
        ]));
        let column_rows = query.scan_st_columns_by_col(ColId(0), table_id.into())?;
        #[rustfmt::skip]
        assert_eq!(column_rows, map_array(basic_table_schema_cols()));
        Ok(())
    }

    #[test]
    fn test_create_table_post_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx_for_test(tx)?;
        let tx = datastore.begin_mut_tx();
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let table_rows = query.scan_st_tables_by_col(ColId(0), table_id.into())?;
        #[rustfmt::skip]
        assert_eq!(table_rows, map_array([
            TableRow { id: 6, name: "Foo", ty: StTableType::User, access: StAccess::Public }
        ]));
        let column_rows = query.scan_st_columns_by_col(ColId(0), table_id.into())?;
        #[rustfmt::skip]
        assert_eq!(column_rows, map_array(basic_table_schema_cols()));

        Ok(())
    }

    #[test]
    fn test_create_table_post_rollback() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.rollback_mut_tx_for_test(tx);
        let tx = datastore.begin_mut_tx();
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let table_rows = query.scan_st_tables_by_col(ColId(0), table_id.into())?;
        assert_eq!(table_rows, []);
        let column_rows = query.scan_st_columns_by_col(ColId(0), table_id.into())?;
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
        let tx = datastore.begin_mut_tx();
        let schema = &*datastore.schema_for_table_mut_tx(&tx, table_id)?;
        #[rustfmt::skip]
        assert_eq!(schema, &basic_table_schema_created(table_id));
        Ok(())
    }

    #[test]
    fn test_schema_for_table_alter_indexes() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx_for_test(tx)?;

        let mut tx = datastore.begin_mut_tx();
        let schema = datastore.schema_for_table_mut_tx(&tx, table_id)?.into_owned();

        for index in &*schema.indexes {
            datastore.drop_index_mut_tx(&mut tx, index.index_id)?;
        }
        assert!(
            datastore.schema_for_table_mut_tx(&tx, table_id)?.indexes.is_empty(),
            "no indexes should be left in the schema pre-commit"
        );
        datastore.commit_mut_tx_for_test(tx)?;

        let mut tx = datastore.begin_mut_tx();
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

        let tx = datastore.begin_mut_tx();
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
        let tx = datastore.begin_mut_tx();
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
        let tx = datastore.begin_mut_tx();
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_insert_post_rollback() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        let row = u32_str_u32(15, "Foo", 18); // 15 is ignored.
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx();
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.rollback_mut_tx_for_test(tx);
        let tx = datastore.begin_mut_tx();
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
        let mut tx = datastore.begin_mut_tx();
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
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        for _ in 0..2 {
            let created_row = u32_str_u32(1, "Foo", 18);
            let num_deleted = datastore.delete_by_rel_mut_tx(&mut tx, table_id, [created_row.clone()]);
            assert_eq!(num_deleted, 1);
            assert_eq!(all_rows(&datastore, &tx, table_id).len(), 0);
            datastore.insert_mut_tx(&mut tx, table_id, created_row)?;
            #[rustfmt::skip]
            assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(1, "Foo", 18)]);
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
            Err(DBError::Index(IndexError::UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                cols: _,
                value: _,
            })) => (),
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
        let mut tx = datastore.begin_mut_tx();
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                cols: _,
                value: _,
            })) => (),
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
        let mut tx = datastore.begin_mut_tx();
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
        datastore.rollback_mut_tx_for_test(tx);
        let mut tx = datastore.begin_mut_tx();
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        #[rustfmt::skip]
        assert_eq!(all_rows(&datastore, &tx, table_id), vec![u32_str_u32(2, "Foo", 18)]);
        Ok(())
    }

    #[test]
    fn test_create_index_pre_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let row = u32_str_u32(0, "Foo", 18); // 0 will be ignored.
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let index_def = IndexDef::btree("age_idx".into(), ColId(2), true);
        datastore.create_index_mut_tx(&mut tx, table_id, index_def)?;
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 0, table: 0, col: col(0), name: "idx_st_table_table_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 1, table: 0, col: col(1), name: "idx_st_table_table_name_unique", unique: true },
            IndexRow { id: 2, table: 1, col: cols(0, vec![1]), name: "idx_st_columns_table_id_col_pos_unique", unique: true },
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
            Err(DBError::Index(IndexError::UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                cols: _,
                value: _,
            })) => (),
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
        let mut tx = datastore.begin_mut_tx();
        let index_def = IndexDef::btree("age_idx".into(), ColId(2), true);
        datastore.create_index_mut_tx(&mut tx, table_id, index_def)?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 0, table: 0, col: col(0), name: "idx_st_table_table_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 1, table: 0, col: col(1), name: "idx_st_table_table_name_unique", unique: true },
            IndexRow { id: 2, table: 1, col: cols(0, vec![1]), name: "idx_st_columns_table_id_col_pos_unique", unique: true },
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
            Err(DBError::Index(IndexError::UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                cols: _,
                value: _,
            })) => (),
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
        let mut tx = datastore.begin_mut_tx();
        let index_def = IndexDef::btree("age_idx".into(), ColId(2), true);
        datastore.create_index_mut_tx(&mut tx, table_id, index_def)?;
        datastore.rollback_mut_tx_for_test(tx);
        let mut tx = datastore.begin_mut_tx();
        let ctx = ExecutionContext::default();
        let query = query_st_tables(&ctx, &tx);

        let index_rows = query.scan_st_indexes()?;
        #[rustfmt::skip]
        assert_eq!(index_rows, [
            IndexRow { id: 0, table: 0, col: col(0), name: "idx_st_table_table_id_primary_key_auto_unique", unique: true },
            IndexRow { id: 1, table: 0, col: col(1), name: "idx_st_table_table_name_unique", unique: true },
            IndexRow { id: 2, table: 1, col: cols(0, vec![1]), name: "idx_st_columns_table_id_col_pos_unique", unique: true },
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
                    AlgebraicValue::U32(1),
                )
                .unwrap()
                .map(|data_ref| data_ref.data.clone())
                .collect::<Vec<_>>()
        };

        // Update the db with the same actual value for that row, in a new tx.
        let mut tx = datastore.begin_mut_tx();
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
