mod btree_index;
mod hash_index;
mod sequence;
mod table;

use self::{
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    sequence::Sequence,
    table::Table,
};
use nonempty::NonEmpty;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::{Deref, RangeBounds},
    sync::Arc,
    time::{Duration, Instant},
    vec,
};

use super::{
    system_tables::{
        self, StColumnRow, StIndexRow, StModuleRow, StSequenceRow, StTableRow, INDEX_ID_SEQUENCE_ID,
        SEQUENCE_ID_SEQUENCE_ID, ST_COLUMNS_ID, ST_COLUMNS_ROW_TYPE, ST_INDEXES_ID, ST_INDEX_ROW_TYPE, ST_MODULE_ID,
        ST_SEQUENCES_ID, ST_SEQUENCE_ROW_TYPE, ST_TABLES_ID, ST_TABLE_ROW_TYPE, TABLE_ID_SEQUENCE_ID, WASM_MODULE,
    },
    traits::{
        self, DataRow, IndexDef, IndexSchema, MutTx, MutTxDatastore, SequenceDef, TableDef, TableSchema, TxData,
        TxDatastore,
    },
};

use crate::{
    db::datastore::traits::{TxOp, TxRecord},
    db::{
        datastore::{
            system_tables::{st_columns_schema, st_indexes_schema, st_sequences_schema, st_table_schema},
            traits::ColumnSchema,
        },
        messages::{transaction::Transaction, write::Operation},
        ostorage::ObjectDB,
    },
    error::{DBError, IndexError, TableError},
};
use crate::{
    db::{
        datastore::system_tables::{
            st_constraints_schema, st_module_schema, table_name_is_system, StConstraintRow, SystemTables,
            CONSTRAINT_ID_SEQUENCE_ID, ST_CONSTRAINTS_ID, ST_CONSTRAINT_ROW_TYPE, ST_MODULE_ROW_TYPE,
        },
        db_metrics::DB_METRICS,
    },
    execution_context::ExecutionContext,
};

use anyhow::anyhow;
use parking_lot::{lock_api::ArcMutexGuard, Mutex, RawMutex};
use spacetimedb_lib::{
    auth::{StAccess, StTableType},
    data_key::ToDataKey,
    relation::RelValue,
    Address, DataKey, Hash,
};
use spacetimedb_primitives::{ColId, IndexId, SequenceId, TableId};
use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductType, ProductValue};
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

pub struct MutTxId {
    lock: ArcMutexGuard<RawMutex, Inner>,
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

    fn get_or_create_table(&mut self, table_id: TableId, row_type: &ProductType, schema: &TableSchema) -> &mut Table {
        self.tables
            .entry(table_id)
            .or_insert_with(|| Table::new(row_type.clone(), schema.clone()))
    }

    fn get_table(&mut self, table_id: &TableId) -> Option<&mut Table> {
        self.tables.get_mut(table_id)
    }

