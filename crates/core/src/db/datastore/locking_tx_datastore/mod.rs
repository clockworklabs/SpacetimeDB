mod btree_index;
mod sequence;
mod table;
use self::{
    btree_index::{BTreeIndex, BTreeIndexRangeIter},
    sequence::Sequence,
    table::Table,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::RangeBounds,
    sync::Arc,
    vec,
};

use super::{
    system_tables::{
        StColumnRow, StIndexRow, StSequenceRow, StTableRow, INDEX_ID_SEQUENCE_ID, SEQUENCE_ID_SEQUENCE_ID,
        ST_COLUMNS_ID, ST_COLUMNS_ROW_TYPE, ST_INDEXES_ID, ST_INDEX_ROW_TYPE, ST_SEQUENCES_ID, ST_SEQUENCE_ROW_TYPE,
        ST_TABLES_ID, ST_TABLE_ROW_TYPE, TABLE_ID_SEQUENCE_ID,
    },
    traits::{
        self, ColId, DataRow, IndexDef, IndexId, IndexSchema, MutTx, MutTxDatastore, SequenceDef, SequenceId, TableDef,
        TableId, TableSchema, TxData, TxDatastore,
    },
};

use crate::db::datastore::system_tables::{
    st_constraints_schema, table_name_is_system, StConstraintRow, SystemTables, CONSTRAINT_ID_SEQUENCE_ID,
    ST_CONSTRAINTS_ID, ST_CONSTRAINT_ROW_TYPE,
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
use parking_lot::{lock_api::ArcMutexGuard, Mutex, RawMutex};
use spacetimedb_lib::{
    auth::{StAccess, StTableType},
    data_key::ToDataKey,
    DataKey,
};
use spacetimedb_sats::{
    AlgebraicType, AlgebraicValue, BuiltinType, BuiltinValue, ProductType, ProductTypeElement, ProductValue,
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
}

const SEQUENCE_PREALLOCATION_AMOUNT: i128 = 4_096;

pub struct Data {
    data: ProductValue,
}

impl From<Data> for ProductValue {
    fn from(data: Data) -> Self {
        data.data
    }
}

impl traits::Data for Data {
    fn view(&self) -> &ProductValue {
        &self.data
    }
}

#[derive(Clone)]
pub struct DataRef {
    data: ProductValue,
}

impl DataRef {
    fn new(data: ProductValue) -> Self {
        Self { data }
    }

    pub fn view(&self) -> &ProductValue {
        &self.data
    }
}

pub struct MutTxId {
    lock: ArcMutexGuard<RawMutex, Inner>,
}

struct CommittedState {
    tables: HashMap<TableId, Table>,
}

impl CommittedState {
    fn new() -> Self {
        Self { tables: HashMap::new() }
    }

    fn get_or_create_table(&mut self, table_id: TableId, row_type: &ProductType, schema: &TableSchema) -> &mut Table {
        self.tables.entry(table_id).or_insert_with(|| Table {
            row_type: row_type.clone(),
            schema: schema.clone(),
            rows: BTreeMap::new(),
            indexes: HashMap::new(),
        })
    }

    fn get_table(&mut self, table_id: &TableId) -> Option<&mut Table> {
        self.tables.get_mut(table_id)
    }

    fn merge(&mut self, tx_state: TxState, memory: BTreeMap<DataKey, Arc<Vec<u8>>>) -> TxData {
        let mut tx_data = TxData { records: vec![] };
        for (table_id, table) in tx_state.insert_tables {
            let commit_table = self.get_or_create_table(table_id, &table.row_type, &table.schema);
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
                if !commit_table.indexes.contains_key(&ColId(index.col_id)) {
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
        col_id: &ColId,
        value: &'a AlgebraicValue,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        if let Some(table) = self.tables.get(table_id) {
            table.index_seek(*col_id, value)
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
enum RowState {
    /// The row is present in the table because it was inserted
    /// in a previously committed transaction.
    Committed(ProductValue),
    /// The row is present because it has been inserted in the
    /// current transaction.
    Insert(ProductValue),
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

    pub fn get_row_op(&self, table_id: &TableId, row_id: &RowId) -> RowState {
        if let Some(true) = self.delete_tables.get(table_id).map(|set| set.contains(row_id)) {
            return RowState::Delete;
        }
        let Some(table) = self.insert_tables.get(table_id) else {
            return RowState::Absent;
        };
        table
            .get_row(row_id)
            .map(|pv| RowState::Insert(pv.clone()))
            .unwrap_or(RowState::Absent)
    }

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

    /// When there's an index on `col_id`,
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
        col_id: &ColId,
        value: &'a AlgebraicValue,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        self.insert_tables.get(table_id)?.index_seek(*col_id, value)
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
}

impl Inner {
    pub fn new() -> Self {
        Self {
            memory: BTreeMap::new(),
            committed_state: CommittedState::new(),
            tx_state: None,
            sequence_state: SequencesState::new(),
        }
    }

    fn bootstrap_system_table(&mut self, schema: TableSchema) -> Result<(), DBError> {
        let table_id = schema.table_id;
        let table_name = &schema.table_name;

        // Insert the table row into st_tables, creating st_tables if it's missing
        let st_tables = self
            .committed_state
            .get_or_create_table(ST_TABLES_ID, &ST_TABLE_ROW_TYPE, &st_table_schema());
        let row = StTableRow {
            table_id,
            table_name: &table_name,
            table_type: StTableType::System,
            table_access: StAccess::Public,
        };
        let row: ProductValue = (&row).into();
        let data_key = row.to_data_key();
        st_tables.rows.insert(RowId(data_key), row);

        // Insert the columns into st_columns
        for (i, col) in schema.columns.iter().enumerate() {
            let row = StColumnRow {
                table_id,
                col_id: i as u32,
                col_name: &col.col_name,
                col_type: col.col_type.clone(),
                is_autoinc: col.is_autoinc,
            };
            let row = ProductValue::from(&row);
            let data_key = row.to_data_key();
            {
                let st_columns =
                    self.committed_state
                        .get_or_create_table(ST_COLUMNS_ID, &ST_COLUMNS_ROW_TYPE, &st_columns_schema());
                st_columns.rows.insert(RowId(data_key), row);
            }

            // If any columns are auto incrementing, we need to create a sequence
            // NOTE: This code with the `seq_start` is particularly fragile.
            if col.is_autoinc {
                // The database is bootstrapped with the total of `SystemTables::total_` that identify what is the start of the sequence
                let (seq_start, seq_id): (i128, SequenceId) = match TableId(schema.table_id) {
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
                    sequence_id: seq_id.0,
                    sequence_name: &format!("{}_seq", col.col_name),
                    table_id: col.table_id,
                    col_id: col.col_id,
                    increment: 1,
                    start: seq_start,
                    min_value: 1,
                    max_value: u32::MAX as i128,
                    allocated: SEQUENCE_PREALLOCATION_AMOUNT,
                };
                let row = ProductValue::from(&row);
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
        for constraint in schema.constraints {
            assert_eq!(constraint.columns.len(), 1, "Constraints only supported for 1 column.");
            let col_id = schema.columns.first().unwrap();

            let row = StConstraintRow {
                constraint_id: constraint.constraint_id,
                constraint_name: constraint.constraint_name.clone(),
                kind: constraint.kind,
                table_id,
                columns: constraint.columns,
            };
            let row = ProductValue::from(&row);
            let data_key = row.to_data_key();
            st_constraints.rows.insert(RowId(data_key), row);

            //Check if add an index:
            let idx = match constraint.kind {
                x if x.is_unique() => IndexSchema {
                    index_id: constraint.constraint_id,
                    table_id,
                    col_id: col_id.col_id,
                    index_name: format!("idx_{}", &constraint.constraint_name),
                    is_unique: true,
                },
                x if x.is_indexed() => IndexSchema {
                    index_id: constraint.constraint_id,
                    table_id,
                    col_id: col_id.col_id,
                    index_name: format!("idx_{}", &constraint.constraint_name),
                    is_unique: false,
                },
                x => {
                    panic!("Adding constraint of kind `{x:?}` is not supported yet.")
                }
            };
            indexes.push(idx);
        }

        // Insert the indexes into st_indexes
        let st_indexes =
            self.committed_state
                .get_or_create_table(ST_INDEXES_ID, &ST_INDEX_ROW_TYPE, &st_indexes_schema());
        for (_, index) in indexes.iter().enumerate() {
            let row = StIndexRow {
                index_id: index.index_id,
                table_id,
                col_id: index.col_id,
                index_name: &index.index_name,
                is_unique: index.is_unique,
            };
            let row = ProductValue::from(&row);
            let data_key = row.to_data_key();
            st_indexes.rows.insert(RowId(data_key), row);
        }

        Ok(())
    }

    fn build_sequence_state(&mut self) -> super::Result<()> {
        let st_sequences = self.committed_state.tables.get(&ST_SEQUENCES_ID).unwrap();
        let rows = st_sequences.scan_rows().cloned().collect::<Vec<_>>();
        for row in rows {
            let sequence = StSequenceRow::try_from(&row)?;
            let schema = (&sequence).into();
            self.sequence_state
                .sequences
                .insert(SequenceId(sequence.sequence_id), Sequence::new(schema));
        }
        Ok(())
    }

    fn build_indexes(&mut self) -> super::Result<()> {
        let st_indexes = self.committed_state.tables.get(&ST_INDEXES_ID).unwrap();
        let rows = st_indexes.scan_rows().cloned().collect::<Vec<_>>();
        for row in rows {
            let index_row = StIndexRow::try_from(&row)?;
            let table = self.committed_state.get_table(&TableId(index_row.table_id)).unwrap();
            let mut index = BTreeIndex::new(
                IndexId(index_row.index_id),
                index_row.table_id,
                index_row.col_id,
                index_row.index_name.into(),
                index_row.is_unique,
            );
            index.build_from_rows(table.scan_rows())?;
            table.indexes.insert(ColId(index_row.col_id), index);
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
            let table_id = TableId(table_row.table_id);
            let schema = self.schema_for_table(table_id)?;
            let row_type = self.row_type_for_table(table_id)?;
            if self.committed_state.get_table(&table_id).is_none() {
                self.committed_state.tables.insert(
                    table_id,
                    Table {
                        row_type,
                        schema,
                        indexes: HashMap::new(),
                        rows: BTreeMap::new(),
                    },
                );
            }
        }
        Ok(())
    }

    fn drop_table_from_st_tables(&mut self, table_id: TableId) -> super::Result<()> {
        const ST_TABLES_TABLE_ID_COL: ColId = ColId(0);
        let value = AlgebraicValue::U32(table_id.0);
        let rows = self.iter_by_col_eq(&ST_TABLES_ID, &ST_TABLES_TABLE_ID_COL, &value)?;
        let rows = rows.map(|row| row.view().to_owned()).collect::<Vec<_>>();
        if rows.is_empty() {
            return Err(TableError::IdNotFound(table_id.0).into());
        }
        self.delete_by_rel(&table_id, rows)?;
        Ok(())
    }

    fn drop_table_from_st_columns(&mut self, table_id: TableId) -> super::Result<()> {
        const ST_COLUMNS_TABLE_ID_COL: ColId = ColId(0);
        let value = AlgebraicValue::U32(table_id.0);
        let rows = self.iter_by_col_eq(&ST_COLUMNS_ID, &ST_COLUMNS_TABLE_ID_COL, &value)?;
        let rows = rows.map(|row| row.view().to_owned()).collect::<Vec<_>>();
        if rows.is_empty() {
            return Err(TableError::IdNotFound(table_id.0).into());
        }
        self.delete_by_rel(&table_id, rows)?;
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    fn get_next_sequence_value(&mut self, seq_id: SequenceId) -> super::Result<i128> {
        {
            let Some(sequence) = self.sequence_state.get_sequence_mut(seq_id) else {
                return Err(SequenceError::NotFound(seq_id).into());
            };

            // If there are allocated sequence values, return the new value.
            if let Some(value) = sequence.gen_next_value() {
                return Ok(value);
            }
        }
        // Allocate new sequence values
        // If we're out of allocations, then update the sequence row in st_sequences to allocate a fresh batch of sequences.
        const ST_SEQUENCES_SEQUENCE_ID_COL: ColId = ColId(0);
        let old_seq_row = self
            .iter_by_col_eq(
                &ST_SEQUENCES_ID,
                &ST_SEQUENCES_SEQUENCE_ID_COL,
                &AlgebraicValue::U32(seq_id.0),
            )?
            .last()
            .unwrap()
            .data;
        let (seq_row, old_seq_row_id) = {
            let Some(sequence) = self.sequence_state.get_sequence_mut(seq_id) else {
                return Err(SequenceError::NotFound(seq_id).into());
            };
            let old_seq_row_id = RowId(old_seq_row.to_data_key());
            let mut seq_row = StSequenceRow::try_from(&old_seq_row)?;
            let num_to_allocate = 1024;
            seq_row.allocated = sequence.nth_value(num_to_allocate);
            sequence.set_allocation(seq_row.allocated);
            (seq_row, old_seq_row_id)
        };

        self.delete(&ST_SEQUENCES_ID, &old_seq_row_id)?;
        self.insert(ST_SEQUENCES_ID, ProductValue::from(&seq_row))?;

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
            seq.col_id
        );

        // Insert the sequence row into st_sequences
        // NOTE: Because st_sequences has a unique index on sequence_name, this will
        // fail if the table already exists.
        let sequence_row = StSequenceRow {
            sequence_id: 0, // autogen'd
            sequence_name: seq.sequence_name.as_str(),
            table_id: seq.table_id,
            col_id: seq.col_id,
            allocated: 0,
            increment: seq.increment,
            start: seq.start.unwrap_or(1),
            min_value: seq.min_value.unwrap_or(1),
            max_value: seq.max_value.unwrap_or(i128::MAX),
        };
        let row = (&sequence_row).into();
        let result = self.insert(ST_SEQUENCES_ID, row)?;
        let sequence_row = StSequenceRow::try_from(&result)?;
        let sequence_id = SequenceId(sequence_row.sequence_id);

        let schema = (&sequence_row).into();
        self.sequence_state.sequences.insert(sequence_id, Sequence::new(schema));

        log::trace!("SEQUENCE CREATED: {}", seq.sequence_name);

        Ok(sequence_id)
    }

    fn drop_sequence(&mut self, seq_id: SequenceId) -> super::Result<()> {
        const ST_SEQUENCES_SEQUENCE_ID_COL: ColId = ColId(0);
        let old_seq_row = self
            .iter_by_col_eq(
                &ST_SEQUENCES_ID,
                &ST_SEQUENCES_SEQUENCE_ID_COL,
                &AlgebraicValue::U32(seq_id.0),
            )?
            .last()
            .unwrap()
            .data;
        let old_seq_row_id = RowId(old_seq_row.to_data_key());
        self.delete(&ST_SEQUENCES_ID, &old_seq_row_id)?;
        self.sequence_state.sequences.remove(&seq_id);
        Ok(())
    }

    fn sequence_id_from_name(&self, seq_name: &str) -> super::Result<Option<SequenceId>> {
        let seq_name_col: ColId = ColId(1);
        self.iter_by_col_eq(
            &ST_SEQUENCES_ID,
            &seq_name_col,
            &AlgebraicValue::String(seq_name.to_owned()),
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
            table_id: 0,
            table_name,
            table_type: table_schema.table_type,
            table_access: table_schema.table_access,
        };
        let table_id = StTableRow::try_from(&self.insert(ST_TABLES_ID, (&row).into())?)?.table_id;

        // Insert the columns into st_columns
        for (i, col) in table_schema.columns.iter().enumerate() {
            let col_id = i as u32;
            let row = StColumnRow {
                table_id,
                col_id,
                col_name: &col.col_name,
                col_type: col.col_type.clone(),
                is_autoinc: col.is_autoinc,
            };
            self.insert(ST_COLUMNS_ID, (&row).into())?;

            // Insert create the sequence for the autoinc column
            if col.is_autoinc {
                let sequence_def = SequenceDef {
                    sequence_name: format!("{}_{}_seq", table_name, col.col_name),
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
        let schema = self.schema_for_table(TableId(table_id))?;

        // Create the in memory representation of the table
        // NOTE: This should be done before creating the indexes
        self.create_table_internal(TableId(table_id), table_schema.get_row_type(), schema)?;

        // Create the indexes for the table
        for mut index in table_schema.indexes {
            // NOTE: The below ensure, that when creating a table you can only
            // create indexes on the table you are creating.
            index.table_id = table_id;
            self.create_index(index)?;
        }

        log::trace!("TABLE CREATED: {table_name}, table_id:{table_id}");

        Ok(TableId(table_id))
    }

    fn create_table_internal(
        &mut self,
        table_id: TableId,
        row_type: ProductType,
        schema: TableSchema,
    ) -> super::Result<()> {
        self.tx_state.as_mut().unwrap().insert_tables.insert(
            table_id,
            Table {
                row_type,
                schema,
                indexes: HashMap::new(),
                rows: BTreeMap::new(),
            },
        );
        Ok(())
    }

    fn row_type_for_table(&self, table_id: TableId) -> super::Result<ProductType> {
        // Fetch the `ProductType` from the in memory table if it exists.
        // The `ProductType` is invalidated if the schema of the table changes.
        if let Some(row_type) = self.get_row_type(&table_id) {
            return Ok(row_type.clone());
        }

        // Look up the columns for the table in question.
        // NOTE: This is quite an expensive operation, although we only need
        // to do this in situations where there is not currently an in memory
        // representation of a table. This would happen in situations where
        // we have created the table in the database, but have not yet
        // represented in memory or inserted any rows into it.
        let table_schema = self.schema_for_table(table_id)?;
        let elements = table_schema
            .columns
            .into_iter()
            .map(|col| ProductTypeElement {
                name: None,
                algebraic_type: col.col_type,
            })
            .collect();
        Ok(ProductType { elements })
    }

    #[tracing::instrument(skip_all)]
    fn schema_for_table(&self, table_id: TableId) -> super::Result<TableSchema> {
        if let Some(schema) = self.get_schema(&table_id) {
            return Ok(schema.clone());
        }

        // Look up the table_name for the table in question.
        let table_id_col: ColId = ColId(0);

        // TODO(george): As part of the bootstrapping process, we add a bunch of rows
        // and only at very end do we patch things up and create table metadata, indexes,
        // and so on. Early parts of that process insert rows, and need the schema to do
        // so. We can't just call iter_by_col_eq here as that would attempt to use the
        // index which we haven't created yet. So instead we just manually Scan here.
        let rows = IterByColEq::Scan(ScanIterByColEq {
            value: &AlgebraicValue::U32(table_id.0),
            col_id: table_id_col,
            scan_iter: self.iter(&ST_TABLES_ID)?,
        })
        .collect::<Vec<_>>();
        assert!(rows.len() <= 1, "Expected at most one row in st_tables for table_id");

        let row = rows.first().ok_or_else(|| TableError::IdNotFound(table_id.0))?;
        let el = StTableRow::try_from(row.view())?;
        let table_name = el.table_name.to_owned();
        let table_id = el.table_id;

        // Look up the columns for the table in question.
        let mut columns = Vec::new();
        const TABLE_ID_COL: ColId = ColId(0);
        for data_ref in self.iter_by_col_eq(&ST_COLUMNS_ID, &TABLE_ID_COL, &AlgebraicValue::U32(table_id))? {
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
        let table_id_col: ColId = ColId(1);
        for data_ref in self.iter_by_col_eq(&ST_INDEXES_ID, &table_id_col, &AlgebraicValue::U32(table_id))? {
            let row = data_ref.view();

            let el = StIndexRow::try_from(row)?;
            let index_schema = IndexSchema {
                table_id: el.table_id,
                col_id: el.col_id,
                index_name: el.index_name.into(),
                is_unique: el.is_unique,
                index_id: el.index_id,
            };
            indexes.push(index_schema);
        }

        Ok(TableSchema {
            columns,
            table_id,
            table_name,
            indexes,
            constraints: vec![],
            table_type: el.table_type,
            table_access: el.table_access,
        })
    }

    fn drop_table(&mut self, table_id: TableId) -> super::Result<()> {
        // First drop the tables indexes.
        const ST_INDEXES_TABLE_ID_COL: ColId = ColId(1);
        let rows = self
            .iter_by_col_eq(
                &ST_INDEXES_ID,
                &ST_INDEXES_TABLE_ID_COL,
                &AlgebraicValue::U32(table_id.0),
            )?
            .collect::<Vec<_>>();
        for data_ref in rows {
            let row = data_ref.view();
            let el = StIndexRow::try_from(row)?;
            self.drop_index(&IndexId(el.index_id))?;
        }

        // Remove the table's sequences from st_sequences.
        const ST_SEQUENCES_TABLE_ID_COL: ColId = ColId(2);
        let rows = self
            .iter_by_col_eq(
                &ST_SEQUENCES_ID,
                &ST_SEQUENCES_TABLE_ID_COL,
                &AlgebraicValue::U32(table_id.0),
            )?
            .collect::<Vec<_>>();
        for data_ref in rows {
            let row = data_ref.view();
            let el = StSequenceRow::try_from(row)?;
            self.drop_sequence(SequenceId(el.sequence_id))?;
        }

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
        let rows = self
            .iter_by_col_eq(&ST_TABLES_ID, &ST_TABLES_TABLE_ID_COL, &AlgebraicValue::U32(table_id.0))?
            .collect::<Vec<_>>();
        assert!(rows.len() <= 1, "Expected at most one row in st_tables for table_id");
        let row = rows.first().ok_or_else(|| TableError::IdNotFound(table_id.0))?;
        let row_id = RowId(row.view().to_data_key());
        let mut el = StTableRow::try_from(row.view())?;
        el.table_name = new_name;
        self.delete(&ST_TABLES_ID, &row_id)?;
        self.insert(ST_TABLES_ID, (&el).into())?;
        Ok(())
    }

    fn table_id_from_name(&self, table_name: &str) -> super::Result<Option<TableId>> {
        let table_name_col: ColId = ColId(1);
        self.iter_by_col_eq(
            &ST_TABLES_ID,
            &table_name_col,
            &AlgebraicValue::String(table_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| TableId(*row.view().elements[0].as_u32().unwrap()))
        })
    }

    fn table_name_from_id(&self, table_id: TableId) -> super::Result<Option<String>> {
        let table_id_col: ColId = ColId(0);
        self.iter_by_col_eq(&ST_TABLES_ID, &table_id_col, &AlgebraicValue::U32(table_id.0))
            .map(|mut iter| {
                iter.next()
                    .map(|row| row.view().elements[1].as_string().unwrap().to_owned())
            })
    }

    fn create_index(&mut self, index: IndexDef) -> super::Result<IndexId> {
        log::trace!(
            "INDEX CREATING: {} for table: {} and col: {}",
            index.name,
            index.table_id,
            index.col_id
        );

        // Insert the index row into st_indexes
        // NOTE: Because st_indexes has a unique index on index_name, this will
        // fail if the index already exists.
        let row = StIndexRow {
            index_id: 0, // Autogen'd
            table_id: index.table_id,
            col_id: index.col_id,
            index_name: &index.name,
            is_unique: index.is_unique,
        };
        let index_id = StIndexRow::try_from(&self.insert(ST_INDEXES_ID, (&row).into())?)?.index_id;

        // Create the index in memory
        if !self.table_exists(&TableId(index.table_id)) {
            return Err(TableError::IdNotFound(index.table_id).into());
        }
        self.create_index_internal(IndexId(index_id), &index)?;

        log::trace!(
            "INDEX CREATED: {} for table: {} and col: {}",
            index.name,
            index.table_id,
            index.col_id
        );
        Ok(IndexId(index_id))
    }

    fn create_index_internal(&mut self, index_id: IndexId, index: &IndexDef) -> super::Result<()> {
        let insert_table = if let Some(insert_table) = self
            .tx_state
            .as_mut()
            .unwrap()
            .get_insert_table_mut(&TableId(index.table_id))
        {
            insert_table
        } else {
            let row_type = self.row_type_for_table(TableId(index.table_id))?;
            let schema = self.schema_for_table(TableId(index.table_id))?;
            self.tx_state.as_mut().unwrap().insert_tables.insert(
                TableId(index.table_id),
                Table {
                    row_type,
                    schema,
                    indexes: HashMap::new(),
                    rows: BTreeMap::new(),
                },
            );
            self.tx_state
                .as_mut()
                .unwrap()
                .get_insert_table_mut(&TableId(index.table_id))
                .unwrap()
        };

        let mut insert_index = BTreeIndex::new(
            index_id,
            index.table_id,
            index.col_id,
            index.name.to_string(),
            index.is_unique,
        );
        insert_index.build_from_rows(insert_table.scan_rows())?;

        // NOTE: Also add all the rows in the already committed table to the index.
        if let Some(committed_table) = self.committed_state.get_table(&TableId(index.table_id)) {
            insert_index.build_from_rows(committed_table.scan_rows())?;
        }

        insert_table.schema.indexes.push(IndexSchema {
            table_id: index.table_id,
            col_id: index.col_id,
            index_name: index.name.to_string(),
            is_unique: index.is_unique,
            index_id: index_id.0,
        });

        insert_table.indexes.insert(ColId(index.col_id), insert_index);
        Ok(())
    }

    fn drop_index(&mut self, index_id: &IndexId) -> super::Result<()> {
        log::trace!("INDEX DROPPING: {}", index_id.0);

        // Remove the index from st_indexes.
        const ST_INDEXES_INDEX_ID_COL: ColId = ColId(0);
        let old_index_row = self
            .iter_by_col_eq(
                &ST_INDEXES_ID,
                &ST_INDEXES_INDEX_ID_COL,
                &AlgebraicValue::U32(index_id.0),
            )?
            .last()
            .unwrap()
            .data;
        let old_index_row_id = RowId(old_index_row.to_data_key());
        self.delete(&ST_INDEXES_ID, &old_index_row_id)?;

        self.drop_index_internal(index_id);

        log::trace!("INDEX DROPPED: {}", index_id.0);
        Ok(())
    }

    fn drop_index_internal(&mut self, index_id: &IndexId) {
        for (_, table) in self.committed_state.tables.iter_mut() {
            let mut cols = vec![];
            for index in table.indexes.values_mut() {
                if index.index_id == *index_id {
                    cols.push(index.col_id);
                }
            }
            for col in cols {
                table.indexes.remove(&ColId(col));
                table.schema.indexes.retain(|x| x.col_id != col);
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
                    cols.push(index.col_id);
                }
            }
            for col in cols {
                insert_table.indexes.remove(&ColId(col));
                insert_table.schema.indexes.retain(|x| x.col_id != col);
            }
        }
    }

    fn index_id_from_name(&self, index_name: &str) -> super::Result<Option<IndexId>> {
        let index_name_col: ColId = ColId(3);
        self.iter_by_col_eq(
            &ST_INDEXES_ID,
            &index_name_col,
            &AlgebraicValue::String(index_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| IndexId(*row.view().elements[0].as_u32().unwrap()))
        })
    }

    fn contains_row(&self, table_id: &TableId, row_id: &RowId) -> RowState {
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
            Some(pv) => RowState::Committed(pv.clone()),
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

    fn sequence_value_to_algebraic_value(
        table_name: &str,
        col_name: &str,
        ty: &AlgebraicType,
        sequence_value: i128,
    ) -> Result<AlgebraicValue, SequenceError> {
        match ty {
            AlgebraicType::Builtin(of) => Ok(match of {
                BuiltinType::I8 => AlgebraicValue::I8(sequence_value as i8),
                BuiltinType::U8 => AlgebraicValue::U8(sequence_value as u8),
                BuiltinType::I16 => AlgebraicValue::I16(sequence_value as i16),
                BuiltinType::U16 => AlgebraicValue::U16(sequence_value as u16),
                BuiltinType::I32 => AlgebraicValue::I32(sequence_value as i32),
                BuiltinType::U32 => AlgebraicValue::U32(sequence_value as u32),
                BuiltinType::I64 => AlgebraicValue::I64(sequence_value as i64),
                BuiltinType::U64 => AlgebraicValue::U64(sequence_value as u64),
                BuiltinType::I128 => AlgebraicValue::I128(sequence_value),
                BuiltinType::U128 => AlgebraicValue::U128(sequence_value as u128),
                _ => {
                    return Err(SequenceError::NotInteger {
                        col: format!("{}.{}", table_name, col_name),
                        found: ty.clone(),
                    })
                }
            }),
            _ => Err(SequenceError::NotInteger {
                col: format!("{}.{}", table_name, col_name),
                found: ty.clone(),
            }),
        }
    }

    /// Check if the value is one of the `numeric` types and is `0`.
    fn can_replace_with_sequence(value: &AlgebraicValue) -> bool {
        match value.as_builtin() {
            Some(x) => match x {
                BuiltinValue::I8(x) => *x == 0,
                BuiltinValue::U8(x) => *x == 0,
                BuiltinValue::I16(x) => *x == 0,
                BuiltinValue::U16(x) => *x == 0,
                BuiltinValue::I32(x) => *x == 0,
                BuiltinValue::U32(x) => *x == 0,
                BuiltinValue::I64(x) => *x == 0,
                BuiltinValue::U64(x) => *x == 0,
                BuiltinValue::I128(x) => *x == 0,
                BuiltinValue::U128(x) => *x == 0,
                BuiltinValue::F32(x) => *x == 0.0,
                BuiltinValue::F64(x) => *x == 0.0,
                _ => false,
            },
            _ => false,
        }
    }

    #[tracing::instrument(skip_all)]
    fn insert(&mut self, table_id: TableId, mut row: ProductValue) -> super::Result<ProductValue> {
        // TODO: Excuting schema_for_table for every row insert is expensive.
        // We should store the schema in the [Table] struct instead.
        let schema = self.schema_for_table(table_id)?;
        for col in schema.columns {
            if col.is_autoinc {
                if !Self::can_replace_with_sequence(&row.elements[col.col_id as usize]) {
                    continue;
                }
                let st_sequences_table_id_col = ColId(2);
                for seq_row in self.iter_by_col_eq(
                    &ST_SEQUENCES_ID,
                    &st_sequences_table_id_col,
                    &AlgebraicValue::U32(table_id.0),
                )? {
                    let seq_row = seq_row.view();
                    let seq_row = StSequenceRow::try_from(seq_row)?;
                    if seq_row.col_id != col.col_id {
                        continue;
                    }
                    let sequence_value = self.get_next_sequence_value(SequenceId(seq_row.sequence_id))?;
                    row.elements[col.col_id as usize] = Self::sequence_value_to_algebraic_value(
                        &schema.table_name,
                        &col.col_name,
                        &col.col_type,
                        sequence_value,
                    )?;
                    break;
                }
            }
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
                return Err(TableError::IdNotFound(table_id.0).into());
            };
            let table = Table {
                row_type: committed_table.row_type.clone(),
                schema: committed_table.get_schema().clone(),
                indexes: committed_table
                    .indexes
                    .iter()
                    .map(|(col_id, index)| {
                        (
                            *col_id,
                            BTreeIndex::new(
                                index.index_id,
                                index.table_id,
                                index.col_id,
                                index.name.clone(),
                                index.is_unique,
                            ),
                        )
                    })
                    .collect::<HashMap<_, _>>(),
                rows: BTreeMap::new(),
            };
            self.tx_state.as_mut().unwrap().insert_tables.insert(table_id, table);
            self.tx_state.as_ref().unwrap().get_insert_table(&table_id).unwrap()
        };

        // Check unique constraints
        for index in insert_table.indexes.values() {
            if index.violates_unique_constraint(&row) {
                let value = row.get_field(index.col_id as usize, None).unwrap();
                return Err(IndexError::UniqueConstraintViolation {
                    constraint_name: index.name.clone(),
                    table_name: insert_table.schema.table_name.clone(),
                    col_name: insert_table.schema.columns[index.col_id as usize].col_name.clone(),
                    value: value.clone(),
                }
                .into());
            }
        }
        if let Some(table) = self.committed_state.tables.get_mut(&table_id) {
            for index in table.indexes.values() {
                let Some(violators) = index.get_rows_that_violate_unique_constraint(&row) else {
                    continue;
                };
                for row_id in violators {
                    if let Some(delete_table) = self.tx_state.as_ref().unwrap().delete_tables.get(&table_id) {
                        if !delete_table.contains(&row_id) {
                            let value = row.get_field(index.col_id as usize, None).unwrap();
                            return Err(IndexError::UniqueConstraintViolation {
                                constraint_name: index.name.clone(),
                                table_name: table.schema.table_name.clone(),
                                col_name: table.schema.columns[index.col_id as usize].col_name.clone(),
                                value: value.clone(),
                            }
                            .into());
                        }
                    } else {
                        let value = row.get_field(index.col_id as usize, None).unwrap();
                        return Err(IndexError::UniqueConstraintViolation {
                            constraint_name: index.name.clone(),
                            table_name: table.schema.table_name.clone(),
                            col_name: table.schema.columns[index.col_id as usize].col_name.clone(),
                            value: value.clone(),
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
                return Err(TableError::RowInvalidType {
                    table_id: table_id.0,
                    row,
                }
                .into());
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

    fn get(&self, table_id: &TableId, row_id: &RowId) -> super::Result<Option<DataRef>> {
        if !self.table_exists(table_id) {
            return Err(TableError::IdNotFound(table_id.0).into());
        }
        match self.tx_state.as_ref().unwrap().get_row_op(table_id, row_id) {
            RowState::Committed(_) => unreachable!("a row cannot be committed in a tx state"),
            RowState::Insert(row) => {
                return Ok(Some(DataRef::new(row)));
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
            .map(|row| DataRef::new(row.clone())))
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

    fn delete(&mut self, table_id: &TableId, row_id: &RowId) -> super::Result<bool> {
        Ok(self.delete_row_internal(table_id, row_id))
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

    fn delete_by_rel(
        &mut self,
        table_id: &TableId,
        relation: impl IntoIterator<Item = spacetimedb_sats::ProductValue>,
    ) -> super::Result<Option<u32>> {
        let mut count = 0;
        for tuple in relation {
            let data_key = tuple.to_data_key();
            if self.delete(table_id, &RowId(data_key))? {
                count += 1;
            }
        }
        Ok(Some(count))
    }

    fn iter(&self, table_id: &TableId) -> super::Result<Iter> {
        if self.table_exists(table_id) {
            return Ok(Iter::new(*table_id, self));
        }
        Err(TableError::IdNotFound(table_id.0).into())
    }

    fn iter_by_col_range<'a, R: std::ops::RangeBounds<spacetimedb_sats::AlgebraicValue>>(
        &'a self,
        table_id: &TableId,
        col_id: &ColId,
        range: R,
    ) -> super::Result<IterByColRange<'a, R>> {
        Ok(IterByColRange::Scan(ScanIterByColRange {
            range,
            scan_iter: self.iter(table_id)?,
            col_id: *col_id,
        }))
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the column data identified by `col_id` equates to `value`.
    fn iter_by_col_eq<'a>(
        &'a self,
        table_id: &TableId,
        col_id: &ColId,
        value: &'a AlgebraicValue,
    ) -> super::Result<IterByColEq> {
        // We have to index_seek in both the committed state and the current tx state.
        // First, we will check modifications in the current tx. It may be that the table
        // has not been modified yet in the current tx, in which case we will only search
        // the committed state. Finally, the table may not be indexed at all, in which case
        // we fall back to iterating the entire table.

        // We need to check the tx_state first. In particular, it may be that the index
        // was only added in the current transaction.
        // TODO(george): It's unclear that we truly support dynamically creating an index
        // yet. In particular, I don't know if creating an index in a transaction and
        // rolling it back will leave the index in place.
        if let Some(inserted_rows) = self
            .tx_state
            .as_ref()
            .and_then(|tx_state| tx_state.index_seek(table_id, col_id, value))
        {
            // The current transaction has modified this table, and the table is indexed.
            let tx_state = self.tx_state.as_ref().unwrap();
            Ok(IterByColEq::Index(IndexIterByColEq {
                value,
                col_id: *col_id,
                iter: IndexSeekIterInner {
                    table_id: *table_id,
                    tx_state,
                    inserted_rows,
                    committed_rows: self.committed_state.index_seek(table_id, col_id, value),
                    committed_state: &self.committed_state,
                },
            }))
        } else {
            // Either the current transaction has not modified this table, or the table is not
            // indexed.
            match self.committed_state.index_seek(table_id, col_id, value) {
                Some(committed_rows) => match self.tx_state.as_ref() {
                    None => Ok(IterByColEq::Scan(ScanIterByColEq {
                        value,
                        col_id: *col_id,
                        scan_iter: self.iter(table_id)?,
                    })),
                    Some(tx_state) => Ok(IterByColEq::CommittedIndex(CommittedIndexIterByColEq {
                        table_id: *table_id,
                        tx_state,
                        committed_state: &self.committed_state,
                        committed_rows,
                    })),
                },
                None => Ok(IterByColEq::Scan(ScanIterByColEq {
                    value,
                    col_id: *col_id,
                    scan_iter: self.iter(table_id)?,
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
    pub fn bootstrap() -> Result<Self, DBError> {
        log::trace!("DATABASE: BOOTSTRAPPING SYSTEM TABLES...");

        // NOTE! The bootstrapping process does not take plan in a transaction.
        // This is intentional.
        let mut datastore = Inner::new();

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

    pub fn replay_transaction(
        &self,
        transaction: &Transaction,
        odb: Arc<std::sync::Mutex<Box<dyn ObjectDB + Send>>>,
    ) -> Result<(), DBError> {
        let mut inner = self.inner.lock();
        for write in &transaction.writes {
            let table_id = TableId(write.set_id);
            let schema = inner.schema_for_table(table_id)?;
            let row_type = inner.row_type_for_table(table_id)?;
            let table = inner.committed_state.tables.entry(table_id).or_insert(Table {
                row_type: row_type.clone(),
                schema,
                indexes: HashMap::new(),
                rows: BTreeMap::new(),
            });
            match write.operation {
                Operation::Delete => {
                    table.rows.remove(&RowId(write.data_key));
                }
                Operation::Insert => {
                    let product_value = match write.data_key {
                        DataKey::Data(data) => ProductValue::decode(&row_type, &mut &data[..]).unwrap_or_else(|_| {
                            panic!("Couldn't decode product value to {:?} from message log", row_type)
                        }),
                        DataKey::Hash(hash) => {
                            let data = odb.lock().unwrap().get(hash).unwrap();
                            ProductValue::decode(&row_type, &mut &data[..]).unwrap_or_else(|_| {
                                panic!("Couldn't decode product value to {:?} from message log", row_type)
                            })
                        }
                    };
                    table.rows.insert(RowId(write.data_key), product_value);
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
    type Data = Data;
    type DataRef = DataRef;

    fn data_to_owned(&self, data_ref: Self::DataRef) -> Self::Data {
        Data { data: data_ref.data }
    }
}

impl traits::Tx for Locking {
    type TxId = MutTxId;

    fn begin_tx(&self) -> Self::TxId {
        self.begin_mut_tx()
    }

    fn release_tx(&self, tx: Self::TxId) {
        self.rollback_mut_tx(tx)
    }
}

pub struct Iter<'a> {
    table_id: TableId,
    inner: &'a Inner,
    stage: ScanStage<'a>,
}

impl<'a> Iter<'a> {
    fn new(table_id: TableId, inner: &'a Inner) -> Self {
        Self {
            table_id,
            inner,
            stage: ScanStage::Start,
        }
    }
}

enum ScanStage<'a> {
    Start,
    CurrentTx {
        iter: std::collections::btree_map::Iter<'a, RowId, ProductValue>,
    },
    Committed {
        iter: std::collections::btree_map::Iter<'a, RowId, ProductValue>,
    },
}

impl Iterator for Iter<'_> {
    type Item = DataRef;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.stage {
                ScanStage::Start => {
                    if let Some(table) = self.inner.committed_state.tables.get(&self.table_id) {
                        self.stage = ScanStage::Committed {
                            iter: table.rows.iter(),
                        };
                    } else if let Some(table) = self.inner.tx_state.as_ref().unwrap().insert_tables.get(&self.table_id)
                    {
                        self.stage = ScanStage::CurrentTx {
                            iter: table.rows.iter(),
                        };
                    };
                }
                ScanStage::Committed { iter } => {
                    for (row_id, row) in iter {
                        match self
                            .inner
                            .tx_state
                            .as_ref()
                            .map(|tx_state| tx_state.get_row_op(&self.table_id, row_id))
                        {
                            Some(RowState::Committed(_)) => unreachable!("a row cannot be committed in a tx state"),
                            Some(RowState::Insert(_)) => (), // Do nothing, we'll get it in the next stage
                            Some(RowState::Delete) => (),    // Skip it, it's been deleted
                            Some(RowState::Absent) => {
                                return Some(DataRef::new(row.clone()));
                            }
                            None => {
                                return Some(DataRef::new(row.clone()));
                            }
                        }
                    }
                    if let Some(table) = self
                        .inner
                        .tx_state
                        .as_ref()
                        .and_then(|tx_state| tx_state.insert_tables.get(&self.table_id))
                    {
                        self.stage = ScanStage::CurrentTx {
                            iter: table.rows.iter(),
                        };
                    } else {
                        break;
                    }
                }
                ScanStage::CurrentTx { iter } => {
                    if let Some((_, row)) = iter.next() {
                        return Some(DataRef::new(row.clone()));
                    }
                    break;
                }
            }
        }
        None
    }
}

/// An iterator returned from `iter_by_col_eq`. This yields up all
/// rows in a table which have a column with a particular value.
pub enum IterByColEq<'a> {
    /// When the column in question does not have an index.
    Scan(ScanIterByColEq<'a>),

    /// When the column has an index, and the table
    /// has been modified this transaction.
    Index(IndexIterByColEq<'a>),

    /// When the column has an index, and the table
    /// has not been modified in this transaction.
    CommittedIndex(CommittedIndexIterByColEq<'a>),
}

impl Iterator for IterByColEq<'_> {
    type Item = DataRef;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IterByColEq::Scan(seek) => seek.next(),
            IterByColEq::Index(seek) => seek.next(),
            IterByColEq::CommittedIndex(seek) => seek.next(),
        }
    }
}

pub struct ScanIterByColEq<'a> {
    scan_iter: Iter<'a>,
    col_id: ColId,
    value: &'a AlgebraicValue,
}

impl Iterator for ScanIterByColEq<'_> {
    type Item = DataRef;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        for data_ref in &mut self.scan_iter {
            let row = data_ref.view();
            let value = &row.elements[self.col_id.0 as usize];
            if self.value == value {
                return Some(data_ref);
            }
        }
        None
    }
}

pub struct IndexIterByColEq<'a> {
    iter: IndexSeekIterInner<'a>,
    col_id: ColId,
    value: &'a AlgebraicValue,
}

impl Iterator for IndexIterByColEq<'_> {
    type Item = DataRef;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.find(|data_ref| {
            let row = data_ref.view();
            let value = &row.elements[self.col_id.0 as usize];
            self.value == value
        })
    }
}

struct IndexSeekIterInner<'a> {
    table_id: TableId,
    tx_state: &'a TxState,
    committed_state: &'a CommittedState,
    inserted_rows: BTreeIndexRangeIter<'a>,
    committed_rows: Option<BTreeIndexRangeIter<'a>>,
}

impl Iterator for IndexSeekIterInner<'_> {
    type Item = DataRef;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row_id) = self.inserted_rows.next() {
            return Some(DataRef::new(
                self.tx_state.get_row(&self.table_id, &row_id).unwrap().clone(),
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
            return Some(get_committed_row(self.committed_state, &self.table_id, &row_id));
        }

        None
    }
}

pub struct CommittedIndexIterByColEq<'a> {
    table_id: TableId,
    tx_state: &'a TxState,
    committed_state: &'a CommittedState,
    committed_rows: BTreeIndexRangeIter<'a>,
}

impl Iterator for CommittedIndexIterByColEq<'_> {
    type Item = DataRef;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row_id) = self.committed_rows.find(|row_id| {
            !self
                .tx_state
                .delete_tables
                .get(&self.table_id)
                .map_or(false, |table| table.contains(row_id))
        }) {
            return Some(get_committed_row(self.committed_state, &self.table_id, &row_id));
        }

        None
    }
}

/// Retrieve a commited row. Panics if `table_id` and `row_id` do not identify an actually
/// present row.
fn get_committed_row(state: &CommittedState, table_id: &TableId, row_id: &RowId) -> DataRef {
    DataRef::new(state.tables.get(table_id).unwrap().get_row(row_id).unwrap().clone())
}

pub enum IterByColRange<'a, R: RangeBounds<AlgebraicValue>> {
    Scan(ScanIterByColRange<'a, R>),
    // TODO: Index(IndexRangeScanIter<'a>),
}

impl<R: RangeBounds<AlgebraicValue>> Iterator for IterByColRange<'_, R> {
    type Item = DataRef;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IterByColRange::Scan(range) => range.next(),
            // TODO: RangeScanIter::Index(range) => range.next(),
        }
    }
}

pub struct ScanIterByColRange<'a, R: RangeBounds<AlgebraicValue>> {
    scan_iter: Iter<'a>,
    col_id: ColId,
    range: R,
}

impl<R: RangeBounds<AlgebraicValue>> Iterator for ScanIterByColRange<'_, R> {
    type Item = DataRef;

    fn next(&mut self) -> Option<Self::Item> {
        for data_ref in &mut self.scan_iter {
            let row = data_ref.view();
            let value = &row.elements[self.col_id.0 as usize];
            if self.range.contains(value) {
                return Some(data_ref);
            }
        }
        None
    }
}

impl TxDatastore for Locking {
    type Iter<'a> = Iter<'a> where Self: 'a;
    type IterByColRange<'a, R: std::ops::RangeBounds<spacetimedb_sats::AlgebraicValue>> = IterByColRange<'a, R> where Self: 'a;
    type IterByColEq<'a> = IterByColEq<'a> where Self: 'a;

    fn iter_tx<'a>(&'a self, tx: &'a Self::TxId, table_id: TableId) -> super::Result<Self::Iter<'a>> {
        self.iter_mut_tx(tx, table_id)
    }

    fn iter_by_col_range_tx<'a, R: std::ops::RangeBounds<spacetimedb_sats::AlgebraicValue>>(
        &'a self,
        tx: &'a Self::TxId,
        table_id: TableId,
        col_id: ColId,
        range: R,
    ) -> super::Result<Self::IterByColRange<'a, R>> {
        self.iter_by_col_range_mut_tx(tx, table_id, col_id, range)
    }

    fn iter_by_col_eq_tx<'a>(
        &'a self,
        tx: &'a Self::TxId,
        table_id: TableId,
        col_id: ColId,
        value: &'a spacetimedb_sats::AlgebraicValue,
    ) -> super::Result<Self::IterByColEq<'a>> {
        self.iter_by_col_eq_mut_tx(tx, table_id, col_id, value)
    }

    fn get_tx<'a>(
        &'a self,
        tx: &'a Self::TxId,
        table_id: TableId,
        row_id: Self::RowId,
    ) -> super::Result<Option<Self::DataRef>> {
        self.get_mut_tx(tx, table_id, row_id)
    }
}