    fn merge(&mut self, tx_state: TxState, memory: BTreeMap<DataKey, Arc<Vec<u8>>>) -> TxData {
        let mut tx_data = TxData { records: vec![] };
        for (table_id, table) in tx_state.insert_tables {
            let commit_table = self.get_or_create_table(table_id, &table.row_type, &table.schema);
            // The schema may have been modified in the transaction.
            commit_table.row_type = table.row_type;
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
                    commit_table.insert_btree_index(index);
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
        self.delete_tables.entry(table_id).or_insert_with(BTreeSet::new)
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

struct Inner {
    /// All of the byte objects inserted in the current transaction.
    memory: BTreeMap<DataKey, Arc<Vec<u8>>>,
    /// The state of the database up to the point of the last committed transaction.
    committed_state: CommittedState,
    /// The state of all insertions and deletions in this transaction.
    tx_state: Option<TxState>,
    /// The state of sequence generation in this database.
    sequence_state: SequencesState,
    /// The address of this database.
    database_address: Address,
}

impl Inner {
    pub fn new(database_address: Address) -> Self {
        Self {
            memory: BTreeMap::new(),
            committed_state: CommittedState::new(),
            tx_state: None,
            sequence_state: SequencesState::new(),
            database_address,
        }
    }

    fn bootstrap_system_table(&mut self, schema: TableSchema) -> Result<(), DBError> {
        let table_id = schema.table_id;

        // Insert the table row into st_tables, creating st_tables if it's missing
        let st_tables = self
            .committed_state
            .get_or_create_table(ST_TABLES_ID, &ST_TABLE_ROW_TYPE, &st_table_schema());
        let row = StTableRow {
            table_id,
            table_name: schema.table_name,
            table_type: StTableType::System,
            table_access: StAccess::Public,
        };
        let row: ProductValue = row.into();
        let data_key = row.to_data_key();
        st_tables.rows.insert(RowId(data_key), row);

        // Insert the columns into st_columns
        let first_col_id = schema.columns.first().unwrap().col_id;
        for (i, col) in schema.columns.into_iter().enumerate() {
            let col_name_for_autoinc = col.is_autoinc.then(|| col.col_name.clone());

            let row = StColumnRow {
                table_id,
                col_id: i.into(),
                col_name: col.col_name,
                col_type: col.col_type,
                is_autoinc: col.is_autoinc,
            };
            let row = ProductValue::from(row);
            let data_key = row.to_data_key();
            {
                let st_columns =
                    self.committed_state
                        .get_or_create_table(ST_COLUMNS_ID, &ST_COLUMNS_ROW_TYPE, &st_columns_schema());
                st_columns.rows.insert(RowId(data_key), row);
            }

            // If any columns are auto incrementing, we need to create a sequence
            // NOTE: This code with the `seq_start` is particularly fragile.
            // TODO: If we exceed  `SEQUENCE_PREALLOCATION_AMOUNT` we will get a unique violation
            if let Some(col_name) = col_name_for_autoinc {
                // The database is bootstrapped with the total of `SystemTables::total_` that identify what is the start of the sequence
                let (seq_start, seq_id): (i128, SequenceId) = match schema.table_id {
                    ST_TABLES_ID => (SystemTables::total_tables() as i128, TABLE_ID_SEQUENCE_ID),
                    ST_INDEXES_ID => (
                        (SystemTables::total_indexes() + SystemTables::total_constraints_indexes()) as i128,
                        INDEX_ID_SEQUENCE_ID,
                    ),
                    ST_SEQUENCES_ID => (SystemTables::total_sequences() as i128, SEQUENCE_ID_SEQUENCE_ID),
                    ST_CONSTRAINTS_ID => (SystemTables::total_constraints() as i128, CONSTRAINT_ID_SEQUENCE_ID),
                    _ => unreachable!(),
                };
                let st_sequences = self.committed_state.get_or_create_table(
                    ST_SEQUENCES_ID,
                    &ST_SEQUENCE_ROW_TYPE,
                    &st_sequences_schema(),
                );
                let row = StSequenceRow {
                    sequence_id: seq_id,
                    sequence_name: format!("{}_seq", col_name),
                    table_id: col.table_id,
                    col_id: col.col_id,
                    increment: 1,
                    start: seq_start,
                    min_value: 1,
                    max_value: u32::MAX as i128,
                    allocated: SEQUENCE_PREALLOCATION_AMOUNT,
                };
                let row = ProductValue::from(row);
                let data_key = row.to_data_key();
                st_sequences.rows.insert(RowId(data_key), row);
            }
        }

        //Insert constraints into `st_constraints`
        let st_constraints = self.committed_state.get_or_create_table(
            ST_CONSTRAINTS_ID,
            &ST_CONSTRAINT_ROW_TYPE,
            &st_constraints_schema(),
        );

        let mut indexes = schema.indexes.clone();
        //TODO: The constraints are limited to 1 column until indexes are changed to deal with n-columns
        indexes.extend(schema.constraints.into_iter().map(|constraint| {
            assert_eq!(constraint.columns.len(), 1, "Constraints only supported for 1 column.");

            let index_name = format!("idx_{}", &constraint.constraint_name);

            let row = StConstraintRow {
                constraint_id: constraint.constraint_id,
                constraint_name: constraint.constraint_name.clone(),
                kind: constraint.kind,
                table_id,
                columns: constraint.columns,
            };
            let row = ProductValue::from(row);
            let data_key = row.to_data_key();
            st_constraints.rows.insert(RowId(data_key), row);

            //Check if add an index:
            match constraint.kind {
                x if x.is_unique() => IndexSchema {
                    index_id: constraint.constraint_id,
                    table_id,
                    cols: NonEmpty::new(first_col_id),
                    index_name,
                    is_unique: true,
                },
                x if x.is_indexed() => IndexSchema {
                    index_id: constraint.constraint_id,
                    table_id,
                    cols: NonEmpty::new(first_col_id),
                    index_name,
                    is_unique: false,
                },
                x => panic!("Adding constraint of kind `{x:?}` is not supported yet."),
            }
        }));

        // Insert the indexes into st_indexes
        let st_indexes =
            self.committed_state
                .get_or_create_table(ST_INDEXES_ID, &ST_INDEX_ROW_TYPE, &st_indexes_schema());
        for (_, index) in indexes.into_iter().enumerate() {
            let row = StIndexRow {
                index_id: index.index_id,
                table_id,
                cols: index.cols,
                index_name: index.index_name,
                is_unique: index.is_unique,
            };
            let row = ProductValue::from(row);
            let data_key = row.to_data_key();
            st_indexes.rows.insert(RowId(data_key), row);
        }

        Ok(())
    }

    fn build_sequence_state(&mut self) -> super::Result<()> {
        let st_sequences = self.committed_state.tables.get(&ST_SEQUENCES_ID).unwrap();
        for row in st_sequences.scan_rows() {
            let sequence = StSequenceRow::try_from(row)?;
            // TODO: The system tables have initialized their value already, but this is wrong:
            // If we exceed  `SEQUENCE_PREALLOCATION_AMOUNT` we will get a unique violation
            let is_system_table = self
                .committed_state
                .tables
                .get(&sequence.table_id)
                .map_or(false, |x| x.schema.table_type == StTableType::System);

            let schema = sequence.to_owned().into();

            let mut seq = Sequence::new(schema);
            // Now we need to recover the last allocation value.
            if !is_system_table && seq.value < sequence.allocated + 1 {
                seq.value = sequence.allocated + 1;
            }

            self.sequence_state.sequences.insert(sequence.sequence_id, seq);
        }
        Ok(())
    }

    fn build_indexes(&mut self) -> super::Result<()> {
        let st_indexes = self.committed_state.tables.get(&ST_INDEXES_ID).unwrap();
        let rows = st_indexes.scan_rows().cloned().collect::<Vec<_>>();
        for row in rows {
            let index_row = StIndexRow::try_from(&row)?;
            let table = self.committed_state.get_table(&index_row.table_id).unwrap();
            let mut index = BTreeIndex::new(
                index_row.index_id,
                index_row.table_id,
                index_row.cols.clone(),
                index_row.index_name.into(),
                index_row.is_unique,
            );
            index.build_from_rows(table.scan_rows())?;
            table.indexes.insert(index_row.cols, index);
        }
        Ok(())
    }

    /// After replaying all old transactions, tables which have rows will
    /// have been created in memory, but tables with no rows will not have
    /// been created. This function ensures that they are created.
    fn build_missing_tables(&mut self) -> super::Result<()> {
        let st_tables = self.committed_state.tables.get(&ST_TABLES_ID).unwrap();
        let rows = st_tables.scan_rows().cloned().collect::<Vec<_>>();
        for row in rows {
            let table_row = StTableRow::try_from(&row)?;
            let table_id = table_row.table_id;
            if self.committed_state.get_table(&table_id).is_none() {
                let schema = self.schema_for_table(table_id)?.into_owned();
                let row_type = self.row_type_for_table(table_id)?.into_owned();
                self.committed_state
                    .tables
                    .insert(table_id, Table::new(row_type, schema));
            }
        }
        Ok(())
    }

    fn drop_col_eq(&mut self, table_id: TableId, col_id: ColId, value: AlgebraicValue) -> super::Result<()> {
        let ctx = ExecutionContext::internal(self.database_address);
        let rows = self.iter_by_col_eq(&ctx, &table_id, col_id, value)?;
        let ids_to_delete = rows.map(|row| RowId(*row.id())).collect::<Vec<_>>();
        if ids_to_delete.is_empty() {
            return Err(TableError::IdNotFound(table_id).into());
        }
        self.delete(&table_id, ids_to_delete);
        Ok(())
    }

    fn drop_table_from_st_tables(&mut self, table_id: TableId) -> super::Result<()> {
        const ST_TABLES_TABLE_ID_COL: ColId = ColId(0);
        self.drop_col_eq(ST_TABLES_ID, ST_TABLES_TABLE_ID_COL, table_id.into())
    }

    fn drop_table_from_st_columns(&mut self, table_id: TableId) -> super::Result<()> {
        const ST_COLUMNS_TABLE_ID_COL: ColId = ColId(0);
        self.drop_col_eq(ST_COLUMNS_ID, ST_COLUMNS_TABLE_ID_COL, table_id.into())
    }

    #[tracing::instrument(skip_all)]
    fn get_next_sequence_value(&mut self, seq_id: SequenceId) -> super::Result<i128> {
        {
            let Some(sequence) = self.sequence_state.get_sequence_mut(seq_id) else {
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
        const ST_SEQUENCES_SEQUENCE_ID_COL: ColId = ColId(0);
        let ctx = ExecutionContext::internal(self.database_address);
        let old_seq_row = self
            .iter_by_col_eq(&ctx, &ST_SEQUENCES_ID, ST_SEQUENCES_SEQUENCE_ID_COL, seq_id.into())?
            .last()
            .unwrap()
            .data;
        let (seq_row, old_seq_row_id) = {
            let old_seq_row_id = RowId(old_seq_row.to_data_key());
            let mut seq_row = StSequenceRow::try_from(old_seq_row)?.to_owned();

            let Some(sequence) = self.sequence_state.get_sequence_mut(seq_id) else {
                return Err(SequenceError::NotFound(seq_id).into());
            };
            seq_row.allocated = sequence.nth_value(SEQUENCE_PREALLOCATION_AMOUNT as usize);
            sequence.set_allocation(seq_row.allocated);
            (seq_row, old_seq_row_id)
        };

        self.delete(&ST_SEQUENCES_ID, [old_seq_row_id]);
        self.insert(ST_SEQUENCES_ID, ProductValue::from(seq_row))?;

        let Some(sequence) = self.sequence_state.get_sequence_mut(seq_id) else {
            return Err(SequenceError::NotFound(seq_id).into());
        };
        if let Some(value) = sequence.gen_next_value() {
            return Ok(value);
        }
        Err(SequenceError::UnableToAllocate(seq_id).into())
    }

    fn create_sequence(&mut self, seq: SequenceDef) -> super::Result<SequenceId> {
        log::trace!(
            "SEQUENCE CREATING: {} for table: {} and col: {}",
            seq.sequence_name,
            seq.table_id,
            seq.col_id.0
        );

        // Insert the sequence row into st_sequences
        // NOTE: Because st_sequences has a unique index on sequence_name, this will
        // fail if the table already exists.
        let sequence_row = StSequenceRow {
            sequence_id: 0.into(), // autogen'd
            sequence_name: seq.sequence_name,
            table_id: seq.table_id,
            col_id: seq.col_id,
            allocated: seq.start.unwrap_or(1),
            increment: seq.increment,
            start: seq.start.unwrap_or(1),
            min_value: seq.min_value.unwrap_or(1),
            max_value: seq.max_value.unwrap_or(i128::MAX),
        };
        let row = sequence_row.into();
        let result = self.insert(ST_SEQUENCES_ID, row)?;
        // TODO(centril): `result` is already owned, so pass that in.
        let sequence_row = StSequenceRow::try_from(&result)?.to_owned();
        let sequence_id = sequence_row.sequence_id;

        let schema = sequence_row.into();
        self.sequence_state.sequences.insert(sequence_id, Sequence::new(schema));

        log::trace!("SEQUENCE CREATED: id = {}", sequence_id);

        Ok(sequence_id)
    }

    fn drop_sequence(&mut self, seq_id: SequenceId) -> super::Result<()> {
        const ST_SEQUENCES_SEQUENCE_ID_COL: ColId = ColId(0);
        let ctx = ExecutionContext::internal(self.database_address);
        let old_seq_row = self
            .iter_by_col_eq(&ctx, &ST_SEQUENCES_ID, ST_SEQUENCES_SEQUENCE_ID_COL, seq_id.into())?
            .last()
            .unwrap()
            .data;
        let old_seq_row_id = RowId(old_seq_row.to_data_key());
        self.delete(&ST_SEQUENCES_ID, [old_seq_row_id]);
        self.sequence_state.sequences.remove(&seq_id);
        Ok(())
    }

    fn sequence_id_from_name(&self, seq_name: &str) -> super::Result<Option<SequenceId>> {
        let seq_name_col: ColId = 1.into();
        self.iter_by_col_eq(
            &ExecutionContext::internal(self.database_address),
            &ST_SEQUENCES_ID,
            seq_name_col,
            AlgebraicValue::String(seq_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| SequenceId(*row.view().elements[0].as_u32().unwrap()))
        })
    }

    fn create_table(&mut self, table_schema: TableDef) -> super::Result<TableId> {
        let table_name = table_schema.table_name.as_str();
        log::trace!("TABLE CREATING: {table_name}");

        if table_name_is_system(table_name) {
            return Err(TableError::System(table_name.into()).into());
        }
        // Insert the table row into st_tables
        // NOTE: Because st_tables has a unique index on table_name, this will
        // fail if the table already exists.
        let row = StTableRow {
            table_id: 0.into(),
            table_name: table_schema.table_name.clone(),
            table_type: table_schema.table_type,
            table_access: table_schema.table_access,
        };
        let table_id = StTableRow::try_from(&self.insert(ST_TABLES_ID, row.into())?)?.table_id;

        let row_type = table_schema.get_row_type();

        // Insert the columns into st_columns
        for (i, col) in table_schema.columns.into_iter().enumerate() {
            let col_id = i.into();
            let col_name_for_autoinc = col.is_autoinc.then(|| col.col_name.clone());
            let row = StColumnRow {
                table_id,
                col_id,
                col_name: col.col_name,
                col_type: col.col_type,
                is_autoinc: col.is_autoinc,
            };
            self.insert(ST_COLUMNS_ID, row.into())?;

            // Insert create the sequence for the autoinc column
            if let Some(col_name) = col_name_for_autoinc {
                let sequence_def = SequenceDef {
                    sequence_name: format!("{}_{}_seq", table_name, col_name),
                    table_id,
                    col_id,
                    increment: 1,
                    start: Some(1),
                    min_value: Some(1),
                    max_value: None,
                };
                self.create_sequence(sequence_def)?;
            }
        }

        // Get the half formed schema
        let schema = self.schema_for_table(table_id)?.into_owned();

        // Create the in memory representation of the table
        // NOTE: This should be done before creating the indexes
        self.create_table_internal(table_id, row_type, schema)?;

        // Create the indexes for the table
        for mut index in table_schema.indexes {
            // NOTE: The below ensure, that when creating a table you can only
            // create indexes on the table you are creating.
            index.table_id = table_id;
            self.create_index(index)?;
        }

        log::trace!("TABLE CREATED: {table_name}, table_id:{table_id}");

        Ok(table_id)
    }

    fn create_table_internal(
        &mut self,
        table_id: TableId,
        row_type: ProductType,
        schema: TableSchema,
    ) -> super::Result<()> {
        self.tx_state
            .as_mut()
            .unwrap()
            .insert_tables
            .insert(table_id, Table::new(row_type, schema));
        Ok(())
    }

    fn row_type_for_table(&self, table_id: TableId) -> super::Result<Cow<'_, ProductType>> {
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
        let elements = match self.schema_for_table(table_id)? {
            Cow::Borrowed(table_schema) => table_schema
                .columns
                .iter()
                .map(|col| col.col_type.clone().into())
                .collect(),
            Cow::Owned(table_schema) => table_schema
                .columns
                .into_iter()
                .map(|col| col.col_type.into())
                .collect(),
        };
        Ok(Cow::Owned(ProductType { elements }))
    }

    #[tracing::instrument(skip_all)]
    fn schema_for_table(&self, table_id: TableId) -> super::Result<Cow<'_, TableSchema>> {
        if let Some(schema) = self.get_schema(&table_id) {
            return Ok(Cow::Borrowed(schema));
        }

        let ctx = ExecutionContext::internal(self.database_address);

        // Look up the table_name for the table in question.
        let table_id_col = NonEmpty::new(0.into());

        // TODO(george): As part of the bootstrapping process, we add a bunch of rows
        // and only at very end do we patch things up and create table metadata, indexes,
        // and so on. Early parts of that process insert rows, and need the schema to do
        // so. We can't just call `iter_by_col_range` here as that would attempt to use the
        // index which we haven't created yet. So instead we just manually Scan here.
        let value: AlgebraicValue = table_id.into();
        let rows = IterByColRange::Scan(ScanIterByColRange {
            range: value,
            cols: table_id_col,
            scan_iter: self.iter(&ctx, &ST_TABLES_ID)?,
        })
        .collect::<Vec<_>>();
        assert!(rows.len() <= 1, "Expected at most one row in st_tables for table_id");

        let row = rows.first().ok_or_else(|| TableError::IdNotFound(table_id))?;
        let el = StTableRow::try_from(row.view())?;
        let table_name = el.table_name.to_owned();
        let table_id = el.table_id;

        // Look up the columns for the table in question.
        let mut columns = Vec::new();
        const TABLE_ID_COL: ColId = ColId(0);
        for data_ref in self.iter_by_col_eq(&ctx, &ST_COLUMNS_ID, TABLE_ID_COL, table_id.into())? {
            let row = data_ref.view();

            let el = StColumnRow::try_from(row)?;
            let col_schema = ColumnSchema {
                table_id: el.table_id,
                col_id: el.col_id,
                col_name: el.col_name.into(),
                col_type: el.col_type,
                is_autoinc: el.is_autoinc,
            };
            columns.push(col_schema);
        }

        columns.sort_by_key(|col| col.col_id);

        // Look up the indexes for the table in question.
        let mut indexes = Vec::new();
        let table_id_col: ColId = 1.into();
        for data_ref in self.iter_by_col_eq(&ctx, &ST_INDEXES_ID, table_id_col, table_id.into())? {
            let row = data_ref.view();

            let el = StIndexRow::try_from(row)?;
            let index_schema = IndexSchema {
                table_id: el.table_id,
                cols: el.cols,
                index_name: el.index_name.into(),
                is_unique: el.is_unique,
                index_id: el.index_id,
            };
            indexes.push(index_schema);
        }

        Ok(Cow::Owned(TableSchema {
            columns,
            table_id,
            table_name,
            indexes,
            constraints: vec![],
            table_type: el.table_type,
            table_access: el.table_access,
        }))
    }

    fn drop_table(&mut self, table_id: TableId) -> super::Result<()> {
        let ctx = ExecutionContext::internal(self.database_address);
        // First drop the tables indexes.
        const ST_INDEXES_TABLE_ID_COL: ColId = ColId(1);
        let iter = self.iter_by_col_eq(&ctx, &ST_INDEXES_ID, ST_INDEXES_TABLE_ID_COL, table_id.into())?;
        iter.map(|row| StIndexRow::try_from(row.view()).map(|el| el.index_id))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .try_for_each(|id| self.drop_index(id))?;

        // Remove the table's sequences from st_sequences.
        const ST_SEQUENCES_TABLE_ID_COL: ColId = ColId(2);
        let iter = self.iter_by_col_eq(&ctx, &ST_SEQUENCES_ID, ST_SEQUENCES_TABLE_ID_COL, table_id.into())?;
        iter.map(|row| StSequenceRow::try_from(row.view()).map(|el| el.sequence_id))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .try_for_each(|id| self.drop_sequence(id))?;

        // Remove the table's columns from st_columns.
        self.drop_table_from_st_columns(table_id)?;

        // Remove the table from st_tables.
        self.drop_table_from_st_tables(table_id)?;

        // Delete the table and its rows and indexes from memory.
        // TODO: This needs to not remove it from the committed state, because it can still be rolled back.
        // We will have to store the deletion in the TxState and then apply it to the CommittedState in commit.
        self.committed_state.tables.remove(&table_id);
        Ok(())
    }

    fn rename_table(&mut self, table_id: TableId, new_name: &str) -> super::Result<()> {
        // Update the table's name in st_tables.
        const ST_TABLES_TABLE_ID_COL: ColId = ColId(0);
        let ctx = ExecutionContext::internal(self.database_address);
        let mut row_iter = self.iter_by_col_eq(&ctx, &ST_TABLES_ID, ST_TABLES_TABLE_ID_COL, table_id.into())?;

        let row = row_iter.next().ok_or_else(|| TableError::IdNotFound(table_id))?;
        let row_id = RowId(*row.id);
        let mut el = StTableRow::try_from(row.view())?;
        el.table_name = new_name;
        let new_row = el.to_owned().into();

        assert!(
            row_iter.next().is_none(),
            "Expected at most one row in st_tables for table_id"
        );

        // Note the borrow checker requires that we explictly drop the iterator.
        // That is, before we delete and insert the new row.
        // This is because datastore iterators write to the metric store when dropped.
        // Hence if we don't explicitly drop here,
        // there will be another immutable borrow of self after the two mutable borrows below.
        drop(row_iter);

        self.delete(&ST_TABLES_ID, [row_id]);
        self.insert(ST_TABLES_ID, new_row)?;
        Ok(())
    }

    fn table_id_from_name(&self, table_name: &str) -> super::Result<Option<TableId>> {
        let table_name_col: ColId = 1.into();
        self.iter_by_col_eq(
            &ExecutionContext::internal(self.database_address),
            &ST_TABLES_ID,
            table_name_col,
            AlgebraicValue::String(table_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| TableId(*row.view().elements[0].as_u32().unwrap()))
        })
    }

    fn table_name_from_id<'a>(&'a self, ctx: &'a ExecutionContext, table_id: TableId) -> super::Result<Option<&str>> {
        let table_id_col: ColId = 0.into();
        self.iter_by_col_eq(ctx, &ST_TABLES_ID, table_id_col, table_id.into())
            .map(|mut iter| {
                iter.next()
                    .map(|row| row.view().elements[1].as_string().unwrap().deref())
            })
    }

    fn create_index(&mut self, index: IndexDef) -> super::Result<IndexId> {
        log::trace!(
            "INDEX CREATING: {} for table: {} and col(s): {:?}",
            index.name,
            index.table_id,
            index.cols
        );

        // Insert the index row into st_indexes
        // NOTE: Because st_indexes has a unique index on index_name, this will
        // fail if the index already exists.
        let row = StIndexRow {
            index_id: 0.into(), // Autogen'd
            table_id: index.table_id,
            cols: index.cols.clone(),
            index_name: index.name.clone(),
            is_unique: index.is_unique,
        };
        let index_id = StIndexRow::try_from(&self.insert(ST_INDEXES_ID, row.into())?)?.index_id;

        // Create the index in memory
        if !self.table_exists(&index.table_id) {
            return Err(TableError::IdNotFound(index.table_id).into());
        }
        self.create_index_internal(index_id, index)?;

        log::trace!("INDEX CREATED: id = {}", index_id);
        Ok(index_id)
    }

    fn create_index_internal(&mut self, index_id: IndexId, index: IndexDef) -> super::Result<()> {
        let insert_table =
            if let Some(insert_table) = self.tx_state.as_mut().unwrap().get_insert_table_mut(&index.table_id) {
                insert_table
            } else {
                let row_type = self.row_type_for_table(index.table_id)?.into_owned();
                let schema = self.schema_for_table(index.table_id)?.into_owned();
                self.tx_state
                    .as_mut()
                    .unwrap()
                    .insert_tables
                    .insert(index.table_id, Table::new(row_type, schema));
                self.tx_state
                    .as_mut()
                    .unwrap()
                    .get_insert_table_mut(&index.table_id)
                    .unwrap()
            };

        let mut insert_index = BTreeIndex::new(
            index_id,
            index.table_id,
            index.cols.clone(),
            index.name.clone(),
            index.is_unique,
        );
        insert_index.build_from_rows(insert_table.scan_rows())?;

        // NOTE: Also add all the rows in the already committed table to the index.
        if let Some(committed_table) = self.committed_state.get_table(&index.table_id) {
            insert_index.build_from_rows(committed_table.scan_rows())?;
        }

        insert_table.schema.indexes.push(IndexSchema {
            table_id: index.table_id,
            cols: index.cols.clone(),
            index_name: index.name,
            is_unique: index.is_unique,
            index_id,
        });

        insert_table.indexes.insert(index.cols, insert_index);
        Ok(())
    }