impl traits::MutTx for Locking {
    type MutTxId = MutTxId;

    fn begin_mut_tx(&self) -> Self::MutTxId {
        let mut inner = self.inner.lock_arc();
        if inner.tx_state.is_some() {
            panic!("The previous transaction was not properly rolled back or committed.");
        }
        inner.tx_state = Some(TxState::new());
        MutTxId { lock: inner }
    }

    fn rollback_mut_tx(&self, mut tx: Self::MutTxId) {
        tx.lock.rollback();
    }

    fn commit_mut_tx(&self, mut tx: Self::MutTxId) -> super::Result<Option<TxData>> {
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
    fn row_type_for_table_mut_tx(&self, tx: &Self::MutTxId, table_id: TableId) -> super::Result<ProductType> {
        tx.lock.row_type_for_table(table_id)
    }

    /// IMPORTANT! This function is relatively expensive, and much more
    /// expensive than `row_type_for_table_mut_tx`.  Prefer
    /// `row_type_for_table_mut_tx` if you only need to access the `ProductType`
    /// of the table.
    fn schema_for_table_mut_tx(&self, tx: &Self::MutTxId, table_id: TableId) -> super::Result<TableSchema> {
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

    fn table_name_from_id_mut_tx(&self, tx: &Self::MutTxId, table_id: TableId) -> super::Result<Option<String>> {
        tx.lock.table_name_from_id(table_id)
    }

    fn create_index_mut_tx(&self, tx: &mut Self::MutTxId, index: IndexDef) -> super::Result<IndexId> {
        tx.lock.create_index(index)
    }

    fn drop_index_mut_tx(&self, tx: &mut Self::MutTxId, index_id: IndexId) -> super::Result<()> {
        tx.lock.drop_index(&index_id)
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

    fn iter_mut_tx<'a>(&'a self, tx: &'a Self::MutTxId, table_id: TableId) -> super::Result<Self::Iter<'a>> {
        tx.lock.iter(&table_id)
    }

    fn iter_by_col_range_mut_tx<'a, R: std::ops::RangeBounds<spacetimedb_sats::AlgebraicValue>>(
        &'a self,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        col_id: ColId,
        range: R,
    ) -> super::Result<Self::IterByColRange<'a, R>> {
        tx.lock.iter_by_col_range(&table_id, &col_id, range)
    }

    fn iter_by_col_eq_mut_tx<'a>(
        &'a self,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        col_id: ColId,
        value: &'a spacetimedb_sats::AlgebraicValue,
    ) -> super::Result<Self::IterByColEq<'a>> {
        tx.lock.iter_by_col_eq(&table_id, &col_id, value)
    }

    fn get_mut_tx<'a>(
        &'a self,
        tx: &'a Self::MutTxId,
        table_id: TableId,
        row_id: Self::RowId,
    ) -> super::Result<Option<Self::DataRef>> {
        tx.lock.get(&table_id, &row_id)
    }

    fn delete_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTxId,
        table_id: TableId,
        row_id: Self::RowId,
    ) -> super::Result<bool> {
        tx.lock.delete(&table_id, &row_id)
    }

    fn delete_by_rel_mut_tx<R: IntoIterator<Item = spacetimedb_sats::ProductValue>>(
        &self,
        tx: &mut Self::MutTxId,
        table_id: TableId,
        relation: R,
    ) -> super::Result<Option<u32>> {
        tx.lock.delete_by_rel(&table_id, relation)
    }

    fn insert_mut_tx<'a>(
        &'a self,
        tx: &'a mut Self::MutTxId,
        table_id: TableId,
        row: spacetimedb_sats::ProductValue,
    ) -> super::Result<ProductValue> {
        tx.lock.insert(table_id, row)
    }
}

#[cfg(test)]
mod tests {
    use super::{ColId, Locking, StTableRow};
    use crate::db::datastore::system_tables::{StConstraintRow, ST_CONSTRAINTS_ID};
    use crate::{
        db::datastore::{
            locking_tx_datastore::{
                StColumnRow, StIndexRow, StSequenceRow, ST_COLUMNS_ID, ST_INDEXES_ID, ST_SEQUENCES_ID, ST_TABLES_ID,
            },
            traits::{
                ColumnDef, ColumnSchema, DataRow, IndexDef, IndexSchema, MutTx, MutTxDatastore, TableDef, TableSchema,
            },
        },
        error::{DBError, IndexError},
    };
    use itertools::Itertools;
    use spacetimedb_lib::{
        auth::{StAccess, StTableType},
        error::ResultTest,
        ColumnIndexAttribute,
    };
    use spacetimedb_sats::{AlgebraicType, AlgebraicValue, ProductValue};

    fn get_datastore() -> super::super::Result<Locking> {
        Locking::bootstrap()
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
                IndexDef {
                    table_id: 0, // Ignored
                    col_id: 0,
                    name: "id_idx".into(),
                    is_unique: true,
                },
                IndexDef {
                    table_id: 0, // Ignored
                    col_id: 1,
                    name: "name_idx".into(),
                    is_unique: true,
                },
            ],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        }
    }

    #[test]
    fn test_bootstrapping_sets_up_tables() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let tx = datastore.begin_mut_tx();
        let table_rows = datastore
            .iter_mut_tx(&tx, ST_TABLES_ID)?
            .map(|x| StTableRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.table_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            table_rows,
            vec![
                StTableRow { table_id: 0, table_name: "st_table".to_string(), table_type: StTableType::System, table_access: StAccess::Public },
                StTableRow { table_id: 1, table_name: "st_columns".to_string(), table_type: StTableType::System, table_access: StAccess::Public },
                StTableRow { table_id: 2, table_name: "st_sequence".to_string(), table_type: StTableType::System, table_access: StAccess::Public},
                StTableRow { table_id: 3, table_name: "st_indexes".to_string() , table_type: StTableType::System, table_access: StAccess::Public},
                StTableRow { table_id: 4, table_name: "st_constraints".to_string() , table_type: StTableType::System, table_access: StAccess::Public},
            ]
        );
        let column_rows = datastore
            .iter_mut_tx(&tx, ST_COLUMNS_ID)?
            .map(|x| StColumnRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| (x.table_id, x.col_id))
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            column_rows,
            vec![
                StColumnRow { table_id: 0, col_id: 0, col_name: "table_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: true },
                StColumnRow { table_id: 0, col_id: 1, col_name: "table_name".to_string(), col_type: AlgebraicType::String, is_autoinc: false },
                StColumnRow { table_id: 0, col_id: 2, col_name: "table_type".to_string(), col_type: AlgebraicType::String, is_autoinc: false },
                StColumnRow { table_id: 0, col_id: 3, col_name: "table_access".to_string(), col_type: AlgebraicType::String, is_autoinc: false },

                StColumnRow { table_id: 1, col_id: 0, col_name: "table_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
                StColumnRow { table_id: 1, col_id: 1, col_name: "col_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
                StColumnRow { table_id: 1, col_id: 2, col_name: "col_type".to_string(), col_type: AlgebraicType::array(AlgebraicType::U8), is_autoinc: false },
                StColumnRow { table_id: 1, col_id: 3, col_name: "col_name".to_string(), col_type: AlgebraicType::String, is_autoinc: false },
                StColumnRow { table_id: 1, col_id: 4, col_name: "is_autoinc".to_string(), col_type: AlgebraicType::Bool, is_autoinc: false },

                StColumnRow { table_id: 2, col_id: 0, col_name: "sequence_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: true },
                StColumnRow { table_id: 2, col_id: 1, col_name: "sequence_name".to_string(), col_type: AlgebraicType::String, is_autoinc: false },
                StColumnRow { table_id: 2, col_id: 2, col_name: "table_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
                StColumnRow { table_id: 2, col_id: 3, col_name: "col_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
                StColumnRow { table_id: 2, col_id: 4, col_name: "increment".to_string(), col_type: AlgebraicType::I128, is_autoinc: false },
                StColumnRow { table_id: 2, col_id: 5, col_name: "start".to_string(), col_type: AlgebraicType::I128, is_autoinc: false },
                StColumnRow { table_id: 2, col_id: 6, col_name: "min_value".to_string(), col_type: AlgebraicType::I128, is_autoinc: false },
                StColumnRow { table_id: 2, col_id: 7, col_name: "max_malue".to_string(), col_type: AlgebraicType::I128, is_autoinc: false },
                StColumnRow { table_id: 2, col_id: 8, col_name: "allocated".to_string(), col_type: AlgebraicType::I128, is_autoinc: false },

                StColumnRow { table_id: 3, col_id: 0, col_name: "index_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: true },
                StColumnRow { table_id: 3, col_id: 1, col_name: "table_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
                StColumnRow { table_id: 3, col_id: 2, col_name: "col_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
                StColumnRow { table_id: 3, col_id: 3, col_name: "index_name".to_string(), col_type: AlgebraicType::String, is_autoinc: false },
                StColumnRow { table_id: 3, col_id: 4, col_name: "is_unique".to_string(), col_type: AlgebraicType::Bool, is_autoinc: false },

                StColumnRow { table_id: 4, col_id: 0, col_name: "constraint_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: true },
                StColumnRow { table_id: 4, col_id: 1, col_name: "constraint_name".to_string(), col_type: AlgebraicType::String, is_autoinc: false },
                StColumnRow { table_id: 4, col_id: 2, col_name: "kind".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
                StColumnRow { table_id: 4, col_id: 3, col_name: "table_id".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
                StColumnRow { table_id: 4, col_id: 4, col_name: "columns".to_string(), col_type: AlgebraicType::array(AlgebraicType::U32), is_autoinc: false },
            ]
        );
        let index_rows = datastore
            .iter_mut_tx(&tx, ST_INDEXES_ID)?
            .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.index_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            index_rows,
            vec![
                StIndexRow { index_id: 0, table_id: 0, col_id: 0, index_name: "table_id_idx".to_string(), is_unique: true },
                StIndexRow { index_id: 1, table_id: 3, col_id: 0, index_name: "index_id_idx".to_string(), is_unique: true },
                StIndexRow { index_id: 2, table_id: 2, col_id: 0, index_name: "sequences_id_idx".to_string(), is_unique: true },
                StIndexRow { index_id: 3, table_id: 0, col_id: 1, index_name: "table_name_idx".to_string(), is_unique: true },
                StIndexRow { index_id: 4, table_id: 4, col_id: 0, index_name: "constraint_id_idx".to_string(), is_unique: true },
                StIndexRow { index_id: 5, table_id: 1, col_id: 0, index_name: "idx_ct_columns_table_id".to_string(), is_unique: false }
            ]
        );
        let sequence_rows = datastore
            .iter_mut_tx(&tx, ST_SEQUENCES_ID)?
            .map(|x| StSequenceRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.sequence_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            sequence_rows,
            vec![
                StSequenceRow { sequence_id: 0, sequence_name: "table_id_seq".to_string(), table_id: 0, col_id: 0, increment: 1, start: 5, min_value: 1, max_value: 4294967295, allocated: 4096 },
                StSequenceRow { sequence_id: 1, sequence_name: "sequence_id_seq".to_string(), table_id: 2, col_id: 0, increment: 1, start: 4, min_value: 1, max_value: 4294967295, allocated: 4096 },
                StSequenceRow { sequence_id: 2, sequence_name: "index_id_seq".to_string(), table_id: 3, col_id: 0, increment: 1, start: 6, min_value: 1, max_value: 4294967295, allocated: 4096 },
                StSequenceRow { sequence_id: 3, sequence_name: "constraint_id_seq".to_string(), table_id: 4, col_id: 0, increment: 1, start: 1, min_value: 1, max_value: 4294967295, allocated: 4096 },
            ]
        );
        let constraints_rows = datastore
            .iter_mut_tx(&tx, ST_CONSTRAINTS_ID)?
            .map(|x| StConstraintRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.constraint_id)
            .collect::<Vec<_>>();

        #[rustfmt::skip]
        assert_eq!(
            constraints_rows,
            vec![
                StConstraintRow{ constraint_id: 5, constraint_name: "ct_columns_table_id".to_string(), kind:  ColumnIndexAttribute::INDEXED, table_id: 1, columns: vec![0] },
            ]
        );
        datastore.rollback_mut_tx(tx);
        Ok(())
    }

    #[test]
    fn test_create_table_pre_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let table_rows = datastore
            .iter_by_col_eq_mut_tx(&tx, ST_TABLES_ID, ColId(0), &AlgebraicValue::U32(table_id.0))?
            .map(|x| StTableRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.table_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            table_rows,
            vec![
                StTableRow { table_id: 5, table_name: "Foo".to_string(), table_type: StTableType::User, table_access: StAccess::Public }
            ]
        );
        let column_rows = datastore
            .iter_by_col_eq_mut_tx(&tx, ST_COLUMNS_ID, ColId(0), &AlgebraicValue::U32(table_id.0))?
            .map(|x| StColumnRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| (x.table_id, x.col_id))
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            column_rows,
            vec![
                StColumnRow { table_id: 5, col_id: 0, col_name: "id".to_string(), col_type: AlgebraicType::U32, is_autoinc: true },
                StColumnRow { table_id: 5, col_id: 1, col_name: "name".to_string(), col_type: AlgebraicType::String, is_autoinc: false },
                StColumnRow { table_id: 5, col_id: 2, col_name: "age".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
            ]
        );
        Ok(())
    }

    #[test]
    fn test_create_table_post_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        datastore.commit_mut_tx(tx)?;
        let tx = datastore.begin_mut_tx();
        let table_rows = datastore
            .iter_by_col_eq_mut_tx(&tx, ST_TABLES_ID, ColId(0), &AlgebraicValue::U32(table_id.0))?
            .map(|x| StTableRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.table_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            table_rows,
            vec![
                StTableRow { table_id: 5, table_name: "Foo".to_string() , table_type: StTableType::User, table_access: StAccess::Public}
            ]
        );
        let column_rows = datastore
            .iter_by_col_eq_mut_tx(&tx, ST_COLUMNS_ID, ColId(0), &AlgebraicValue::U32(table_id.0))?
            .map(|x| StColumnRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| (x.table_id, x.col_id))
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(
            column_rows,
            vec![
                StColumnRow { table_id: 5, col_id: 0, col_name: "id".to_string(), col_type: AlgebraicType::U32, is_autoinc: true },
                StColumnRow { table_id: 5, col_id: 1, col_name: "name".to_string(), col_type: AlgebraicType::String, is_autoinc: false },
                StColumnRow { table_id: 5, col_id: 2, col_name: "age".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
            ]
        );
        Ok(())
    }

    #[test]
    fn test_create_table_post_rollback() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        datastore.rollback_mut_tx(tx);
        let tx = datastore.begin_mut_tx();
        let table_rows = datastore
            .iter_by_col_eq_mut_tx(&tx, ST_TABLES_ID, ColId(0), &AlgebraicValue::U32(table_id.0))?
            .map(|x| StTableRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.table_id)
            .collect::<Vec<_>>();
        assert_eq!(table_rows, vec![]);
        let column_rows = datastore
            .iter_by_col_eq_mut_tx(&tx, ST_COLUMNS_ID, ColId(0), &AlgebraicValue::U32(table_id.0))?
            .map(|x| StColumnRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.table_id)
            .collect::<Vec<_>>();
        assert_eq!(column_rows, vec![]);
        Ok(())
    }

    #[test]
    fn test_schema_for_table_pre_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let schema = datastore.schema_for_table_mut_tx(&tx, table_id)?;
        #[rustfmt::skip]
        assert_eq!(schema, TableSchema {
            table_id: table_id.0,
            table_name: "Foo".into(),
            columns: vec![
                ColumnSchema { table_id: 5, col_id: 0, col_name: "id".to_string(), col_type: AlgebraicType::U32, is_autoinc: true },
                ColumnSchema { table_id: 5, col_id: 1, col_name: "name".to_string(), col_type: AlgebraicType::String, is_autoinc: false },
                ColumnSchema { table_id: 5, col_id: 2, col_name: "age".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
            ],
            indexes: vec![
                IndexSchema { index_id: 6, table_id: 5, col_id: 0, index_name: "id_idx".to_string(), is_unique: true },
                IndexSchema { index_id: 7, table_id: 5, col_id: 1, index_name: "name_idx".to_string(), is_unique: true },
            ],
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        });
        Ok(())
    }

    #[test]
    fn test_schema_for_table_post_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        datastore.commit_mut_tx(tx)?;
        let tx = datastore.begin_mut_tx();
        let schema = datastore.schema_for_table_mut_tx(&tx, table_id)?;
        #[rustfmt::skip]
        assert_eq!(schema, TableSchema {
            table_id: table_id.0,
            table_name: "Foo".into(),
            columns: vec![
                ColumnSchema { table_id: 5, col_id: 0, col_name: "id".to_string(), col_type: AlgebraicType::U32, is_autoinc: true },
                ColumnSchema { table_id: 5, col_id: 1, col_name: "name".to_string(), col_type: AlgebraicType::String, is_autoinc: false },
                ColumnSchema { table_id: 5, col_id: 2, col_name: "age".to_string(), col_type: AlgebraicType::U32, is_autoinc: false },
            ],
            indexes: vec![
                IndexSchema { index_id: 6, table_id: 5, col_id: 0, index_name: "id_idx".to_string(), is_unique: true },
                IndexSchema { index_id: 7, table_id: 5, col_id: 1, index_name: "name_idx".to_string(), is_unique: true },
            ],
            constraints: vec![],
            table_type: StTableType::User,
            table_access: StAccess::Public,
        });
        Ok(())
    }

    #[test]
    fn test_schema_for_table_rollback() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        datastore.rollback_mut_tx(tx);
        let tx = datastore.begin_mut_tx();
        let schema = datastore.schema_for_table_mut_tx(&tx, table_id);
        assert!(schema.is_err());
        Ok(())
    }

    #[test]
    fn test_insert_pre_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![
            ProductValue::from_iter(vec![
                AlgebraicValue::U32(1),
                AlgebraicValue::String("Foo".to_string()),
                AlgebraicValue::U32(18),
            ])
        ]);
        Ok(())
    }

    #[test]
    fn test_insert_wrong_schema_pre_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
        ]);
        assert!(datastore.insert_mut_tx(&mut tx, table_id, row).is_err());
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![]);
        Ok(())
    }

    #[test]
    fn test_insert_post_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx(tx)?;
        let tx = datastore.begin_mut_tx();
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![
            ProductValue::from_iter(vec![
                AlgebraicValue::U32(1),
                AlgebraicValue::String("Foo".to_string()),
                AlgebraicValue::U32(18),
            ])
        ]);
        Ok(())
    }

    #[test]
    fn test_insert_post_rollback() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(15), // A number which will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx();
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.rollback_mut_tx(tx);
        let tx = datastore.begin_mut_tx();
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![]);
        Ok(())
    }

    #[test]
    fn test_insert_commit_delete_insert() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let created_row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(1),
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        let num_deleted = datastore.delete_by_rel_mut_tx(&mut tx, table_id, vec![created_row])?;
        assert_eq!(num_deleted, Some(1));
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 0);
        let created_row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(1),
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(19),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, created_row)?;
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![
            ProductValue::from_iter(vec![
                AlgebraicValue::U32(1),
                AlgebraicValue::String("Foo".to_string()),
                AlgebraicValue::U32(19),
            ])
        ]);
        Ok(())
    }

    #[test]
    fn test_insert_delete_insert_delete_insert() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        for _ in 0..2 {
            let created_row = ProductValue::from_iter(vec![
                AlgebraicValue::U32(1),
                AlgebraicValue::String("Foo".to_string()),
                AlgebraicValue::U32(18),
            ]);
            let num_deleted = datastore.delete_by_rel_mut_tx(&mut tx, table_id, vec![created_row.clone()])?;
            assert_eq!(num_deleted, Some(1));
            let rows = datastore
                .iter_mut_tx(&tx, table_id)?
                .map(|r| r.view().clone())
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 0);
            datastore.insert_mut_tx(&mut tx, table_id, created_row)?;
            let rows = datastore
                .iter_mut_tx(&tx, table_id)?
                .map(|r| r.view().clone())
                .collect::<Vec<_>>();
            #[rustfmt::skip]
            assert_eq!(rows, vec![
                ProductValue::from_iter(vec![
                    AlgebraicValue::U32(1),
                    AlgebraicValue::String("Foo".to_string()),
                    AlgebraicValue::U32(18),
                ])
            ]);
        }
        Ok(())
    }

    #[test]
    fn test_unique_constraint_pre_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                col_name: _,
                value: _,
            })) => (),
            _ => panic!("Expected an unique constraint violation error."),
        }
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![
            ProductValue::from_iter(vec![
                AlgebraicValue::U32(1),
                AlgebraicValue::String("Foo".to_string()),
                AlgebraicValue::U32(18),
            ])
        ]);
        Ok(())
    }

    #[test]
    fn test_unique_constraint_post_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                col_name: _,
                value: _,
            })) => (),
            _ => panic!("Expected an unique constraint violation error."),
        }
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![
            ProductValue::from_iter(vec![
                AlgebraicValue::U32(1),
                AlgebraicValue::String("Foo".to_string()),
                AlgebraicValue::U32(18),
            ])
        ]);
        Ok(())
    }

    #[test]
    fn test_unique_constraint_post_rollback() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
        datastore.rollback_mut_tx(tx);
        let mut tx = datastore.begin_mut_tx();
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![
            ProductValue::from_iter(vec![
                AlgebraicValue::U32(2),
                AlgebraicValue::String("Foo".to_string()),
                AlgebraicValue::U32(18),
            ])
        ]);
        Ok(())
    }

    #[test]
    fn test_create_index_pre_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let index_def = IndexDef {
            col_id: 2,
            name: "age_idx".to_string(),
            is_unique: true,
            table_id: table_id.0,
        };
        datastore.create_index_mut_tx(&mut tx, index_def)?;
        let index_rows = datastore
            .iter_mut_tx(&tx, ST_INDEXES_ID)?
            .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.index_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(index_rows, vec![
            StIndexRow { index_id: 0, table_id: 0, col_id: 0, index_name: "table_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 1, table_id: 3, col_id: 0, index_name: "index_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 2, table_id: 2, col_id: 0, index_name: "sequences_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 3, table_id: 0, col_id: 1, index_name: "table_name_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 4, table_id: 4, col_id: 0, index_name: "constraint_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 5, table_id: 1, col_id: 0, index_name: "idx_ct_columns_table_id".to_string(), is_unique: false },
            StIndexRow { index_id: 6, table_id: 5, col_id: 0, index_name: "id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 7, table_id: 5, col_id: 1, index_name: "name_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 8, table_id: 5, col_id: 2, index_name: "age_idx".to_string(), is_unique: true },
        ]);
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Bar".to_string()),
            AlgebraicValue::U32(18),
        ]);
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                col_name: _,
                value: _,
            })) => (),
            _ => panic!("Expected an unique constraint violation error."),
        }
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![
            ProductValue::from_iter(vec![
                AlgebraicValue::U32(1),
                AlgebraicValue::String("Foo".to_string()),
                AlgebraicValue::U32(18),
            ])
        ]);
        Ok(())
    }

    #[test]
    fn test_create_index_post_commit() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let index_def = IndexDef {
            table_id: table_id.0,
            col_id: 2,
            name: "age_idx".to_string(),
            is_unique: true,
        };
        datastore.create_index_mut_tx(&mut tx, index_def)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let index_rows = datastore
            .iter_mut_tx(&tx, ST_INDEXES_ID)?
            .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.index_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(index_rows, vec![
            StIndexRow { index_id: 0, table_id: 0, col_id: 0, index_name: "table_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 1, table_id: 3, col_id: 0, index_name: "index_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 2, table_id: 2, col_id: 0, index_name: "sequences_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 3, table_id: 0, col_id: 1, index_name: "table_name_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 4, table_id: 4, col_id: 0, index_name: "constraint_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 5, table_id: 1, col_id: 0, index_name: "idx_ct_columns_table_id".to_string(), is_unique: false },
            StIndexRow { index_id: 6, table_id: 5, col_id: 0, index_name: "id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 7, table_id: 5, col_id: 1, index_name: "name_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 8, table_id: 5, col_id: 2, index_name: "age_idx".to_string(), is_unique: true },
        ]);
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Bar".to_string()),
            AlgebraicValue::U32(18),
        ]);
        let result = datastore.insert_mut_tx(&mut tx, table_id, row);
        match result {
            Err(DBError::Index(IndexError::UniqueConstraintViolation {
                constraint_name: _,
                table_name: _,
                col_name: _,
                value: _,
            })) => (),
            _ => panic!("Expected an unique constraint violation error."),
        }
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![
            ProductValue::from_iter(vec![
                AlgebraicValue::U32(1),
                AlgebraicValue::String("Foo".to_string()),
                AlgebraicValue::U32(18),
            ])
        ]);
        Ok(())
    }

    #[test]
    fn test_create_index_post_rollback() -> ResultTest<()> {
        let datastore = get_datastore()?;
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx(tx)?;
        let mut tx = datastore.begin_mut_tx();
        let index_def = IndexDef {
            col_id: 2,
            name: "age_idx".to_string(),
            is_unique: true,
            table_id: table_id.0,
        };
        datastore.create_index_mut_tx(&mut tx, index_def)?;
        datastore.rollback_mut_tx(tx);
        let mut tx = datastore.begin_mut_tx();
        let index_rows = datastore
            .iter_mut_tx(&tx, ST_INDEXES_ID)?
            .map(|x| StIndexRow::try_from(x.view()).unwrap().to_owned())
            .sorted_by_key(|x| x.index_id)
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(index_rows, vec![
            StIndexRow { index_id: 0, table_id: 0, col_id: 0, index_name: "table_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 1, table_id: 3, col_id: 0, index_name: "index_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 2, table_id: 2, col_id: 0, index_name: "sequences_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 3, table_id: 0, col_id: 1, index_name: "table_name_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 4, table_id: 4, col_id: 0, index_name: "constraint_id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 5, table_id: 1, col_id: 0, index_name: "idx_ct_columns_table_id".to_string(), is_unique: false },
            StIndexRow { index_id: 6, table_id: 5, col_id: 0, index_name: "id_idx".to_string(), is_unique: true },
            StIndexRow { index_id: 7, table_id: 5, col_id: 1, index_name: "name_idx".to_string(), is_unique: true },
        ]);
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Bar".to_string()),
            AlgebraicValue::U32(18),
        ]);
        datastore.insert_mut_tx(&mut tx, table_id, row)?;
        let rows = datastore
            .iter_mut_tx(&tx, table_id)?
            .map(|r| r.view().clone())
            .collect::<Vec<_>>();
        #[rustfmt::skip]
        assert_eq!(rows, vec![
            ProductValue::from_iter(vec![
                AlgebraicValue::U32(1),
                AlgebraicValue::String("Foo".to_string()),
                AlgebraicValue::U32(18),
            ]),
            ProductValue::from_iter(vec![
                AlgebraicValue::U32(2),
                AlgebraicValue::String("Bar".to_string()),
                AlgebraicValue::U32(18),
            ])
        ]);
        Ok(())
    }

    #[test]
    fn test_update_reinsert() -> ResultTest<()> {
        let datastore = get_datastore()?;

        // Insert a row and commit the tx.
        let mut tx = datastore.begin_mut_tx();
        let schema = basic_table_schema();
        let table_id = datastore.create_table_mut_tx(&mut tx, schema)?;
        let row = ProductValue::from_iter(vec![
            AlgebraicValue::U32(0), // 0 will be ignored.
            AlgebraicValue::String("Foo".to_string()),
            AlgebraicValue::U32(18),
        ]);
        // Because of autoinc columns, we will get a slightly different
        // value than the one we inserted.
        let row = datastore.insert_mut_tx(&mut tx, table_id, row)?;
        datastore.commit_mut_tx(tx)?;

        // Update the db with the same actual value for that row, in a new tx.
        let mut tx = datastore.begin_mut_tx();
        // Iterate over all rows with the value 1 (from the autoinc) in column 0.
        let rows = datastore
            .iter_by_col_eq_mut_tx(&tx, table_id, ColId(0), &AlgebraicValue::U32(1))?
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);
        let rows: Vec<ProductValue> = rows
            .into_iter()
            .map(|row| datastore.data_to_owned(row).into())
            .collect();
        assert_eq!(row, rows[0]);
        // Delete the row.
        let count_deleted = datastore.delete_by_rel_mut_tx(&mut tx, table_id, rows)?;
        assert_eq!(count_deleted, Some(1));

        // We shouldn't see the row when iterating now that it's deleted.
        let rows = datastore
            .iter_by_col_eq_mut_tx(&tx, table_id, ColId(0), &AlgebraicValue::U32(1))?
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 0);

        // Reinsert the row.
        let reinserted_row = datastore.insert_mut_tx(&mut tx, table_id, row.clone())?;
        assert_eq!(reinserted_row, row);

        // The actual test: we should be able to iterate again, while still in the
        // second transaction, and see exactly one row.
        let rows = datastore
            .iter_by_col_eq_mut_tx(&tx, table_id, ColId(0), &AlgebraicValue::U32(1))?
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1);

        datastore.commit_mut_tx(tx)?;

        Ok(())
    }

    // TODO: Add the following tests
    // - Create index with unique constraint and immediately insert a row that violates the constraint before committing.
    // - Create a tx that inserts 2000 rows with an autoinc column
    // - Create a tx that inserts 2000 rows with an autoinc column and then rolls back
    // - Test creating sequences pre_commit, post_commit, post_rollback
}