    fn drop_index(&mut self, index_id: IndexId) -> super::Result<()> {
        log::trace!("INDEX DROPPING: {}", index_id.0);

        // Remove the index from st_indexes.
        const ST_INDEXES_INDEX_ID_COL: ColId = ColId(0);
        let ctx = ExecutionContext::internal(self.database_address);
        let old_index_row = self
            .iter_by_col_eq(&ctx, &ST_INDEXES_ID, ST_INDEXES_INDEX_ID_COL, index_id.into())?
            .last()
            .unwrap()
            .data;
        let old_index_row_id = RowId(old_index_row.to_data_key());
        self.delete(&ST_INDEXES_ID, [old_index_row_id]);

        self.drop_index_internal(&index_id);

        log::trace!("INDEX DROPPED: {}", index_id.0);
        Ok(())
    }

    fn drop_index_internal(&mut self, index_id: &IndexId) {
        for (_, table) in self.committed_state.tables.iter_mut() {
            let mut cols = vec![];
            for index in table.indexes.values_mut() {
                if index.index_id == *index_id {
                    cols.push(index.cols.clone());
                }
            }
            for col in cols {
                table.indexes.remove(&col);
                table.schema.indexes.retain(|x| x.cols != col);
            }
        }
        if let Some(insert_table) = self
            .tx_state
            .as_mut()
            .unwrap()
            .get_insert_table_mut(&TableId(index_id.0))
        {
            let mut cols = vec![];
            for index in insert_table.indexes.values_mut() {
                if index.index_id == *index_id {
                    cols.push(index.cols.clone());
                }
            }
            for col in cols {
                insert_table.indexes.remove(&col);
                insert_table.schema.indexes.retain(|x| x.cols != col);
            }
        }
    }

    fn index_id_from_name(&self, index_name: &str) -> super::Result<Option<IndexId>> {
        let index_name_col: ColId = 3.into();
        self.iter_by_col_eq(
            &ExecutionContext::internal(self.database_address),
            &ST_INDEXES_ID,
            index_name_col,
            AlgebraicValue::String(index_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| IndexId(*row.view().elements[0].as_u32().unwrap()))
        })
    }

    fn contains_row(&self, table_id: &TableId, row_id: &RowId) -> RowState<'_> {
        match self.tx_state.as_ref().unwrap().get_row_op(table_id, row_id) {
            RowState::Committed(_) => unreachable!("a row cannot be committed in a tx state"),
            RowState::Insert(pv) => return RowState::Insert(pv),
            RowState::Delete => return RowState::Delete,
            RowState::Absent => (),
        }
        match self
            .committed_state
            .tables
            .get(table_id)
            .and_then(|table| table.rows.get(row_id))
        {
            Some(pv) => RowState::Committed(pv),
            None => RowState::Absent,
        }
    }

    fn table_exists(&self, table_id: &TableId) -> bool {
        self.tx_state
            .as_ref()
            .map(|tx_state| tx_state.insert_tables.contains_key(table_id))
            .unwrap_or(false)
            || self.committed_state.tables.contains_key(table_id)
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
    fn insert(&mut self, table_id: TableId, mut row: ProductValue) -> super::Result<ProductValue> {
        // TODO: Excuting schema_for_table for every row insert is expensive.
        // We should store the schema in the [Table] struct instead.
        let schema = self.schema_for_table(table_id)?;

        let mut col_to_update = None;
        for col in &*schema.columns {
            if col.is_autoinc {
                if !row.elements[col.col_id.idx()].is_numeric_zero() {
                    continue;
                }
                let st_sequences_table_id_col = ColId(2);
                for seq_row in self.iter_by_col_eq(
                    &ExecutionContext::internal(self.database_address),
                    &ST_SEQUENCES_ID,
                    st_sequences_table_id_col,
                    table_id.into(),
                )? {
                    let seq_row = seq_row.view();
                    let seq_row = StSequenceRow::try_from(seq_row)?;
                    if seq_row.col_id != col.col_id {
                        continue;
                    }

                    col_to_update = Some((col.col_id, seq_row.sequence_id));
                    break;
                }
            }
        }

        if let Some((col_id, sequence_id)) = col_to_update {
            let col_idx = col_id.idx();
            let col = &schema.columns[col_idx];
            if !Self::algebraic_type_is_numeric(&col.col_type) {
                return Err(SequenceError::NotInteger {
                    col: format!("{}.{}", &schema.table_name, &col.col_name),
                    found: col.col_type.clone(),
                }
                .into());
            }
            // At this point, we know this will be essentially a cheap copy.
            let col_ty = col.col_type.clone();
            let seq_val = self.get_next_sequence_value(sequence_id)?;
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
        let insert_table = if let Some(table) = self.tx_state.as_ref().unwrap().get_insert_table(&table_id) {
            table
        } else {
            let Some(committed_table) = self.committed_state.tables.get(&table_id) else {
                return Err(TableError::IdNotFound(table_id).into());
            };
            let table = Table {
                row_type: committed_table.row_type.clone(),
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
            self.tx_state.as_mut().unwrap().insert_tables.insert(table_id, table);
            self.tx_state.as_ref().unwrap().get_insert_table(&table_id).unwrap()
        };

        // Check unique constraints
        for index in insert_table.indexes.values() {
            if index.violates_unique_constraint(&row) {
                let value = row.project_not_empty(&index.cols).unwrap();
                return Err(IndexError::UniqueConstraintViolation {
                    constraint_name: index.name.clone(),
                    table_name: insert_table.schema.table_name.clone(),
                    col_names: index
                        .cols
                        .iter()
                        .map(|&x| insert_table.schema.columns[x.idx()].col_name.clone())
                        .collect(),
                    value,
                }
                .into());
            }
        }
        if let Some(table) = self.committed_state.tables.get_mut(&table_id) {
            for index in table.indexes.values() {
                let value = index.get_fields(&row)?;
                let Some(violators) = index.get_rows_that_violate_unique_constraint(&value) else {
                    continue;
                };
                for row_id in violators {
                    if let Some(delete_table) = self.tx_state.as_ref().unwrap().delete_tables.get(&table_id) {
                        if !delete_table.contains(row_id) {
                            let value = row.project_not_empty(&index.cols)?;
                            return Err(IndexError::UniqueConstraintViolation {
                                constraint_name: index.name.clone(),
                                table_name: table.schema.table_name.clone(),
                                col_names: index
                                    .cols
                                    .iter()
                                    .map(|&x| insert_table.schema.columns[x.idx()].col_name.clone())
                                    .collect(),
                                value,
                            }
                            .into());
                        }
                    } else {
                        let value = row.project_not_empty(&index.cols)?;
                        return Err(IndexError::UniqueConstraintViolation {
                            constraint_name: index.name.clone(),
                            table_name: table.schema.table_name.clone(),
                            col_names: index
                                .cols
                                .iter()
                                .map(|&x| insert_table.schema.columns[x.idx()].col_name.clone())
                                .collect(),
                            value,
                        }
                        .into());
                    }
                }
            }
        }

        // Now that we have checked all the constraints, we can perform the actual insertion.
        {
            let tx_state = self.tx_state.as_mut().unwrap();

            // We have a few cases to consider, based on the history of this transaction, and
            // whether the row was already present or not at the start of this transaction.
            // 1. If the row was not originally present, and therefore also not deleted by
            //    this transaction, we will add it to `insert_tables`.
            // 2. If the row was originally present, but not deleted by this transaction,
            //    we should fail, as we would otherwise violate set semantics.
            // 3. If the row was originally present, and is currently going to be deleted
            //    by this transaction, we will remove it from `delete_tables`, and the
            //    cummulative effect will be to leave the row in place in the committed state.

            let delete_table = tx_state.get_or_create_delete_table(table_id);
            let row_was_previously_deleted = delete_table.remove(&row_id);

            // If the row was just deleted in this transaction and we are re-inserting it now,
            // we're done. Otherwise we have to add the row to the insert table, and into our memory.
            if row_was_previously_deleted {
                return Ok(());
            }

            let insert_table = tx_state.get_insert_table_mut(&table_id).unwrap();

            // TODO(cloutiertyler): should probably also check that all the columns are correct? Perf considerations.
            if insert_table.row_type.elements.len() != row.elements.len() {
                return Err(TableError::RowInvalidType { table_id, row }.into());
            }

            insert_table.insert(row_id, row);

            match data_key {
                DataKey::Data(_) => (),
                DataKey::Hash(_) => {
                    self.memory.insert(data_key, Arc::new(bytes));
                }
            };
        }

        Ok(())
    }

    fn get<'a>(&'a self, table_id: &TableId, row_id: &'a RowId) -> super::Result<Option<DataRef<'a>>> {
        if !self.table_exists(table_id) {
            return Err(TableError::IdNotFound(*table_id).into());
        }
        match self.tx_state.as_ref().unwrap().get_row_op(table_id, row_id) {
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
            .committed_state
            .tables
            .get(table_id)
            .and_then(|table| table.get_row(row_id))
            .map(|row| DataRef::new(row_id, row)))
    }

    fn get_row_type(&self, table_id: &TableId) -> Option<&ProductType> {
        if let Some(row_type) = self
            .tx_state
            .as_ref()
            .and_then(|tx_state| tx_state.insert_tables.get(table_id))
            .map(|table| table.get_row_type())
        {
            return Some(row_type);
        }
        self.committed_state
            .tables
            .get(table_id)
            .map(|table| table.get_row_type())
    }

    fn get_schema(&self, table_id: &TableId) -> Option<&TableSchema> {
        if let Some(schema) = self
            .tx_state
            .as_ref()
            .and_then(|tx_state| tx_state.insert_tables.get(table_id))
            .map(|table| table.get_schema())
        {
            return Some(schema);
        }
        self.committed_state
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
                self.tx_state
                    .as_mut()
                    .unwrap()
                    .get_or_create_delete_table(*table_id)
                    .insert(*row_id);
                // True because we did delete the row.
                true
            }
            RowState::Insert(_) => {
                // If the row is present because of a an insertion in this transaction,
                // we need to remove it from the appropriate insert_table.
                let insert_table = self.tx_state.as_mut().unwrap().get_insert_table_mut(table_id).unwrap();
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
            return Ok(Iter::new(ctx, *table_id, self));
        }
        Err(TableError::IdNotFound(*table_id).into())
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `col_id` equates to `value`.
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
    /// where the values of `col_id` are contained in `range`.
    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: NonEmpty<ColId>,
        range: R,
    ) -> super::Result<IterByColRange<'a, R>> {
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
        if let Some(inserted_rows) = self
            .tx_state
            .as_ref()
            .and_then(|tx_state| tx_state.index_seek(table_id, &cols, &range))
        {
            // The current transaction has modified this table, and the table is indexed.
            let tx_state = self.tx_state.as_ref().unwrap();
            Ok(IterByColRange::Index(IndexSeekIterInner {
                table_id: *table_id,
                tx_state,
                inserted_rows,
                committed_rows: self.committed_state.index_seek(table_id, &cols, &range),
                committed_state: &self.committed_state,
            }))
        } else {
            // Either the current transaction has not modified this table, or the table is not
            // indexed.
            match self.committed_state.index_seek(table_id, &cols, &range) {
                //If we don't have `self.tx_state` yet is likely we are running the bootstrap process
                Some(committed_rows) => match self.tx_state.as_ref() {
                    None => Ok(IterByColRange::Scan(ScanIterByColRange {
                        range,
                        cols,
                        scan_iter: self.iter(ctx, table_id)?,
                    })),
                    Some(tx_state) => Ok(IterByColRange::CommittedIndex(CommittedIndexIter {
                        table_id: *table_id,
                        tx_state,
                        committed_state: &self.committed_state,
                        committed_rows,
                    })),
                },
                None => Ok(IterByColRange::Scan(ScanIterByColRange {
                    range,
                    cols,
                    scan_iter: self.iter(ctx, table_id)?,
                })),
            }
        }
    }

    fn commit(&mut self) -> super::Result<Option<TxData>> {
        let tx_state = self.tx_state.take().unwrap();
        let memory = std::mem::take(&mut self.memory);
        let tx_data = self.committed_state.merge(tx_state, memory);
        Ok(Some(tx_data))
    }

    fn rollback(&mut self) {
        self.tx_state = None;
        // TODO: Check that no sequences exceed their allocation after the rollback.
    }
}

#[derive(Clone)]
pub struct Locking {
    inner: Arc<Mutex<Inner>>,
}

impl Locking {
    /// IMPORTANT! This the most delicate function in the entire codebase.
    /// DO NOT CHANGE UNLESS YOU KNOW WHAT YOU'RE DOING!!!
    pub fn bootstrap(database_address: Address) -> Result<Self, DBError> {
        log::trace!("DATABASE: BOOTSTRAPPING SYSTEM TABLES...");

        // NOTE! The bootstrapping process does not take plan in a transaction.
        // This is intentional.
        let mut datastore = Inner::new(database_address);

        // TODO(cloutiertyler): One thing to consider in the future is, should
        // we persist the bootstrap transaction in the message log? My intuition
        // is no, because then if we change the schema of the system tables we
        // would need to migrate that data, whereas since the tables are defined
        // in the code we don't have that issue. We may have other issues though
        // for code that relies on the old schema...

        // Create the system tables and insert information about themselves into
        // st_table, st_columns, st_indexes, and st_sequences.
        datastore.bootstrap_system_table(st_table_schema())?;
        datastore.bootstrap_system_table(st_columns_schema())?;
        datastore.bootstrap_system_table(st_constraints_schema())?;
        datastore.bootstrap_system_table(st_indexes_schema())?;
        datastore.bootstrap_system_table(st_sequences_schema())?;
        // TODO(kim): We need to make sure to have ST_MODULE in the committed
        // state. `bootstrap_system_table` initializes the others lazily, but
        // it doesn't know about `ST_MODULE_ROW_TYPE`. Perhaps the committed
        // state should be initialized eagerly here?
        datastore
            .committed_state
            .get_or_create_table(ST_MODULE_ID, &ST_MODULE_ROW_TYPE, &st_module_schema());
        datastore.bootstrap_system_table(st_module_schema())?;

        // The database tables are now initialized with the correct data.
        // Now we have to build our in memory structures.
        datastore.build_sequence_state()?;
        datastore.build_indexes()?;

        log::trace!("DATABASE:BOOTSTRAPPING SYSTEM TABLES DONE");

        Ok(Locking {
            inner: Arc::new(Mutex::new(datastore)),
        })
    }

    /// The purpose of this is to rebuild the state of the datastore
    /// after having inserted all of rows from the message log.
    /// This is necessary because, for example, inserting a row into `st_table`
    /// is not equivalent to calling `create_table`.
    /// There may eventually be better way to do this, but this will have to do for now.
    pub fn rebuild_state_after_replay(&self) -> Result<(), DBError> {
        let mut inner = self.inner.lock();

        // `build_missing_tables` must be called before indexes.
        // Honestly this should maybe just be one big procedure.
        // See John Carmack's philosophy on this.
        inner.build_missing_tables()?;
        inner.build_indexes()?;
        inner.build_sequence_state()?;

        Ok(())
    }

    fn table_rows(
        inner: &mut Inner,
        table_id: TableId,
        schema: TableSchema,
        row_type: ProductType,
    ) -> &mut indexmap::IndexMap<RowId, ProductValue> {
        &mut inner
            .committed_state
            .tables
            .entry(table_id)
            .or_insert_with(|| Table::new(row_type, schema))
            .rows
    }

    pub fn replay_transaction(
        &self,
        transaction: &Transaction,
        odb: Arc<std::sync::Mutex<Box<dyn ObjectDB + Send>>>,
    ) -> Result<(), DBError> {
        let mut inner = self.inner.lock();
        for write in &transaction.writes {
            let table_id = TableId(write.set_id);
            let schema = inner.schema_for_table(table_id)?.into_owned();
            let row_type = inner.row_type_for_table(table_id)?.into_owned();
            match write.operation {
                Operation::Delete => {
                    Self::table_rows(&mut inner, table_id, schema, row_type).remove(&RowId(write.data_key));
                }
                Operation::Insert => {
                    let product_value = match write.data_key {
                        DataKey::Data(data) => ProductValue::decode(&row_type, &mut &data[..]).unwrap_or_else(|_| {
                            panic!("Couldn't decode product value to {:?} from message log", row_type)
                        }),
                        DataKey::Hash(hash) => {
                            let data = odb.lock().unwrap().get(hash).unwrap_or_else(|| {
                                panic!("Object {hash} referenced from transaction not present in object DB");
                            });
                            ProductValue::decode(&row_type, &mut &data[..]).unwrap_or_else(|_| {
                                panic!("Couldn't decode product value to {:?} from message log", row_type)
                            })
                        }
                    };
                    Self::table_rows(&mut inner, table_id, schema, row_type)
                        .insert(RowId(write.data_key), product_value);
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

    fn view_product_value<'a>(&self, data_ref: Self::DataRef<'a>) -> &'a ProductValue {
        data_ref.data
    }
}

impl traits::Tx for Locking {
    type TxId = MutTxId;

    fn begin_tx(&self) -> Self::TxId {
        self.begin_mut_tx()
    }

    fn release_tx(&self, ctx: &ExecutionContext, tx: Self::TxId) {
        self.rollback_mut_tx(ctx, tx)
    }
}

pub struct Iter<'a> {
    ctx: &'a ExecutionContext<'a>,
    table_id: TableId,
    inner: &'a Inner,
    stage: ScanStage<'a>,
    committed_rows_fetched: u64,
}

impl Drop for Iter<'_> {
    fn drop(&mut self) {
        DB_METRICS
            .rdb_num_rows_fetched
            .with_label_values(
                &self.ctx.txn_type(),
                &self.ctx.database(),
                self.ctx.reducer_name().unwrap_or_default(),
                &self.table_id.into(),
            )
            .inc_by(self.committed_rows_fetched);
    }
}

impl<'a> Iter<'a> {
    fn new(ctx: &'a ExecutionContext, table_id: TableId, inner: &'a Inner) -> Self {
        Self {
            ctx,
            table_id,
            inner,
            stage: ScanStage::Start,
            committed_rows_fetched: 0,
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
            let table = this.inner.tx_state.as_ref()?;
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
                    if let Some(table) = self.inner.committed_state.tables.get(&table_id) {
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
                        self.committed_rows_fetched += 1;
                        // Check the committed row's state in the current tx.
                        match self.inner.tx_state.as_ref().map(|tx_state| tx_state.get_row_op(&table_id, row_id)) {
                            Some(RowState::Committed(_)) => unreachable!("a row cannot be committed in a tx state"),
                            // Do nothing, via (3), we'll get it in the next stage (2).
                            Some(RowState::Insert(_)) |
                            // Skip it, it's been deleted.
                            Some(RowState::Delete) => {}
                            // There either are no state changes for the current tx (`None`),
                            // or there are, but `row_id` specifically has not been changed.
                            // Either way, the row is in the committed state
                            // and hasn't been removed in the current tx,
                            // so it exists and can be returned.
                            Some(RowState::Absent) | None => return Some(DataRef::new(row_id, row)),
                        }
                    }
                    // (3) We got here, so we must've exhausted the committed changes.
                    // Start looking in the current tx for inserts, if any, in (2).
                    maybe_stage_current_tx_inserts(self)?;
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

pub struct IndexSeekIterInner<'a> {
    table_id: TableId,
    tx_state: &'a TxState,
    committed_state: &'a CommittedState,
    inserted_rows: BTreeIndexRangeIter<'a>,
    committed_rows: Option<BTreeIndexRangeIter<'a>>,
}

impl<'a> Iterator for IndexSeekIterInner<'a> {
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
            return Some(get_committed_row(self.committed_state, &self.table_id, row_id));
        }

        None
    }
}

pub struct CommittedIndexIter<'a> {
    table_id: TableId,
    tx_state: &'a TxState,
    committed_state: &'a CommittedState,
    committed_rows: BTreeIndexRangeIter<'a>,
}

impl<'a> Iterator for CommittedIndexIter<'a> {
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row_id) = self.committed_rows.find(|row_id| {
            !self
                .tx_state
                .delete_tables
                .get(&self.table_id)
                .map_or(false, |table| table.contains(row_id))
        }) {
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
    Index(IndexSeekIterInner<'a>),

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
        tx: &'a Self::TxId,
        table_id: TableId,
    ) -> super::Result<Self::Iter<'a>> {
        self.iter_mut_tx(ctx, tx, table_id)
    }

    fn iter_by_col_range_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::TxId,
        table_id: TableId,
        cols: NonEmpty<ColId>,
        range: R,
    ) -> super::Result<Self::IterByColRange<'a, R>> {
        self.iter_by_col_range_mut_tx(ctx, tx, table_id, cols, range)
    }

    fn iter_by_col_eq_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::TxId,
        table_id: TableId,
        cols: NonEmpty<ColId>,
        value: AlgebraicValue,
    ) -> super::Result<Self::IterByColEq<'a>> {
        self.iter_by_col_eq_mut_tx(ctx, tx, table_id, cols, value)
    }

    fn get_tx<'a>(
        &self,
        tx: &'a Self::TxId,
        table_id: TableId,
        row_id: &'a Self::RowId,
    ) -> super::Result<Option<Self::DataRef<'a>>> {
        self.get_mut_tx(tx, table_id, row_id)
    }
}

impl traits::MutTx for Locking {
    type MutTxId = MutTxId;

    fn begin_mut_tx(&self) -> Self::MutTxId {
        let timer = Instant::now();
        let mut inner = self.inner.lock_arc();
        let lock_wait_time = timer.elapsed();
        if inner.tx_state.is_some() {
            panic!("The previous transaction was not properly rolled back or committed.");
        }
        inner.tx_state = Some(TxState::new());
        MutTxId {
            lock: inner,
            lock_wait_time,
            timer,
        }
    }

    fn rollback_mut_tx(&self, ctx: &ExecutionContext, mut tx: Self::MutTxId) {
        let elapsed_time = tx.timer.elapsed();
        let cpu_time = elapsed_time - tx.lock_wait_time;
        DB_METRICS
            .rdb_num_txns_rolledback
            .with_label_values(&ctx.txn_type(), &ctx.database(), ctx.reducer_name().unwrap_or(""))
            .inc();
        DB_METRICS
            .rdb_txn_cpu_time_ns
            .with_label_values(&ctx.txn_type(), &ctx.database(), ctx.reducer_name().unwrap_or(""))
            .observe(cpu_time.as_nanos() as f64);
        DB_METRICS
            .rdb_txn_elapsed_time_ns
            .with_label_values(&ctx.txn_type(), &ctx.database(), ctx.reducer_name().unwrap_or(""))
            .observe(elapsed_time.as_nanos() as f64);
        tx.lock.rollback();
    }

    fn commit_mut_tx(&self, ctx: &ExecutionContext, mut tx: Self::MutTxId) -> super::Result<Option<TxData>> {
        let elapsed_time = tx.timer.elapsed();
        let cpu_time = elapsed_time - tx.lock_wait_time;
        // Note, we record empty transactions in our metrics.
        // That is, transactions that don't write any rows to the commit log.
        DB_METRICS
            .rdb_num_txns_committed
            .with_label_values(&ctx.txn_type(), &ctx.database(), ctx.reducer_name().unwrap_or(""))
            .inc();
        DB_METRICS
            .rdb_txn_cpu_time_ns
            .with_label_values(&ctx.txn_type(), &ctx.database(), ctx.reducer_name().unwrap_or(""))
            .observe(cpu_time.as_nanos() as f64);
        DB_METRICS
            .rdb_txn_elapsed_time_ns
            .with_label_values(&ctx.txn_type(), &ctx.database(), ctx.reducer_name().unwrap_or(""))
            .observe(elapsed_time.as_nanos() as f64);
        tx.lock.commit()
    }

    #[cfg(test)]
    fn rollback_mut_tx_for_test(&self, mut tx: Self::MutTxId) {
        tx.lock.rollback();
    }

    #[cfg(test)]
    fn commit_mut_tx_for_test(&self, mut tx: Self::MutTxId) -> super::Result<Option<TxData>> {
        tx.lock.commit()
    }
}

impl MutTxDatastore for Locking {
    fn create_table_mut_tx(&self, tx: &mut Self::MutTxId, schema: TableDef) -> super::Result<TableId> {
        tx.lock.create_table(schema)
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
    /// reflect the schema of the table.
    ///
    /// This function is known to be called quite frequently.
    fn row_type_for_table_mut_tx<'tx>(
        &self,
        tx: &'tx Self::MutTxId,
        table_id: TableId,
    ) -> super::Result<Cow<'tx, ProductType>> {
        tx.lock.row_type_for_table(table_id)
    }

    /// IMPORTANT! This function is relatively expensive, and much more
    /// expensive than `row_type_for_table_mut_tx`.  Prefer
    /// `row_type_for_table_mut_tx` if you only need to access the `ProductType`
    /// of the table.
    fn schema_for_table_mut_tx<'tx>(
        &self,
        tx: &'tx Self::MutTxId,
        table_id: TableId,
    ) -> super::Result<Cow<'tx, TableSchema>> {
        tx.lock.schema_for_table(table_id)
    }

    /// This function is relatively expensive because it needs to be
    /// transactional, however we don't expect to be dropping tables very often.
    fn drop_table_mut_tx(&self, tx: &mut Self::MutTxId, table_id: TableId) -> super::Result<()> {
        tx.lock.drop_table(table_id)
    }

    fn rename_table_mut_tx(&self, tx: &mut Self::MutTxId, table_id: TableId, new_name: &str) -> super::Result<()> {
        tx.lock.rename_table(table_id, new_name)
    }

    fn table_id_exists(&self, tx: &Self::MutTxId, table_id: &TableId) -> bool {
        tx.lock.table_exists(table_id)
    }

    fn table_id_from_name_mut_tx(&self, tx: &Self::MutTxId, table_name: &str) -> super::Result<Option<TableId>> {
        tx.lock.table_id_from_name(table_name)
    }

    fn table_name_from_id_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTxId,
        table_id: TableId,
    ) -> super::Result<Option<&'a str>> {
        tx.lock.table_name_from_id(ctx, table_id)
    }

    fn create_index_mut_tx(&self, tx: &mut Self::MutTxId, index: IndexDef) -> super::Result<IndexId> {
        tx.lock.create_index(index)
    }

    fn drop_index_mut_tx(&self, tx: &mut Self::MutTxId, index_id: IndexId) -> super::Result<()> {
        tx.lock.drop_index(index_id)
    }

    fn index_id_from_name_mut_tx(&self, tx: &Self::MutTxId, index_name: &str) -> super::Result<Option<IndexId>> {
        tx.lock.index_id_from_name(index_name)
    }

    fn get_next_sequence_value_mut_tx(&self, tx: &mut Self::MutTxId, seq_id: SequenceId) -> super::Result<i128> {
        tx.lock.get_next_sequence_value(seq_id)
    }

    fn create_sequence_mut_tx(&self, tx: &mut Self::MutTxId, seq: SequenceDef) -> super::Result<SequenceId> {
        tx.lock.create_sequence(seq)
    }

    fn drop_sequence_mut_tx(&self, tx: &mut Self::MutTxId, seq_id: SequenceId) -> super::Result<()> {
        tx.lock.drop_sequence(seq_id)
    }

    fn sequence_id_from_name_mut_tx(
        &self,
        tx: &Self::MutTxId,
        sequence_name: &str,
    ) -> super::Result<Option<SequenceId>> {
        tx.lock.sequence_id_from_name(sequence_name)
    }

    fn iter_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTxId,
        table_id: TableId,
    ) -> super::Result<Self::Iter<'a>> {
        tx.lock.iter(ctx, &table_id)
    }

    fn iter_by_col_range_mut_tx<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        cols: impl Into<NonEmpty<ColId>>,
        range: R,
    ) -> super::Result<Self::IterByColRange<'a, R>> {
        tx.lock.iter_by_col_range(ctx, &table_id, cols.into(), range)
    }

    fn iter_by_col_eq_mut_tx<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        cols: impl Into<NonEmpty<ColId>>,
        value: AlgebraicValue,
    ) -> super::Result<Self::IterByColEq<'a>> {
        tx.lock.iter_by_col_eq(ctx, &table_id, cols, value)
    }

    fn get_mut_tx<'a>(
        &self,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        row_id: &'a Self::RowId,
    ) -> super::Result<Option<Self::DataRef<'a>>> {
        tx.lock.get(&table_id, row_id)
    }

    fn delete_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTxId,
        table_id: TableId,
        row_ids: impl IntoIterator<Item = Self::RowId>,
    ) -> u32 {
        tx.lock.delete(&table_id, row_ids)
    }

    fn delete_by_rel_mut_tx(
        &self,
        tx: &mut Self::MutTxId,
        table_id: TableId,
        relation: impl IntoIterator<Item = ProductValue>,
    ) -> u32 {
        tx.lock.delete_by_rel(&table_id, relation)
    }

    fn insert_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTxId,
        table_id: TableId,
        row: ProductValue,
    ) -> super::Result<ProductValue> {
        tx.lock.insert(table_id, row)
    }
}

impl traits::Programmable for Locking {
    fn program_hash(&self, tx: &MutTxId) -> Result<Option<Hash>, DBError> {
        match tx
            .lock
            .iter(&ExecutionContext::internal(tx.lock.database_address), &ST_MODULE_ID)?
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
        let ctx = ExecutionContext::internal(tx.lock.database_address);
        let mut iter = tx.lock.iter(&ctx, &ST_MODULE_ID)?;
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

            tx.lock.delete_by_rel(&ST_MODULE_ID, Some(ProductValue::from(&row)));
            tx.lock.insert(
                ST_MODULE_ID,
                ProductValue::from(&StModuleRow {
                    program_hash: hash,
                    kind: WASM_MODULE,
                    epoch: system_tables::Epoch(fence),
                }),
            )?;
            return Ok(());
        }

        // Note the borrow checker requires that we explictly drop the iterator before we insert.
        // This is because datastore iterators write to the metric store when dropped.
        // Hence if we don't explicitly drop here,
        // there will be another immutable borrow of self after the mutable borrow of the insert.
        drop(iter);

        tx.lock.insert(
            ST_MODULE_ID,
            ProductValue::from(&StModuleRow {
                program_hash: hash,
                kind: WASM_MODULE,
                epoch: system_tables::Epoch(fence),
            }),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ColId, Locking, MutTxId, StTableRow};
    use crate::db::datastore::system_tables::{StConstraintRow, ST_CONSTRAINTS_ID};
    use crate::execution_context::ExecutionContext;
    use crate::{
        db::datastore::{
            locking_tx_datastore::{
                StColumnRow, StIndexRow, StSequenceRow, ST_COLUMNS_ID, ST_INDEXES_ID, ST_SEQUENCES_ID, ST_TABLES_ID,
            },
            traits::{ColumnDef, ColumnSchema, IndexDef, IndexSchema, MutTx, MutTxDatastore, TableDef, TableSchema},
        },
        error::{DBError, IndexError},
    };
    use itertools::Itertools;
    use nonempty::NonEmpty;
    use spacetimedb_lib::Address;
    use spacetimedb_lib::{
        auth::{StAccess, StTableType},
        error::ResultTest,
        ColumnIndexAttribute,
    };
    use spacetimedb_primitives::{IndexId, TableId};
    use spacetimedb_sats::{product, AlgebraicType, AlgebraicValue, ProductValue};

    fn u32_str_u32(a: u32, b: &str, c: u32) -> ProductValue {
        product![a, b, c]
    }

    fn get_datastore() -> super::super::Result<Locking> {
        Locking::bootstrap(Address::zero())
    }

    fn index_row(index_id: u32, table_id: u32, col_id: u32, name: &str, is_unique: bool) -> StIndexRow<String> {
        StIndexRow {
            index_id: IndexId(index_id),
            table_id: TableId(table_id),
            cols: NonEmpty::new(ColId(col_id)),
            index_name: name.into(),
            is_unique,
        }
    }

    fn table_row(
        table_id: u32,
        table_name: &str,
        table_type: StTableType,
        table_access: StAccess,
    ) -> StTableRow<String> {
        StTableRow {
            table_id: TableId(table_id),
            table_name: table_name.into(),
            table_type,
            table_access,
        }
    }

    fn column_row(
        table_id: u32,
        col_id: u32,
        col_name: &str,
        col_type: AlgebraicType,
        is_autoinc: bool,
    ) -> StColumnRow<String> {
        StColumnRow {
            table_id: TableId(table_id),
            col_id: ColId(col_id),
            col_name: col_name.into(),
            col_type,
            is_autoinc,
        }
    }

    fn column_schema(table_id: u32, id: u32, name: &str, ty: AlgebraicType, is_autoinc: bool) -> ColumnSchema {
        ColumnSchema {
            table_id: TableId(table_id),
            col_id: ColId(id),
            col_name: name.to_string(),
            col_type: ty,
            is_autoinc,
        }
    }

    fn index_schema(id: u32, table_id: u32, col_id: u32, name: &str, is_unique: bool) -> IndexSchema {
        IndexSchema {
            index_id: IndexId(id),
            table_id: TableId(table_id),
            cols: NonEmpty::new(ColId(col_id)),
            index_name: name.to_string(),
            is_unique,
        }
    }

    fn basic_table_schema() -> TableDef {
        TableDef {
            table_name: "Foo".into(),
            columns: vec![
                ColumnDef {
                    col_name: "id".into(),
                    col_type: AlgebraicType::U32,
                    is_autoinc: true,
                },
                ColumnDef {
                    col_name: "name".into(),
                    col_type: AlgebraicType::String,
                    is_autoinc: false,
                },
                ColumnDef {
                    col_name: "age".into(),
                    col_type: AlgebraicType::U32,
                    is_autoinc: false,
                },
            ],
            indexes: vec![
                IndexDef::new(
                    "id_idx".into(),
                    0.into(), // Ignored
                    0.into(),
                    true,
                ),
                IndexDef::new(
                    "name_idx".into(),
                    0.into(), // Ignored
                    1.into(),
                    true,
                ),
            ],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }
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

    #[test]
    fn test_bootstrapping_sets_up_tables() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let tx = datastore.begin_mut_tx();
        let table_rows = datastore
            .iter_mut_tx(&ExecutionContext::default(), &tx, ST_TABLES_ID)?
            .map(|x| StTableRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.table_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            table_rows,
            vec![
                // table_id, table_name, table_type, table_access
                table_row(0, "st_table", StTableType::System, StAccess::Public),
                table_row(1, "st_columns", StTableType::System, StAccess::Public),
                table_row(2, "st_sequence", StTableType::System, StAccess::Public),
                table_row(3, "st_indexes", StTableType::System, StAccess::Public),
                table_row(4, "st_constraints", StTableType::System, StAccess::Public),
                table_row(5, "st_module", StTableType::System, StAccess::Public),
            ]
        );
        let column_rows = datastore
            .iter_mut_tx(&ExecutionContext::default(), &tx, ST_COLUMNS_ID)?
            .map(|x| StColumnRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| (x.table_id, x.col_id))
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            column_rows,
            vec![
                // table_id, col_id, col_name, col_type, is_autoinc
                column_row(0, 0, "table_id", AlgebraicType::U32, true),
                column_row(0, 1, "table_name", AlgebraicType::String, false),
                column_row(0, 2, "table_type", AlgebraicType::String, false),
                column_row(0, 3, "table_access", AlgebraicType::String, false),

                column_row(1, 0, "table_id", AlgebraicType::U32, false),
                column_row(1, 1, "col_id", AlgebraicType::U32, false),
                column_row(1, 2, "col_type", AlgebraicType::bytes(), false),
                column_row(1, 3, "col_name", AlgebraicType::String, false),
                column_row(1, 4, "is_autoinc", AlgebraicType::Bool, false),

                column_row(2, 0, "sequence_id", AlgebraicType::U32, true),
                column_row(2, 1, "sequence_name", AlgebraicType::String, false),
                column_row(2, 2, "table_id", AlgebraicType::U32, false),
                column_row(2, 3, "col_id", AlgebraicType::U32, false),
                column_row(2, 4, "increment", AlgebraicType::I128, false),
                column_row(2, 5, "start", AlgebraicType::I128, false),
                column_row(2, 6, "min_value", AlgebraicType::I128, false),
                column_row(2, 7, "max_value", AlgebraicType::I128, false),
                column_row(2, 8, "allocated", AlgebraicType::I128, false),

                column_row(3, 0, "index_id", AlgebraicType::U32, true),
                column_row(3, 1, "table_id", AlgebraicType::U32, false),
                column_row(3, 2, "cols", AlgebraicType::array(AlgebraicType::U32), false),
                column_row(3, 3, "index_name", AlgebraicType::String, false),
                column_row(3, 4, "is_unique", AlgebraicType::Bool, false),

                column_row(4, 0, "constraint_id", AlgebraicType::U32, true),
                column_row(4, 1, "constraint_name", AlgebraicType::String, false),
                column_row(4, 2, "kind", AlgebraicType::U32, false),
                column_row(4, 3, "table_id", AlgebraicType::U32, false),
                column_row(4, 4, "columns", AlgebraicType::array(AlgebraicType::U32), false),

                column_row(5, 0, "program_hash", AlgebraicType::array(AlgebraicType::U8), false),
                column_row(5, 1, "kind", AlgebraicType::U8, false),
                column_row(5, 2, "epoch", AlgebraicType::U128, false),
            ]
        );
        let index_rows = datastore
            .iter_mut_tx(&ExecutionContext::default(), &tx, ST_INDEXES_ID)?
            .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.index_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            index_rows,
            vec![
                // index_id, table_id, col_id, index_name, is_unique
                index_row(0, 0, 0, "table_id_idx", true),
                index_row(1, 3, 0, "index_id_idx", true),
                index_row(2, 2, 0, "sequences_id_idx", true),
                index_row(3, 0, 1, "table_name_idx", true),
                index_row(4, 4, 0, "constraint_id_idx", true),
                index_row(5, 1, 0, "idx_ct_columns_table_id", false),
            ]
        );
        let sequence_rows = datastore
            .iter_mut_tx(&ExecutionContext::default(), &tx, ST_SEQUENCES_ID)?
            .map(|x| StSequenceRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.sequence_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            sequence_rows,
            vec![
                StSequenceRow { sequence_id: 0.into(), sequence_name: "table_id_seq".to_string(), table_id: 0.into(), col_id: 0.into(), increment: 1, start: 6, min_value: 1, max_value: 4294967295, allocated: 4096 },
                StSequenceRow { sequence_id: 1.into(), sequence_name: "sequence_id_seq".to_string(), table_id: 2.into(), col_id: 0.into(), increment: 1, start: 4, min_value: 1, max_value: 4294967295, allocated: 4096 },
                StSequenceRow { sequence_id: 2.into(), sequence_name: "index_id_seq".to_string(), table_id: 3.into(), col_id: 0.into(), increment: 1, start: 6, min_value: 1, max_value: 4294967295, allocated: 4096 },
                StSequenceRow { sequence_id: 3.into(), sequence_name: "constraint_id_seq".to_string(), table_id: 4.into(), col_id: 0.into(), increment: 1, start: 1, min_value: 1, max_value: 4294967295, allocated: 4096 },
            ]
        );
        let constraints_rows = datastore
            .iter_mut_tx(&ExecutionContext::default(), &tx, ST_CONSTRAINTS_ID)?
            .map(|x| StConstraintRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.constraint_id)
            .collect::<Vec<_>>();

        #[rustfmt::skip]
        assert_eq!(
            constraints_rows,
            vec![
                StConstraintRow{ constraint_id: 5.into(), constraint_name: "ct_columns_table_id".to_string(), kind:  ColumnIndexAttribute::INDEXED, table_id: 1.into(), columns: vec![0.into()] },
            ]
        );
        datastore.rollback_mut_tx_for_test(tx);
        Ok(())
    }

    #[test]
    fn test_create_table_pre_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        let table_rows = datastore
            .iter_by_col_eq_mut_tx(
                &ExecutionContext::default(),
                &tx,
                ST_TABLES_ID,
                ColId(0),
                table_id.into(),
            )?
            .map(|x| StTableRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.table_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            table_rows,
            vec![
                // table_id, table_name, table_type, table_access
                table_row(6, "Foo", StTableType::User, StAccess::Public)
            ]
        );
        let column_rows = datastore
            .iter_by_col_eq_mut_tx(
                &ExecutionContext::default(),
                &tx,
                ST_COLUMNS_ID,
                ColId(0),
                table_id.into(),
            )?
            .map(|x| StColumnRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| (x.table_id, x.col_id))
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            column_rows,
            vec![
                // table_id, col_id, col_name, col_type, is_autoinc
                column_row(6, 0, "id", AlgebraicType::U32, true),
                column_row(6, 1, "name", AlgebraicType::String, false),
                column_row(6, 2, "age", AlgebraicType::U32, false),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_create_table_post_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx_for_test(tx)?;
        let tx = datastore.begin_mut_tx();
        let table_rows = datastore
            .iter_by_col_eq_mut_tx(
                &ExecutionContext::default(),
                &tx,
                ST_TABLES_ID,
                ColId(0),
                table_id.into(),
            )?
            .map(|x| StTableRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.table_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            table_rows,
            vec![
                // table_id, table_name, table_type, table_access
                table_row(6, "Foo", StTableType::User, StAccess::Public)
            ]
        );
        let column_rows = datastore
            .iter_by_col_eq_mut_tx(
                &ExecutionContext::default(),
                &tx,
                ST_COLUMNS_ID,
                ColId(0),
                table_id.into(),
            )?
            .map(|x| StColumnRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| (x.table_id, x.col_id))
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            column_rows,
            vec![
                // table_id, col_id, col_name, col_type, is_autoinc
                column_row(6, 0, "id", AlgebraicType::U32, true),
                column_row(6, 1, "name", AlgebraicType::String, false),
                column_row(6, 2, "age", AlgebraicType::U32, false),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_create_table_post_rollback() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.rollback_mut_tx_for_test(tx);
        let tx = datastore.begin_mut_tx();
        let table_rows = datastore
            .iter_by_col_eq_mut_tx(
                &ExecutionContext::default(),
                &tx,
                ST_TABLES_ID,
                ColId(0),
                table_id.into(),
            )?
            .map(|x| StTableRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.table_id)
            .collect::<Vec<_>>();
        assert_eq!(table_rows, vec![]);
        let column_rows = datastore
            .iter_by_col_eq_mut_tx(
                &ExecutionContext::default(),
                &tx,
                ST_COLUMNS_ID,
                ColId(0),
                table_id.into(),
            )?
            .map(|x| StColumnRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.table_id)
            .collect::<Vec<_>>();
        assert_eq!(column_rows, vec![]);
        Ok(())
    }

    #[test]
    fn test_schema_for_table_pre_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        let schema = &*datastore.schema_for_table_mut_tx(&tx, table_id)?;
        #[rustfmt::skip]
        assert_eq!(schema, &TableSchema {
            table_id,
            table_name: "Foo".into(),
            columns: vec![
                // table_id, col_id: id, col_name, col_type, is_autoinc
                column_schema(6, 0, "id", AlgebraicType::U32, true),
                column_schema(6, 1, "name", AlgebraicType::String, false),
                column_schema(6, 2, "age", AlgebraicType::U32, false),
            ],
            indexes: vec![
                // index_id, table_id, col_id, index_name, is_unique
                index_schema(6, 6, 0, "id_idx", true),
                index_schema(7, 6, 1, "name_idx", true),
            ],
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        });
        Ok(())
    }

    #[test]
    fn test_schema_for_table_post_commit() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_table()?;
        datastore.commit_mut_tx_for_test(tx)?;
        let tx = datastore.begin_mut_tx();
        let schema = &*datastore.schema_for_table_mut_tx(&tx, table_id)?;
        #[rustfmt::skip]
        assert_eq!(schema, &TableSchema {
            table_id,
            table_name: "Foo".into(),
            columns: vec![
                // table_id, col_id: id, col_name, col_type, is_autoinc
                column_schema(6, 0, "id", AlgebraicType::U32, true),
                column_schema(6, 1, "name", AlgebraicType::String, false),
                column_schema(6, 2, "age", AlgebraicType::U32, false),
            ],
            indexes: vec![
                // index_id, table_id, col_id, index_name, is_unique
                index_schema(6, 6, 0, "id_idx", true),
                index_schema(7, 6, 1, "name_idx", true),
            ],
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        });
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

        datastore.create_index_mut_tx(&mut tx, IndexDef::new("id_idx".into(), 6.into(), 0.into(), true))?;

        let expected_indexes = vec![index_schema(8, 6, 0, "id_idx", true)];
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
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
        ]);
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
                col_names: _,
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
                col_names: _,
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
        let index_def = IndexDef::new("age_idx".to_string(), table_id, 2.into(), true);
        datastore.create_index_mut_tx(&mut tx, index_def)?;
        let index_rows = datastore
            .iter_mut_tx(&ExecutionContext::default(), &tx, ST_INDEXES_ID)?
            .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.index_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(index_rows, vec![
            // index_id, table_id, col_id, index_name, is_unique
            index_row(0, 0, 0, "table_id_idx", true),
            index_row(1, 3, 0, "index_id_idx", true),
            index_row(2, 2, 0, "sequences_id_idx", true),
            index_row(3, 0, 1, "table_name_idx", true),
            index_row(4, 4, 0, "constraint_id_idx", true),
            index_row(5, 1, 0, "idx_ct_columns_table_id", false),
            index_row(6, 6, 0, "id_idx", true),
            index_row(7, 6, 1, "name_idx", true),
            index_row(8, 6, 2, "age_idx", true),
        ]);
        let row = u32_str_u32(0, "Bar", 18); // 0 will be ignored.
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                col_names: _,
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
        let index_def = IndexDef::new("age_idx".to_string(), table_id, 2.into(), true);
        datastore.create_index_mut_tx(&mut tx, index_def)?;
        datastore.commit_mut_tx_for_test(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let index_rows = datastore
            .iter_mut_tx(&ExecutionContext::default(), &tx, ST_INDEXES_ID)?
            .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.index_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(index_rows, vec![
            // index_id, table_id, col_id, index_name, is_unique
            index_row(0, 0, 0, "table_id_idx", true),
            index_row(1, 3, 0, "index_id_idx", true),
            index_row(2, 2, 0, "sequences_id_idx", true),
            index_row(3, 0, 1, "table_name_idx", true),
            index_row(4, 4, 0, "constraint_id_idx", true),
            index_row(5, 1, 0, "idx_ct_columns_table_id", false),
            index_row(6, 6, 0, "id_idx", true),
            index_row(7, 6, 1, "name_idx", true),
            index_row(8, 6, 2, "age_idx", true),
        ]);

        let row = u32_str_u32(0, "Bar", 18); // 0 will be ignored.
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                col_names: _,
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
        let index_def = IndexDef::new("age_idx".to_string(), table_id, 2.into(), true);
        datastore.create_index_mut_tx(&mut tx, index_def)?;
        datastore.rollback_mut_tx_for_test(tx);
        let mut tx = datastore.begin_mut_tx();
        let index_rows = datastore
            .iter_mut_tx(&ExecutionContext::default(), &tx, ST_INDEXES_ID)?
            .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.index_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(index_rows, vec![
            // index_id, table_id, col_id, index_name, is_unique
            index_row(0, 0, 0, "table_id_idx", true),
            index_row(1, 3, 0, "index_id_idx", true),
            index_row(2, 2, 0, "sequences_id_idx", true),
            index_row(3, 0, 1, "table_name_idx", true),
            index_row(4, 4, 0, "constraint_id_idx", true),
            index_row(5, 1, 0, "idx_ct_columns_table_id", false),
            index_row(6, 6, 0, "id_idx", true),
            index_row(7, 6, 1, "name_idx", true),
        ]);
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

    // TODO: Add the following tests
    // - Create index with unique constraint and immediately insert a row that violates the constraint before committing.
    // - Create a tx that inserts 2000 rows with an autoinc column
    // - Create a tx that inserts 2000 rows with an autoinc column and then rolls back
    // - Test creating sequences pre_commit, post_commit, post_rollback
}
