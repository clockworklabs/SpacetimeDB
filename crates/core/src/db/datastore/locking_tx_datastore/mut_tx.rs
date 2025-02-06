use super::{
    committed_state::CommittedState,
    datastore::{record_tx_metrics, Result},
    delete_table::DeleteTable,
    sequence::{Sequence, SequencesState},
    state_view::{IndexSeekIterIdMutTx, ScanIterByColRangeMutTx, StateView},
    tx::TxId,
    tx_state::{IndexIdMap, TxState},
    SharedMutexGuard, SharedWriteGuard,
};
use crate::db::datastore::system_tables::{
    with_sys_table_buf, StColumnFields, StColumnRow, StConstraintFields, StConstraintRow, StFields as _, StIndexFields,
    StIndexRow, StRowLevelSecurityFields, StRowLevelSecurityRow, StScheduledFields, StScheduledRow, StSequenceFields,
    StSequenceRow, StTableFields, StTableRow, SystemTable, ST_COLUMN_ID, ST_CONSTRAINT_ID, ST_INDEX_ID,
    ST_ROW_LEVEL_SECURITY_ID, ST_SCHEDULED_ID, ST_SEQUENCE_ID, ST_TABLE_ID,
};
use crate::db::datastore::traits::{RowTypeForTable, TxData};
use crate::db::datastore::{
    locking_tx_datastore::committed_state::CommittedIndexIterWithDeletedMutTx, traits::InsertFlags,
};
use crate::db::datastore::{
    locking_tx_datastore::state_view::{
        IndexSeekIterIdWithDeletedMutTx, IterByColEqMutTx, IterByColRangeMutTx, IterMutTx,
    },
    traits::UpdateFlags,
};
use crate::execution_context::Workload;
use crate::{
    error::{IndexError, SequenceError, TableError},
    execution_context::ExecutionContext,
};
use core::cell::RefCell;
use core::ops::RangeBounds;
use core::{iter, ops::Bound};
use smallvec::SmallVec;
use spacetimedb_execution::{dml::MutDatastore, Datastore, DeltaStore};
use spacetimedb_lib::db::{auth::StAccess, raw_def::SEQUENCE_ALLOCATION_STEP};
use spacetimedb_lib::{db::raw_def::v9::RawSql, metrics::ExecutionMetrics};
use spacetimedb_primitives::{ColId, ColList, ColSet, ConstraintId, IndexId, ScheduleId, SequenceId, TableId};
use spacetimedb_sats::{
    bsatn::{self, to_writer, DecodeError, Deserializer},
    de::{DeserializeSeed, WithBound},
    ser::Serialize,
    AlgebraicType, AlgebraicValue, ProductType, ProductValue, WithTypespace,
};
use spacetimedb_schema::{
    def::{BTreeAlgorithm, DirectAlgorithm, IndexAlgorithm},
    schema::{ConstraintSchema, IndexSchema, RowLevelSecuritySchema, SequenceSchema, TableSchema},
};
use spacetimedb_table::{
    blob_store::{BlobStore, HashMapBlobStore},
    indexes::{RowPointer, SquashedOffset},
    table::{DuplicateError, IndexScanIter, InsertError, RowRef, Table, TableAndIndex},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

type DecodeResult<T> = core::result::Result<T, DecodeError>;

/// Represents a Mutable transaction. Holds locks for its duration
///
/// The initialization of this struct is sensitive because improper
/// handling can lead to deadlocks. Therefore, it is strongly recommended to use
/// `Locking::begin_mut_tx()` for instantiation to ensure safe acquisition of locks.
pub struct MutTxId {
    pub(super) tx_state: TxState,
    pub(super) committed_state_write_lock: SharedWriteGuard<CommittedState>,
    pub(super) sequence_state_lock: SharedMutexGuard<SequencesState>,
    pub(super) lock_wait_time: Duration,
    pub(crate) timer: Instant,
    pub(crate) ctx: ExecutionContext,
    pub(crate) metrics: ExecutionMetrics,
}

impl Datastore for MutTxId {
    fn blob_store(&self) -> &dyn BlobStore {
        &self.committed_state_write_lock.blob_store
    }

    fn table(&self, table_id: TableId) -> Option<&Table> {
        self.committed_state_write_lock.get_table(table_id)
    }
}

/// Note, deltas are evaluated using read-only transactions, not mutable ones.
/// Nevertheless this contract is still required for query evaluation.
impl DeltaStore for MutTxId {
    fn num_inserts(&self, _: TableId) -> usize {
        0
    }

    fn num_deletes(&self, _: TableId) -> usize {
        0
    }

    fn inserts_for_table(&self, _: TableId) -> Option<std::slice::Iter<'_, ProductValue>> {
        None
    }

    fn deletes_for_table(&self, _: TableId) -> Option<std::slice::Iter<'_, ProductValue>> {
        None
    }
}

impl MutDatastore for MutTxId {
    fn insert_product_value(&mut self, table_id: TableId, row: &ProductValue) -> anyhow::Result<()> {
        self.insert_via_serialize_bsatn(table_id, row)?;
        Ok(())
    }

    fn delete_product_value(&mut self, table_id: TableId, row: &ProductValue) -> anyhow::Result<()> {
        self.delete_by_row_value(table_id, row)?;
        Ok(())
    }
}

impl MutTxId {
    fn drop_col_eq(&mut self, table_id: TableId, col_pos: ColId, value: &AlgebraicValue) -> Result<()> {
        let rows = self.iter_by_col_eq(table_id, col_pos, value)?;
        let ptrs_to_delete = rows.map(|row_ref| row_ref.pointer()).collect::<Vec<_>>();
        if ptrs_to_delete.is_empty() {
            return Err(TableError::IdNotFound(SystemTable::st_column, col_pos.0 as _).into());
        }

        for ptr in ptrs_to_delete {
            // TODO(error-handling,bikeshedding): Consider correct failure semantics here.
            // We can't really roll back the operation,
            // but we could conceivably attempt all the deletions rather than stopping at the first error.
            self.delete(table_id, ptr)?;
        }

        Ok(())
    }

    /// Create a table.
    ///
    /// Requires:
    /// - All system IDs in the `table_schema` must be set to `SENTINEL`.
    /// - All names in the `table_schema` must be unique among named entities in the database.
    ///
    /// Ensures:
    /// - An in-memory insert table is created for the transaction, allowing the transaction to insert rows into the table.
    /// - The table metadata is inserted into the system tables.
    /// - The returned ID is unique and not `TableId::SENTINEL`.
    pub fn create_table(&mut self, mut table_schema: TableSchema) -> Result<TableId> {
        if table_schema.table_id != TableId::SENTINEL {
            return Err(anyhow::anyhow!("`table_id` must be `TableId::SENTINEL` in `{:#?}`", table_schema).into());
            // checks for children are performed in the relevant `create_...` functions.
        }

        log::trace!("TABLE CREATING: {}", table_schema.table_name);

        // Insert the table row into `st_tables`
        // NOTE: Because `st_tables` has a unique index on `table_name`, this will
        // fail if the table already exists.
        let row = StTableRow {
            table_id: TableId::SENTINEL,
            table_name: table_schema.table_name[..].into(),
            table_type: table_schema.table_type,
            table_access: table_schema.table_access,
            table_primary_key: table_schema.primary_key.map(Into::into),
        };
        let table_id = self
            .insert_via_serialize_bsatn(ST_TABLE_ID, &row)?
            .1
            .collapse()
            .read_col(StTableFields::TableId)?;

        table_schema.update_table_id(table_id);

        // Generate the full definition of the table, with the generated indexes, constraints, sequences...

        // Insert the columns into `st_columns`
        for col in table_schema.columns() {
            let row = StColumnRow {
                table_id: col.table_id,
                col_pos: col.col_pos,
                col_name: col.col_name.clone(),
                col_type: col.col_type.clone().into(),
            };
            self.insert_via_serialize_bsatn(ST_COLUMN_ID, &row)?;
        }

        let mut schema_internal = table_schema.clone();
        // Remove all indexes, constraints, and sequences from the schema; we will add them back later with correct index_id, ...
        schema_internal.clear_adjacent_schemas();

        // Create the in memory representation of the table
        // NOTE: This should be done before creating the indexes
        // NOTE: This `TableSchema` will be updated when we call `create_...` below.
        //       This allows us to create the indexes, constraints, and sequences with the correct `index_id`, ...
        self.create_table_internal(schema_internal.into());

        // Insert the scheduled table entry into `st_scheduled`
        if let Some(schedule) = table_schema.schedule {
            let row = StScheduledRow {
                table_id: schedule.table_id,
                schedule_id: ScheduleId::SENTINEL,
                schedule_name: schedule.schedule_name,
                reducer_name: schedule.reducer_name,
                at_column: schedule.at_column,
            };
            let id = self
                .insert_via_serialize_bsatn(ST_SCHEDULED_ID, &row)?
                .1
                .collapse()
                .read_col::<ScheduleId>(StScheduledFields::ScheduleId)?;
            let (table, ..) = self.get_or_create_insert_table_mut(table_id)?;
            table.with_mut_schema(|s| s.schedule.as_mut().unwrap().schedule_id = id);
        }

        // Insert constraints into `st_constraints`
        for constraint in table_schema.constraints.iter().cloned() {
            self.create_constraint(constraint)?;
        }

        // Insert sequences into `st_sequences`
        for seq in table_schema.sequences {
            self.create_sequence(seq)?;
        }

        // Create the indexes for the table
        for index in table_schema.indexes {
            let col_set = ColSet::from(index.index_algorithm.columns());
            let is_unique = table_schema
                .constraints
                .iter()
                .any(|c| c.data.unique_columns() == Some(&col_set));
            self.create_index(index, is_unique)?;
        }

        log::trace!("TABLE CREATED: {}, table_id: {table_id}", table_schema.table_name);

        Ok(table_id)
    }

    fn create_table_internal(&mut self, schema: Arc<TableSchema>) {
        self.tx_state
            .insert_tables
            .insert(schema.table_id, Table::new(schema, SquashedOffset::TX_STATE));
    }

    fn get_row_type(&self, table_id: TableId) -> Option<&ProductType> {
        if let Some(row_type) = self
            .tx_state
            .insert_tables
            .get(&table_id)
            .map(|table| table.get_row_type())
        {
            return Some(row_type);
        }
        self.committed_state_write_lock
            .tables
            .get(&table_id)
            .map(|table| table.get_row_type())
    }

    pub fn row_type_for_table(&self, table_id: TableId) -> Result<RowTypeForTable<'_>> {
        // Fetch the `ProductType` from the in memory table if it exists.
        // The `ProductType` is invalidated if the schema of the table changes.
        if let Some(row_type) = self.get_row_type(table_id) {
            return Ok(RowTypeForTable::Ref(row_type));
        }

        // Look up the columns for the table in question.
        // NOTE: This is quite an expensive operation, although we only need
        // to do this in situations where there is not currently an in memory
        // representation of a table. This would happen in situations where
        // we have created the table in the database, but have not yet
        // represented in memory or inserted any rows into it.
        Ok(RowTypeForTable::Arc(self.schema_for_table(table_id)?))
    }

    pub fn drop_table(&mut self, table_id: TableId) -> Result<()> {
        let schema = &*self.schema_for_table(table_id)?;

        for row in &schema.indexes {
            self.drop_index(row.index_id)?;
        }

        for row in &schema.sequences {
            self.drop_sequence(row.sequence_id)?;
        }

        for row in &schema.constraints {
            self.drop_constraint(row.constraint_id)?;
        }

        // Drop the table and their columns
        self.drop_col_eq(ST_TABLE_ID, StTableFields::TableId.col_id(), &table_id.into())?;
        self.drop_col_eq(ST_COLUMN_ID, StColumnFields::TableId.col_id(), &table_id.into())?;

        if let Some(schedule) = &schema.schedule {
            self.drop_col_eq(
                ST_SCHEDULED_ID,
                StScheduledFields::ScheduleId.col_id(),
                &schedule.schedule_id.into(),
            )?;
        }

        // Delete the table and its rows and indexes from memory.
        // TODO: This needs to not remove it from the committed state, because it can still be rolled back.
        // We will have to store the deletion in the TxState and then apply it to the CommittedState in commit.

        // NOT use unwrap
        self.committed_state_write_lock.tables.remove(&table_id);
        Ok(())
    }

    pub fn rename_table(&mut self, table_id: TableId, new_name: &str) -> Result<()> {
        // Update the table's name in st_tables.
        self.update_st_table_row(table_id, |st| st.table_name = new_name.into())
    }

    fn update_st_table_row(&mut self, table_id: TableId, updater: impl FnOnce(&mut StTableRow)) -> Result<()> {
        // Fetch the row.
        let st_table_ref = self
            .iter_by_col_eq(ST_TABLE_ID, StTableFields::TableId, &table_id.into())?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let mut row = StTableRow::try_from(st_table_ref)?;
        let ptr = st_table_ref.pointer();

        // Delete the row, run updates, and insert again.
        self.delete(ST_TABLE_ID, ptr)?;
        updater(&mut row);
        self.insert_via_serialize_bsatn(ST_TABLE_ID, &row)?;

        Ok(())
    }

    pub fn table_id_from_name(&self, table_name: &str) -> Result<Option<TableId>> {
        let table_name = &table_name.into();
        let row = self
            .iter_by_col_eq(ST_TABLE_ID, StTableFields::TableName, table_name)?
            .next();
        Ok(row.map(|row| row.read_col(StTableFields::TableId).unwrap()))
    }

    pub fn table_name_from_id(&self, table_id: TableId) -> Result<Option<Box<str>>> {
        self.iter_by_col_eq(ST_TABLE_ID, StTableFields::TableId, &table_id.into())
            .map(|mut iter| iter.next().map(|row| row.read_col(StTableFields::TableName).unwrap()))
    }

    /// Retrieves or creates the insert tx table for `table_id`.
    #[allow(clippy::type_complexity)]
    fn get_or_create_insert_table_mut(
        &mut self,
        table_id: TableId,
    ) -> Result<(
        &mut Table,
        &mut dyn BlobStore,
        &mut IndexIdMap,
        Option<&Table>,
        &HashMapBlobStore,
    )> {
        let commit_table = self.committed_state_write_lock.get_table(table_id);

        // Get the insert table, so we can write the row into it.
        self.tx_state
            .get_table_and_blob_store_or_maybe_create_from(table_id, commit_table)
            .ok_or_else(|| TableError::IdNotFoundState(table_id).into())
            .map(|(tx, bs, idx_map, _)| {
                (
                    tx,
                    bs,
                    idx_map,
                    commit_table,
                    &self.committed_state_write_lock.blob_store,
                )
            })
    }

    /// Set the table access of `table_id` to `access`.
    pub(crate) fn alter_table_access(&mut self, table_id: TableId, access: StAccess) -> Result<()> {
        // Write to the table in the tx state.
        let (table, ..) = self.get_or_create_insert_table_mut(table_id)?;
        table.with_mut_schema(|s| s.table_access = access);

        // Update system tables.
        self.update_st_table_row(table_id, |st| st.table_access = access)?;
        Ok(())
    }

    /// Create an index.
    ///
    /// Requires:
    /// - `index.index_name` must not be used for any other database entity.
    /// - `index.index_id == IndexId::SENTINEL`
    /// - `index.table_id != TableId::SENTINEL`
    /// - `is_unique` must be `true` if and only if a unique constraint will exist on
    ///     `ColSet::from(&index.index_algorithm.columns())` after this transaction is committed.
    ///
    /// Ensures:
    /// - The index metadata is inserted into the system tables (and other data structures reflecting them).
    /// - The returned ID is unique and is not `IndexId::SENTINEL`.
    pub fn create_index(&mut self, mut index: IndexSchema, is_unique: bool) -> Result<IndexId> {
        if index.index_id != IndexId::SENTINEL {
            return Err(anyhow::anyhow!("`index_id` must be `IndexId::SENTINEL` in `{:#?}`", index).into());
        }
        if index.table_id == TableId::SENTINEL {
            return Err(anyhow::anyhow!("`table_id` must not be `TableId::SENTINEL` in `{:#?}`", index).into());
        }

        let table_id = index.table_id;
        log::trace!(
            "INDEX CREATING: {} for table: {} and algorithm: {:?}",
            index.index_name,
            table_id,
            index.index_algorithm
        );
        if self.table_name(table_id).is_none() {
            return Err(TableError::IdNotFoundState(table_id).into());
        }

        // Insert the index row into st_indexes
        // NOTE: Because st_indexes has a unique index on index_name, this will
        // fail if the index already exists.
        let row = StIndexRow {
            index_id: IndexId::SENTINEL,
            table_id,
            index_name: index.index_name.clone(),
            index_algorithm: index.index_algorithm.clone().into(),
        };
        let index_id = self
            .insert_via_serialize_bsatn(ST_INDEX_ID, &row)?
            .1
            .collapse()
            .read_col(StIndexFields::IndexId)?;

        // Construct the index schema.
        index.index_id = index_id;

        // Add the index to the transaction's insert table.
        let (table, blob_store, idx_map, commit_table, commit_blob_store) =
            self.get_or_create_insert_table_mut(table_id)?;

        let columns = match &index.index_algorithm {
            IndexAlgorithm::BTree(BTreeAlgorithm { columns }) => columns.clone(),
            IndexAlgorithm::Direct(DirectAlgorithm { column: _ }) => todo!("todo_direct_index"),
            _ => unimplemented!(),
        };
        // Create and build the index.
        //
        // Ensure adding the index does not cause a unique constraint violation due to
        // the existing rows having the same value for some column(s).
        let mut insert_index = table.new_index(columns.clone(), is_unique)?;
        let mut build_from_rows = |table: &Table, bs: &dyn BlobStore| -> Result<()> {
            let rows = table.scan_rows(bs);
            // SAFETY: (1) `insert_index` was derived from `table`
            // which in turn was derived from `commit_table`.
            let violation = unsafe { insert_index.build_from_rows(rows) };
            if let Err(violation) = violation {
                let violation = table
                    .get_row_ref(bs, violation)
                    .expect("row came from scanning the table")
                    .project(&columns)
                    .expect("`cols` should consist of valid columns for this table");
                return Err(IndexError::from(table.build_error_unique(&insert_index, index_id, violation)).into());
            }
            Ok(())
        };
        build_from_rows(table, blob_store)?;
        // NOTE: Also add all the rows in the already committed table to the index.
        //
        // FIXME: Is this correct? Index scan iterators (incl. the existing `Locking` versions)
        // appear to assume that a table's index refers only to rows within that table,
        // and does not handle the case where a `TxState` index refers to `CommittedState` rows.
        //
        // TODO(centril): An alternative here is to actually add this index to `CommittedState`,
        // pretending that it was already committed, and recording this pretense.
        // Then, we can roll that back on a failed tx.
        if let Some(commit_table) = commit_table {
            build_from_rows(commit_table, commit_blob_store)?;
        }

        log::trace!(
            "INDEX CREATED: {} for table: {} and col(s): {:?}",
            index_id,
            table_id,
            columns
        );

        // SAFETY: same as (1).
        unsafe { table.add_index(index_id, insert_index) };
        // Associate `index_id -> table_id` for fast lookup.
        idx_map.insert(index_id, table_id);

        // Update the table's schema.
        // This won't clone-write when creating a table but likely to otherwise.
        table.with_mut_schema(|s| s.indexes.push(index));

        Ok(index_id)
    }

    pub fn drop_index(&mut self, index_id: IndexId) -> Result<()> {
        log::trace!("INDEX DROPPING: {}", index_id);
        // Find the index in `st_indexes`.
        let st_index_ref = self
            .iter_by_col_eq(ST_INDEX_ID, StIndexFields::IndexId, &index_id.into())?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_index, index_id.into()))?;
        let st_index_row = StIndexRow::try_from(st_index_ref)?;
        let st_index_ptr = st_index_ref.pointer();
        let table_id = st_index_row.table_id;

        // Remove the index from st_indexes.
        self.delete(ST_INDEX_ID, st_index_ptr)?;

        // Remove the index in the transaction's insert table.
        // By altering the insert table, this gets moved over to the committed state on merge.
        let (table, blob_store, idx_map, ..) = self.get_or_create_insert_table_mut(table_id)?;
        assert!(table.delete_index(blob_store, index_id));
        // Remove the `index_id -> (table_id, col_list)` association from tx state.
        idx_map.remove(&index_id);
        // Queue the deletion of the index in the committed state.
        // Note that the index could have been added in this tx.
        self.tx_state
            .index_id_map_removals
            .get_or_insert_default()
            .insert(index_id);

        log::trace!("INDEX DROPPED: {}", index_id);
        Ok(())
    }

    pub fn index_id_from_name(&self, index_name: &str) -> Result<Option<IndexId>> {
        let name = &index_name.into();
        let row = self.iter_by_col_eq(ST_INDEX_ID, StIndexFields::IndexName, name)?.next();
        Ok(row.map(|row| row.read_col(StIndexFields::IndexId).unwrap()))
    }

    /// Returns an iterator yielding rows by performing a range index scan
    /// on the range-scan-compatible index identified by `index_id`.
    ///
    /// The `prefix` is equated to the first `prefix_elems` values of the index key
    /// and then `prefix_elem`th value is bounded to the left by by `rstart`
    /// and to the right by `rend`.
    pub fn index_scan_range<'a>(
        &'a self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<(TableId, IndexScanRanged<'a>)> {
        // Extract the table id, and commit/tx indices.
        let (table_id, commit_index, tx_index) = self.get_table_and_index(index_id);
        // Extract the index type and make sure we have a table id.
        let (index_ty, table_id) = commit_index
            .or(tx_index)
            .map(|index| &index.index().key_type)
            .zip(table_id)
            .ok_or_else(|| IndexError::NotFound(index_id))?;

        // TODO(centril): Once we have more index types than range-compatible ones,
        // we'll need to enforce that `index_id` refers to a range-compatible index.

        // We have the index key type, so we can decode everything.
        let bounds =
            Self::range_scan_decode_bounds(index_ty, prefix, prefix_elems, rstart, rend).map_err(IndexError::Decode)?;

        // Get an index seek iterator for the tx and committed state.
        let tx_iter = tx_index.map(|i| i.seek(&bounds));
        let commit_iter = commit_index.map(|i| i.seek(&bounds));

        // Chain together the indexed rows in the tx and committed state,
        // but don't yield rows deleted in the tx state.
        use itertools::Either::*;
        use IndexScanRangedInner::*;
        let commit_iter = commit_iter.map(|iter| match self.tx_state.get_delete_table(table_id) {
            None => Left(iter),
            Some(deletes) => Right(IndexScanFilterDeleted { iter, deletes }),
        });
        // This is effectively just `tx_iter.into_iter().flatten().chain(commit_iter.into_iter().flatten())`,
        // but with all the branching and `Option`s flattened to just one layer.
        let iter = match (tx_iter, commit_iter) {
            (None, None) => Empty(iter::empty()),
            (Some(tx_iter), None) => TxOnly(tx_iter),
            (None, Some(Left(commit_iter))) => CommitOnly(commit_iter),
            (None, Some(Right(commit_iter))) => CommitOnlyWithDeletes(commit_iter),
            (Some(tx_iter), Some(Left(commit_iter))) => Both(tx_iter.chain(commit_iter)),
            (Some(tx_iter), Some(Right(commit_iter))) => BothWithDeletes(tx_iter.chain(commit_iter)),
        };
        Ok((table_id, IndexScanRanged { inner: iter }))
    }

    /// Translate `index_id` to the table id, and commit/tx indices.
    fn get_table_and_index(
        &self,
        index_id: IndexId,
    ) -> (Option<TableId>, Option<TableAndIndex<'_>>, Option<TableAndIndex<'_>>) {
        // The order of querying the committed vs. tx state for the translation is not important.
        // But it is vastly more likely that it is in the committed state,
        // so query that first to avoid two lookups.
        //
        // Also, the tx state must have the index.
        // If the index was e.g., dropped from the tx state but exists physically in the committed state,
        // the index does not exist, semantically.
        // TODO: handle the case where the table has been dropped in this transaction.
        let commit_table_id = self
            .committed_state_write_lock
            .get_table_for_index(index_id)
            .filter(|_| !self.tx_state_removed_index(index_id));

        let (table_id, commit_index, tx_index) = if let t_id @ Some(table_id) = commit_table_id {
            // Index found for commit state, might also exist for tx state.
            let commit_index = self
                .committed_state_write_lock
                .get_index_by_id_with_table(table_id, index_id);
            let tx_index = self.tx_state.get_index_by_id_with_table(table_id, index_id);
            (t_id, commit_index, tx_index)
        } else if let t_id @ Some(table_id) = self.tx_state.get_table_for_index(index_id) {
            // Index might exist for tx state.
            let tx_index = self.tx_state.get_index_by_id_with_table(table_id, index_id);
            (t_id, None, tx_index)
        } else {
            // No index in either side.
            (None, None, None)
        };
        (table_id, commit_index, tx_index)
    }

    /// Returns whether the index with `index_id` was removed in this transaction.
    ///
    /// An index removed in the tx state but existing physically in the committed state
    /// does not exist semantically.
    fn tx_state_removed_index(&self, index_id: IndexId) -> bool {
        self.tx_state
            .index_id_map_removals
            .as_ref()
            .is_some_and(|s| s.contains(&index_id))
    }

    /// Decode the bounds for a ranged index scan for an index typed at `key_type`.
    fn range_scan_decode_bounds(
        key_type: &AlgebraicType,
        mut prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> DecodeResult<(Bound<AlgebraicValue>, Bound<AlgebraicValue>)> {
        match key_type {
            // Multi-column index case.
            AlgebraicType::Product(key_types) => {
                let key_types = &key_types.elements;
                // Split into types for the prefix and for the rest.
                let (prefix_types, rest_types) = key_types
                    .split_at_checked(prefix_elems.idx())
                    .ok_or_else(|| DecodeError::Other("index key type has too few fields compared to prefix".into()))?;

                // The `rstart` and `rend`s must be typed at `Bound<range_type>`.
                // Extract that type and determine the length of the suffix.
                let Some((range_type, suffix_types)) = rest_types.split_first() else {
                    return Err(DecodeError::Other(
                        "prefix length leaves no room for a range in ranged index scan".into(),
                    ));
                };
                let suffix_len = suffix_types.len();

                // We now have the types,
                // so proceed to decoding the prefix, and the start/end bounds.
                // Finally combine all of these to a single bound pair.
                let prefix = bsatn::decode(prefix_types, &mut prefix)?;
                let (start, end) = Self::range_scan_decode_start_end(&range_type.algebraic_type, rstart, rend)?;
                Ok(Self::range_scan_combine_prefix_and_bounds(
                    prefix, start, end, suffix_len,
                ))
            }
            // Single-column index case. We implicitly have a PT of len 1.
            _ if !prefix.is_empty() && prefix_elems.idx() != 0 => Err(DecodeError::Other(
                "a single-column index cannot be prefix scanned".into(),
            )),
            ty => Self::range_scan_decode_start_end(ty, rstart, rend),
        }
    }

    /// Decode `rstart` and `rend` as `Bound<ty>`.
    fn range_scan_decode_start_end(
        ty: &AlgebraicType,
        mut rstart: &[u8],
        mut rend: &[u8],
    ) -> DecodeResult<(Bound<AlgebraicValue>, Bound<AlgebraicValue>)> {
        let range_type = WithBound(WithTypespace::empty(ty));
        let range_start = range_type.deserialize(Deserializer::new(&mut rstart))?;
        let range_end = range_type.deserialize(Deserializer::new(&mut rend))?;
        Ok((range_start, range_end))
    }

    /// Combines `prefix` equality constraints with `start` and `end` bounds
    /// filling with `suffix_len` to ensure that the number of fields matches
    /// that of the index type.
    fn range_scan_combine_prefix_and_bounds(
        prefix: ProductValue,
        start: Bound<AlgebraicValue>,
        end: Bound<AlgebraicValue>,
        suffix_len: usize,
    ) -> (Bound<AlgebraicValue>, Bound<AlgebraicValue>) {
        let prefix_is_empty = prefix.elements.is_empty();
        // Concatenate prefix, value, and the most permissive value for the suffix.
        let concat = |prefix: ProductValue, val, fill| {
            let mut vals: Vec<_> = prefix.elements.into();
            vals.reserve(1 + suffix_len);
            vals.push(val);
            vals.extend(iter::repeat(fill).take(suffix_len));
            AlgebraicValue::product(vals)
        };
        // The start endpoint needs `Min` as the suffix-filling element,
        // as it imposes the least and acts like `Unbounded`.
        let concat_start = |val| concat(prefix.clone(), val, AlgebraicValue::Min);
        let range_start = match start {
            Bound::Included(r) => Bound::Included(concat_start(r)),
            Bound::Excluded(r) => Bound::Excluded(concat_start(r)),
            // Prefix is empty, and suffix will be `Min`,
            // so simplify `(Min, Min, ...)` to `Unbounded`.
            Bound::Unbounded if prefix_is_empty => Bound::Unbounded,
            Bound::Unbounded => Bound::Included(concat_start(AlgebraicValue::Min)),
        };
        // The end endpoint needs `Max` as the suffix-filling element,
        // as it imposes the least and acts like `Unbounded`.
        let concat_end = |val| concat(prefix, val, AlgebraicValue::Max);
        let range_end = match end {
            Bound::Included(r) => Bound::Included(concat_end(r)),
            Bound::Excluded(r) => Bound::Excluded(concat_end(r)),
            // Prefix is empty, and suffix will be `Max`,
            // so simplify `(Max, Max, ...)` to `Unbounded`.
            Bound::Unbounded if prefix_is_empty => Bound::Unbounded,
            Bound::Unbounded => Bound::Included(concat_end(AlgebraicValue::Max)),
        };
        (range_start, range_end)
    }

    fn get_sequence_mut(&mut self, seq_id: SequenceId) -> Result<&mut Sequence> {
        self.sequence_state_lock
            .get_sequence_mut(seq_id)
            .ok_or_else(|| SequenceError::NotFound(seq_id).into())
    }

    pub fn get_next_sequence_value(&mut self, seq_id: SequenceId) -> Result<i128> {
        {
            let sequence = self.get_sequence_mut(seq_id)?;

            // If there are allocated sequence values, return the new value.
            // `gen_next_value` internally checks that the new allocation is acceptable,
            // i.e. is less than or equal to the allocation amount.
            // Note that on restart we start one after the allocation amount.
            if let Some(value) = sequence.gen_next_value() {
                return Ok(value);
            }
        }
        // Allocate new sequence values
        // If we're out of allocations, then update the sequence row in st_sequences to allocate a fresh batch of sequences.
        let old_seq_row_ref = self
            .iter_by_col_eq(ST_SEQUENCE_ID, StSequenceFields::SequenceId, &seq_id.into())?
            .last()
            .unwrap();
        let old_seq_row_ptr = old_seq_row_ref.pointer();
        let seq_row = {
            let mut seq_row = StSequenceRow::try_from(old_seq_row_ref)?;

            let sequence = self.get_sequence_mut(seq_id)?;
            seq_row.allocated = sequence.nth_value(SEQUENCE_ALLOCATION_STEP as usize);
            sequence.set_allocation(seq_row.allocated);
            seq_row
        };

        self.delete(ST_SEQUENCE_ID, old_seq_row_ptr)?;
        // `insert::<GENERATE = false>` rather than `GENERATE = true` because:
        // - We have already checked unique constraints during `create_sequence`.
        // - Similarly, we have already applied autoinc sequences.
        // - We do not want to apply autoinc sequences again,
        //   since the system table sequence `seq_st_table_table_id_primary_key_auto`
        //   has ID 0, and would otherwise trigger autoinc.
        with_sys_table_buf(|buf| {
            to_writer(buf, &seq_row).unwrap();
            self.insert::<false>(ST_SEQUENCE_ID, buf)
        })?;

        self.get_sequence_mut(seq_id)?
            .gen_next_value()
            .ok_or_else(|| SequenceError::UnableToAllocate(seq_id).into())
    }

    /// Create a sequence.
    /// Requires:
    /// - `seq.sequence_id == SequenceId::SENTINEL`
    /// - `seq.table_id != TableId::SENTINEL`
    /// - `seq.sequence_name` must not be used for any other database entity.
    ///
    /// Ensures:
    /// - The sequence metadata is inserted into the system tables (and other data structures reflecting them).
    /// - The returned ID is unique and not `SequenceId::SENTINEL`.
    pub fn create_sequence(&mut self, seq: SequenceSchema) -> Result<SequenceId> {
        if seq.sequence_id != SequenceId::SENTINEL {
            return Err(anyhow::anyhow!("`sequence_id` must be `SequenceId::SENTINEL` in `{:#?}`", seq).into());
        }
        if seq.table_id == TableId::SENTINEL {
            return Err(anyhow::anyhow!("`table_id` must not be `TableId::SENTINEL` in `{:#?}`", seq).into());
        }

        let table_id = seq.table_id;
        log::trace!(
            "SEQUENCE CREATING: {} for table: {} and col: {}",
            seq.sequence_name,
            table_id,
            seq.col_pos
        );

        // Insert the sequence row into st_sequences
        // NOTE: Because st_sequences has a unique index on sequence_name, this will
        // fail if the table already exists.
        let mut sequence_row = StSequenceRow {
            sequence_id: SequenceId::SENTINEL,
            sequence_name: seq.sequence_name,
            table_id,
            col_pos: seq.col_pos,
            allocated: seq.allocated,
            increment: seq.increment,
            start: seq.start,
            min_value: seq.min_value,
            max_value: seq.max_value,
        };
        let row = self.insert_via_serialize_bsatn(ST_SEQUENCE_ID, &sequence_row)?;
        let seq_id = row.1.collapse().read_col(StSequenceFields::SequenceId)?;
        sequence_row.sequence_id = seq_id;

        let schema: SequenceSchema = sequence_row.into();
        self.get_insert_table_mut(schema.table_id)?
            // This won't clone-write when creating a table but likely to otherwise.
            .with_mut_schema(|s| s.update_sequence(schema.clone()));
        self.sequence_state_lock.insert(seq_id, Sequence::new(schema));

        log::trace!("SEQUENCE CREATED: id = {}", seq_id);

        Ok(seq_id)
    }

    pub fn drop_sequence(&mut self, sequence_id: SequenceId) -> Result<()> {
        let st_sequence_ref = self
            .iter_by_col_eq(ST_SEQUENCE_ID, StSequenceFields::SequenceId, &sequence_id.into())?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_sequence, sequence_id.into()))?;
        let table_id = st_sequence_ref.read_col(StSequenceFields::TableId)?;

        self.delete(ST_SEQUENCE_ID, st_sequence_ref.pointer())?;

        // TODO: Transactionality.
        // Currently, a TX which drops a sequence then aborts
        // will leave the sequence deleted,
        // rather than restoring it during rollback.
        self.sequence_state_lock.remove(sequence_id);
        if let Some((insert_table, _)) = self.tx_state.get_table_and_blob_store(table_id) {
            // This likely will do a clone-write as over time?
            // The schema might have found other referents.
            insert_table.with_mut_schema(|s| s.remove_sequence(sequence_id));
        }
        Ok(())
    }

    pub fn sequence_id_from_name(&self, seq_name: &str) -> Result<Option<SequenceId>> {
        let name = &<Box<str>>::from(seq_name).into();
        self.iter_by_col_eq(ST_SEQUENCE_ID, StSequenceFields::SequenceName, name)
            .map(|mut iter| {
                iter.next()
                    .map(|row| row.read_col(StSequenceFields::SequenceId).unwrap())
            })
    }

    /// Create a constraint.
    ///
    /// Requires:
    /// - `constraint.constraint_name` must not be used for any other database entity.
    /// - `constraint.constraint_id == ConstraintId::SENTINEL`
    /// - `constraint.table_id != TableId::SENTINEL`
    /// - `is_unique` must be `true` if and only if a unique constraint will exist on
    ///     `ColSet::from(&constraint.constraint_algorithm.columns())` after this transaction is committed.
    ///
    /// Ensures:
    /// - The constraint metadata is inserted into the system tables (and other data structures reflecting them).
    /// - The returned ID is unique and is not `constraintId::SENTINEL`.
    fn create_constraint(&mut self, mut constraint: ConstraintSchema) -> Result<ConstraintId> {
        if constraint.constraint_id != ConstraintId::SENTINEL {
            return Err(anyhow::anyhow!(
                "`constraint_id` must be `ConstraintId::SENTINEL` in `{:#?}`",
                constraint
            )
            .into());
        }
        if constraint.table_id == TableId::SENTINEL {
            return Err(anyhow::anyhow!("`table_id` must not be `TableId::SENTINEL` in `{:#?}`", constraint).into());
        }

        let table_id = constraint.table_id;

        log::trace!(
            "CONSTRAINT CREATING: {} for table: {} and data: {:?}",
            constraint.constraint_name,
            table_id,
            constraint.data
        );

        // Insert the constraint row into st_constraint
        // NOTE: Because st_constraint has a unique index on constraint_name, this will
        // fail if the table already exists.
        let constraint_row = StConstraintRow {
            table_id,
            constraint_id: ConstraintId::SENTINEL,
            constraint_name: constraint.constraint_name.clone(),
            constraint_data: constraint.data.clone().into(),
        };

        let constraint_row = self.insert_via_serialize_bsatn(ST_CONSTRAINT_ID, &constraint_row)?;
        let constraint_id = constraint_row.1.collapse().read_col(StConstraintFields::ConstraintId)?;
        let existed = matches!(constraint_row.1, RowRefInsertion::Existed(_));
        // TODO: Can we return early here?

        let (table, ..) = self.get_or_create_insert_table_mut(table_id)?;
        constraint.constraint_id = constraint_id;
        // This won't clone-write when creating a table but likely to otherwise.
        table.with_mut_schema(|s| s.update_constraint(constraint));

        if existed {
            log::trace!("CONSTRAINT ALREADY EXISTS: {constraint_id}");
        } else {
            log::trace!("CONSTRAINT CREATED: {constraint_id}");
        }

        Ok(constraint_id)
    }

    fn get_insert_table_mut(&mut self, table_id: TableId) -> Result<&mut Table> {
        self.tx_state
            .get_table_and_blob_store(table_id)
            .map(|(tbl, _)| tbl)
            .ok_or_else(|| TableError::IdNotFoundState(table_id).into())
    }

    pub fn drop_constraint(&mut self, constraint_id: ConstraintId) -> Result<()> {
        // Delete row in `st_constraint`.
        let st_constraint_ref = self
            .iter_by_col_eq(
                ST_CONSTRAINT_ID,
                StConstraintFields::ConstraintId,
                &constraint_id.into(),
            )?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_constraint, constraint_id.into()))?;
        let table_id = st_constraint_ref.read_col(StConstraintFields::TableId)?;
        self.delete(ST_CONSTRAINT_ID, st_constraint_ref.pointer())?;

        // Remove constraint in transaction's insert table.
        let (table, ..) = self.get_or_create_insert_table_mut(table_id)?;
        // This likely will do a clone-write as over time?
        // The schema might have found other referents.
        table.with_mut_schema(|s| s.remove_constraint(constraint_id));
        // TODO(1.0): we should also re-initialize `table` without a unique constraint.
        // unless some other unique constraint on the same columns exists.

        Ok(())
    }

    pub fn constraint_id_from_name(&self, constraint_name: &str) -> Result<Option<ConstraintId>> {
        self.iter_by_col_eq(
            ST_CONSTRAINT_ID,
            StConstraintFields::ConstraintName,
            &<Box<str>>::from(constraint_name).into(),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| row.read_col(StConstraintFields::ConstraintId).unwrap())
        })
    }

    /// Create a row level security policy.
    ///
    /// Requires:
    /// - `row_level_security_schema.table_id != TableId::SENTINEL`
    /// - `row_level_security_schema.sql` must be unique.
    ///
    /// Ensures:
    ///
    /// - The row level security policy metadata is inserted into the system tables (and other data structures reflecting them).
    /// - The returned `sql` is unique.
    pub fn create_row_level_security(&mut self, row_level_security_schema: RowLevelSecuritySchema) -> Result<RawSql> {
        if row_level_security_schema.table_id == TableId::SENTINEL {
            return Err(anyhow::anyhow!(
                "`table_id` must not be `TableId::SENTINEL` in `{:#?}`",
                row_level_security_schema
            )
            .into());
        }

        log::trace!(
            "ROW LEVEL SECURITY CREATING for table: {}",
            row_level_security_schema.table_id
        );

        // Insert the row into st_row_level_security
        // NOTE: Because st_row_level_security has a unique index on sql, this will
        // fail if already exists.
        let row = StRowLevelSecurityRow {
            table_id: row_level_security_schema.table_id,
            sql: row_level_security_schema.sql,
        };

        let row = self.insert_via_serialize_bsatn(ST_ROW_LEVEL_SECURITY_ID, &row)?;
        let row_level_security_sql = row.1.collapse().read_col(StRowLevelSecurityFields::Sql)?;
        let existed = matches!(row.1, RowRefInsertion::Existed(_));

        // Add the row level security to the transaction's insert table.
        self.get_or_create_insert_table_mut(row_level_security_schema.table_id)?;

        if existed {
            log::trace!("ROW LEVEL SECURITY ALREADY EXISTS: {row_level_security_sql}");
        } else {
            log::trace!("ROW LEVEL SECURITY CREATED: {row_level_security_sql}");
        }

        Ok(row_level_security_sql)
    }

    pub fn row_level_security_for_table_id(&self, table_id: TableId) -> Result<Vec<RowLevelSecuritySchema>> {
        Ok(self
            .iter_by_col_eq(
                ST_ROW_LEVEL_SECURITY_ID,
                StRowLevelSecurityFields::TableId,
                &table_id.into(),
            )?
            .map(|row| {
                let row = StRowLevelSecurityRow::try_from(row).unwrap();
                row.into()
            })
            .collect())
    }

    pub fn drop_row_level_security(&mut self, sql: RawSql) -> Result<()> {
        let st_rls_ref = self
            .iter_by_col_eq(
                ST_ROW_LEVEL_SECURITY_ID,
                StRowLevelSecurityFields::Sql,
                &sql.clone().into(),
            )?
            .next()
            .ok_or(TableError::RawSqlNotFound(SystemTable::st_row_level_security, sql))?;
        self.delete(ST_ROW_LEVEL_SECURITY_ID, st_rls_ref.pointer())?;

        Ok(())
    }

    // TODO(perf, deep-integration):
    //   When all of [`Table::read_row`], [`RowRef::new`], [`CommittedState::get`]
    //   and [`TxState::get`] become unsafe,
    //   make this method `unsafe` as well.
    //   Add the following to the docs:
    //
    // # Safety
    //
    // `pointer` must refer to a row within the table at `table_id`
    // which was previously inserted and has not been deleted since.
    //
    // See [`RowRef::new`] for more detailed requirements.
    //
    // Showing that `pointer` was the result of a call to `self.insert`
    // with `table_id`
    // and has not been passed to `self.delete`
    // is sufficient to demonstrate that a call to `self.get` is safe.
    pub fn get(&self, table_id: TableId, row_ptr: RowPointer) -> Result<Option<RowRef<'_>>> {
        if self.table_name(table_id).is_none() {
            return Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into());
        }
        Ok(match row_ptr.squashed_offset() {
            SquashedOffset::TX_STATE => Some(
                // TODO(perf, deep-integration):
                // See above. Once `TxState::get` is unsafe, justify with:
                //
                // Our invariants satisfy `TxState::get`.
                self.tx_state.get(table_id, row_ptr),
            ),
            SquashedOffset::COMMITTED_STATE => {
                if self.tx_state.is_deleted(table_id, row_ptr) {
                    None
                } else {
                    Some(
                        // TODO(perf, deep-integration):
                        // See above. Once `CommittedState::get` is unsafe, justify with:
                        //
                        // Our invariants satisfy `CommittedState::get`.
                        self.committed_state_write_lock.get(table_id, row_ptr),
                    )
                }
            }
            _ => unreachable!("Invalid SquashedOffset for row pointer: {:?}", row_ptr),
        })
    }

    pub fn commit(self) -> TxData {
        let Self {
            mut committed_state_write_lock,
            tx_state,
            ..
        } = self;
        let tx_data = committed_state_write_lock.merge(tx_state, &self.ctx);
        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_tx_metrics(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            true,
            Some(&tx_data),
            Some(&committed_state_write_lock),
            self.metrics,
        );
        tx_data
    }

    pub fn commit_downgrade(mut self, workload: Workload) -> (TxData, TxId) {
        let Self {
            mut committed_state_write_lock,
            tx_state,
            ..
        } = self;
        let tx_data = committed_state_write_lock.merge(tx_state, &self.ctx);
        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_tx_metrics(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            true,
            Some(&tx_data),
            Some(&committed_state_write_lock),
            self.metrics,
        );
        // Update the workload type of the execution context
        self.ctx.workload = workload.into();
        let tx = TxId {
            committed_state_shared_lock: SharedWriteGuard::downgrade(committed_state_write_lock),
            lock_wait_time: Duration::ZERO,
            timer: Instant::now(),
            ctx: self.ctx,
            metrics: ExecutionMetrics::default(),
        };
        (tx_data, tx)
    }

    pub fn rollback(self) {
        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_tx_metrics(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            false,
            None,
            None,
            self.metrics,
        );
    }

    pub fn rollback_downgrade(mut self, workload: Workload) -> TxId {
        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_tx_metrics(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            false,
            None,
            None,
            self.metrics,
        );
        // Update the workload type of the execution context
        self.ctx.workload = workload.into();
        TxId {
            committed_state_shared_lock: SharedWriteGuard::downgrade(self.committed_state_write_lock),
            lock_wait_time: Duration::ZERO,
            timer: Instant::now(),
            ctx: self.ctx,
            metrics: ExecutionMetrics::default(),
        }
    }
}

/// Either a row just inserted to a table or a row that already existed in some table.
#[derive(Clone, Copy)]
pub(crate) enum RowRefInsertion<'a> {
    /// The row was just inserted.
    Inserted(RowRef<'a>),
    /// The row already existed.
    Existed(RowRef<'a>),
}

impl<'a> RowRefInsertion<'a> {
    /// Returns a row,
    /// collapsing the distinction between inserted and existing rows.
    pub(super) fn collapse(&self) -> RowRef<'a> {
        let (Self::Inserted(row) | Self::Existed(row)) = *self;
        row
    }
}

/// The iterator returned by [`MutTxId::index_scan_range`].
pub struct IndexScanRanged<'a> {
    inner: IndexScanRangedInner<'a>,
}

enum IndexScanRangedInner<'a> {
    Empty(iter::Empty<RowRef<'a>>),
    TxOnly(IndexScanIter<'a>),
    CommitOnly(IndexScanIter<'a>),
    CommitOnlyWithDeletes(IndexScanFilterDeleted<'a>),
    Both(iter::Chain<IndexScanIter<'a>, IndexScanIter<'a>>),
    BothWithDeletes(iter::Chain<IndexScanIter<'a>, IndexScanFilterDeleted<'a>>),
}

struct IndexScanFilterDeleted<'a> {
    iter: IndexScanIter<'a>,
    deletes: &'a DeleteTable,
}

impl<'a> Iterator for IndexScanRanged<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            IndexScanRangedInner::Empty(it) => it.next(),
            IndexScanRangedInner::TxOnly(it) => it.next(),
            IndexScanRangedInner::CommitOnly(it) => it.next(),
            IndexScanRangedInner::CommitOnlyWithDeletes(it) => it.next(),
            IndexScanRangedInner::Both(it) => it.next(),
            IndexScanRangedInner::BothWithDeletes(it) => it.next(),
        }
    }
}

impl<'a> Iterator for IndexScanFilterDeleted<'a> {
    type Item = RowRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.find(|row| !self.deletes.contains(row.pointer()))
    }
}

impl MutTxId {
    pub(crate) fn insert_via_serialize_bsatn<'a, T: Serialize>(
        &'a mut self,
        table_id: TableId,
        row: &T,
    ) -> Result<(ColList, RowRefInsertion<'a>, InsertFlags)> {
        thread_local! {
            static BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        }
        BUF.with_borrow_mut(|buf| {
            buf.clear();
            to_writer(buf, row).unwrap();
            self.insert::<true>(table_id, buf)
        })
    }

    /// Insert a row, encoded in BSATN, into a table.
    ///
    /// Zero placeholders, i.e., sequence triggers,
    /// in auto-inc columns in the new row will be replaced with generated values
    /// if and only if `GENERATE` is true.
    /// This method is called with `GENERATE` false when updating the `st_sequence` system table.
    ///
    /// Requires:
    /// - `table_id` must refer to a valid table for the database at `database_address`.
    /// - `row` must be a valid row for the table at `table_id`.
    ///
    /// Returns:
    /// - a list of columns which have been replaced with generated values.
    /// - a ref to the inserted row.
    /// - any insert flags.
    pub(super) fn insert<const GENERATE: bool>(
        &mut self,
        table_id: TableId,
        row: &[u8],
    ) -> Result<(ColList, RowRefInsertion<'_>, InsertFlags)> {
        // Get the insert table, so we can write the row into it.
        let (tx_table, tx_blob_store, ..) = self
            .tx_state
            .get_table_and_blob_store_or_maybe_create_from(
                table_id,
                self.committed_state_write_lock.get_table(table_id),
            )
            .ok_or(TableError::IdNotFoundState(table_id))?;

        let insert_flags = InsertFlags {
            is_scheduler_table: tx_table.is_scheduler(),
        };

        // 1. Insert the physical row.
        let (tx_row_ref, blob_bytes) = tx_table.insert_physically_bsatn(tx_blob_store, row)?;
        // 2. Optionally: Detect, generate, write sequence values.
        // 3. Confirm that the insertion respects constraints and update statistics.
        // 4. Post condition (PC.INS.1):
        //        `res = Ok((hash, ptr))`
        //     => `ptr` refers to a valid row in `table_id` for `tx_table`
        //      ∧ `hash` is the hash of this row
        //    This follows from both `if/else` branches leading to `confirm_insertion`
        //    which both entail the above post-condition.
        let ((tx_table, tx_blob_store, delete_table), gen_cols, res) = if GENERATE {
            // When `GENERATE` is enabled, we're instructed to deal with sequence value generation.
            // Collect all the columns with sequences that need generation.
            let tx_row_ptr = tx_row_ref.pointer();
            let (cols_to_gen, seqs_to_use) = unsafe { tx_table.sequence_triggers_for(tx_blob_store, tx_row_ptr) };

            // Generate a value for every column in the row that needs it.
            let mut seq_vals: SmallVec<[i128; 1]> = <_>::default();
            for sequence_id in seqs_to_use {
                seq_vals.push(self.get_next_sequence_value(sequence_id)?);
            }

            // Write the generated values to the physical row at `tx_row_ptr`.
            // We assume here that column with a sequence is of a sequence-compatible type.
            // SAFETY: By virtue of `get_table_and_blob_store_or_maybe_create_from` above succeeding,
            // we can assume we have an insert and delete table.
            let (tx_table, tx_blob_store, delete_table) =
                unsafe { self.tx_state.assume_present_get_mut_table(table_id) };
            for (col_id, seq_val) in cols_to_gen.iter().zip(seq_vals) {
                // SAFETY:
                // - `self.is_row_present(row)` holds as we haven't deleted the row.
                // - `col_id` is a valid column, and has a sequence, so it must have a primitive type.
                unsafe { tx_table.write_gen_val_to_col(col_id, tx_row_ptr, seq_val) };
            }

            // SAFETY: `self.is_row_present(row)` holds as we still haven't deleted the row,
            // in particular, the `write_gen_val_to_col` call does not remove the row.
            let res = unsafe { tx_table.confirm_insertion(tx_blob_store, tx_row_ptr, blob_bytes) };
            ((tx_table, tx_blob_store, delete_table), cols_to_gen, res)
        } else {
            // When `GENERATE` is not enabled, simply confirm the insertion.
            // This branch is hit when inside sequence generation itself, to avoid infinite recursion.
            let tx_row_ptr = tx_row_ref.pointer();
            // SAFETY: `self.is_row_present(row)` holds as we just inserted the row.
            let res = unsafe { tx_table.confirm_insertion(tx_blob_store, tx_row_ptr, blob_bytes) };
            // SAFETY: By virtue of `get_table_and_blob_store_or_maybe_create_from` above succeeding,
            // we can assume we have an insert and delete table.
            (
                unsafe { self.tx_state.assume_present_get_mut_table(table_id) },
                ColList::empty(),
                res,
            )
        };

        match res {
            Ok((tx_row_hash, tx_row_ptr)) => {
                if let Some(commit_table) = self.committed_state_write_lock.get_table(table_id) {
                    // The `tx_row_ref` was not previously present in insert tables,
                    // but may still be a set-semantic conflict
                    // or may violate a unique constraint with a row in the committed state.
                    // We'll check the set-semantic aspect in (1) and the constraint in (2).

                    // (1) Rule out a set-semantic conflict with the committed state.
                    // SAFETY:
                    // - `commit_table` and `tx_table` use the same schema
                    //   because `tx_table` is derived from `commit_table`.
                    // - `tx_row_ptr` is correct per (PC.INS.1).
                    if let (_, Some(commit_ptr)) =
                        unsafe { Table::find_same_row(commit_table, tx_table, tx_blob_store, tx_row_ptr, tx_row_hash) }
                    {
                        // If `row` was already present in the committed state,
                        // either this is a set-semantic duplicate,
                        // or the row is marked as deleted, so we will undelete it
                        // and leave it in the committed state.
                        // Either way, it should not appear in the insert tables,
                        // so roll back the insertion.
                        //
                        // NOTE for future MVCC implementors:
                        // In MVCC, it is no longer valid to elide inserts in this way.
                        // When a transaction inserts a row, that row *must* appear in its insert tables,
                        // even if the row is already present in the committed state.
                        //
                        // Imagine a chain of committed but un-squashed transactions:
                        // `Committed 0: Insert Row A` - `Committed 1: Delete Row A`
                        // where `Committed 1` happens after `Committed 0`.
                        // Imagine a transaction `Running 2: Insert Row A`,
                        // which began before `Committed 1` was committed.
                        // Because `Committed 1` has since been committed,
                        // `Running 2` *must* happen after `Committed 1`.
                        // Therefore, the correct sequence of events is:
                        // - Insert Row A
                        // - Delete Row A
                        // - Insert Row A
                        // This is impossible to recover if `Running 2` elides its insert.
                        tx_table
                            .delete(tx_blob_store, tx_row_ptr, |_| ())
                            .expect("Failed to delete a row we just inserted");

                        // It's possible that `row` appears in the committed state,
                        // but is marked as deleted.
                        // In this case, undelete it, so it remains in the committed state.
                        delete_table.remove(commit_ptr);

                        // No new row was inserted, but return `committed_ptr`.
                        let blob_store = &self.committed_state_write_lock.blob_store;
                        let rri = RowRefInsertion::Existed(
                            // SAFETY: `find_same_row` told us that `ptr` refers to a valid row in `commit_table`.
                            unsafe { commit_table.get_row_ref_unchecked(blob_store, commit_ptr) },
                        );
                        return Ok((gen_cols, rri, insert_flags));
                    }

                    // Pacify the borrow checker.
                    // SAFETY: `tx_row_ptr` is still correct for `tx_table` per (PC.INS.1).
                    // as there haven't been any interleaving `&mut` calls that could invalidate the pointer.
                    let tx_row_ref = unsafe { tx_table.get_row_ref_unchecked(tx_blob_store, tx_row_ptr) };

                    // (2) The `tx_row_ref` did not violate a unique constraint *within* the `tx_table`,
                    // but it could do so wrt., `commit_table`,
                    // assuming the conflicting row hasn't been deleted since.
                    // Ensure that it doesn't, or roll back the insertion.
                    let is_deleted = |commit_ptr| delete_table.contains(commit_ptr);
                    // SAFETY: `commit_table.row_layout() == tx_row_ref.row_layout()` holds
                    // as the `tx_table` is derived from `commit_table`.
                    let res = unsafe { commit_table.check_unique_constraints(tx_row_ref, |ixs| ixs, is_deleted) };
                    if let Err(e) = res {
                        // There was a constraint violation, so undo the insertion.
                        tx_table.delete(tx_blob_store, tx_row_ptr, |_| {});
                        return Err(IndexError::from(e).into());
                    }
                }

                let rri = RowRefInsertion::Inserted(unsafe {
                    // SAFETY: `tx_row_ptr` is still correct for `tx_table` per (PC.INS.1).
                    // as there haven't been any interleaving `&mut` calls that could invalidate the pointer.
                    tx_table.get_row_ref_unchecked(tx_blob_store, tx_row_ptr)
                });
                Ok((gen_cols, rri, insert_flags))
            }
            // `row` previously present in insert tables; do nothing but return `ptr`.
            Err(InsertError::Duplicate(DuplicateError(ptr))) => {
                let rri = RowRefInsertion::Existed(
                    // SAFETY: `tx_table` told us that `ptr` refers to a valid row in it.
                    unsafe { tx_table.get_row_ref_unchecked(tx_blob_store, ptr) },
                );
                Ok((gen_cols, rri, insert_flags))
            }

            // Unwrap these error into `TableError::{IndexError, Bflatn}`:
            Err(InsertError::IndexError(e)) => Err(IndexError::from(e).into()),
            Err(InsertError::Bflatn(e)) => Err(TableError::Bflatn(e).into()),
        }
    }

    /// Update a row, encoded in BSATN, into a table.
    ///
    /// Zero placeholders, i.e., sequence triggers,
    /// in auto-inc columns in the new row will be replaced with generated values.
    ///
    /// The old row is found by projecting `row` to the columns of `index_id`.
    ///
    /// Requires:
    /// - `table_id` must refer to a valid table for the database at `database_address`.
    /// - `index_id` must refer to a valid index in the table.
    /// - `row` must be a valid row for the table at `table_id`.
    ///
    /// Returns:
    /// - a list of columns which have been replaced with generated values.
    /// - a ref to the new row.
    /// - any update flags.
    pub(crate) fn update(
        &mut self,
        table_id: TableId,
        index_id: IndexId,
        row: &[u8],
    ) -> Result<(ColList, RowRef<'_>, UpdateFlags)> {
        let tx_removed_index = self.tx_state_removed_index(index_id);

        // 1. Insert the physical row into the tx insert table.
        //----------------------------------------------------------------------
        // As we are provided the `row` encoded in BSATN,
        // and since we don't have a convenient way to BSATN to a set of columns,
        // we cannot really do an in-place update in the row-was-in-tx-state case.
        // So we will begin instead by inserting the row physically to the tx state and project that.
        let (tx_table, tx_blob_store, ..) = self
            .tx_state
            .get_table_and_blob_store_or_maybe_create_from(
                table_id,
                self.committed_state_write_lock.get_table(table_id),
            )
            .ok_or(TableError::IdNotFoundState(table_id))?;
        let (tx_row_ref, blob_bytes) = tx_table.insert_physically_bsatn(tx_blob_store, row)?;

        // 2. Detect, generate, write sequence values in the new row.
        //----------------------------------------------------------------------
        // Unlike the `fn insert(...)` case, this is not conditional on a `GENERATE` flag.
        // Collect all the columns with sequences that need generation.
        let tx_row_ptr = tx_row_ref.pointer();
        let (cols_to_gen, seqs_to_use) = unsafe { tx_table.sequence_triggers_for(tx_blob_store, tx_row_ptr) };
        // Generate a value for every column in the row that needs it.
        let mut seq_vals: SmallVec<[i128; 1]> = <_>::default();
        for sequence_id in seqs_to_use {
            seq_vals.push(self.get_next_sequence_value(sequence_id)?);
        }
        // Write the generated values to the physical row at `tx_row_ptr`.
        // We assume here that column with a sequence is of a sequence-compatible type.
        // SAFETY: By virtue of `get_table_and_blob_store_or_maybe_create_from` above succeeding,
        // we can assume we have an insert and delete table.
        let (tx_table, tx_blob_store, del_table) = unsafe { self.tx_state.assume_present_get_mut_table(table_id) };
        for (col_id, seq_val) in cols_to_gen.iter().zip(seq_vals) {
            // SAFETY:
            // - `self.is_row_present(row)` holds as we haven't deleted the row.
            // - `col_id` is a valid column, and has a sequence, so it must have a primitive type.
            unsafe { tx_table.write_gen_val_to_col(col_id, tx_row_ptr, seq_val) };
        }
        // SAFETY: `tx_table.is_row_present(tx_row_ptr)` holds as we haven't deleted it yet.
        let tx_row_ref = unsafe { tx_table.get_row_ref_unchecked(tx_blob_store, tx_row_ptr) };

        let update_flags = UpdateFlags {
            is_scheduler_table: tx_table.is_scheduler(),
        };

        // 3. Find the old row and remove it.
        //----------------------------------------------------------------------
        #[inline]
        fn ensure_unique(index_id: IndexId, index: TableAndIndex<'_>) -> Result<()> {
            if !index.index().is_unique() {
                return Err(IndexError::NotUnique(index_id).into());
            }
            Ok(())
        }
        /// Ensure that the new row does not violate the commit table's unique constraints.
        #[inline]
        fn check_commit_unique_constraints(
            commit_table: &Table,
            del_table: &DeleteTable,
            ignore_index_id: IndexId,
            new_row: RowRef<'_>,
            old_ptr: RowPointer,
        ) -> Result<()> {
            let is_deleted = |commit_ptr| commit_ptr == old_ptr || del_table.contains(commit_ptr);
            // SAFETY: `commit_table.row_layout() == new_row.row_layout()` holds
            // as the `tx_table` is derived from `commit_table`.
            let res = unsafe {
                commit_table.check_unique_constraints(
                    new_row,
                    // Don't check this index since we'll do a 1-1 old/new replacement.
                    |ixs| ixs.filter(|(&id, _)| id != ignore_index_id),
                    is_deleted,
                )
            };
            res.map_err(IndexError::from).map_err(Into::into)
        }
        /// Projects the new row to the index to find the old row.
        #[inline]
        fn find_old_row(new_row: RowRef<'_>, index: TableAndIndex<'_>) -> (Option<RowPointer>, AlgebraicValue) {
            let index = index.index();
            // Project the row to the index's columns/type.
            // SAFETY: `new_row` belongs to the same table as `index`,
            // so all `index.indexed_columns` will be in-bounds of the row layout.
            let needle = unsafe { new_row.project_unchecked(&index.indexed_columns) };
            // Find the old row.
            (index.seek(&needle).next(), needle)
        }

        // The index we've been directed to use must exist
        // either in the committed state or in the tx state.
        // In the former case, the index must not have been removed in the transaction.
        // As it's unlikely that an index was added in this transaction,
        // we begin by checking the committed state.
        let err = 'failed_rev_ins: {
            let tx_row_ptr = if tx_removed_index {
                break 'failed_rev_ins IndexError::NotFound(index_id).into();
            } else if let Some((commit_index, old_ptr)) =
                // Find the committed state index, project the row to it, and find the old row.
                // The old row must not have been deleted.
                //
                // If the old row wasn't found, it may still exist in the tx state,
                // which inherits the index structure of the committed state,
                // so we'd like to avoid an early error in that case.
                self
                    .committed_state_write_lock
                    .get_index_by_id_with_table(table_id, index_id)
                    .and_then(|index| find_old_row(tx_row_ref, index).0.map(|ptr| (index, ptr)))
                    .filter(|(_, ptr)| !del_table.contains(*ptr))
            {
                // 1. Ensure the index is unique.
                // 2. Ensure the new row doesn't violate any other committed state unique indices.
                let commit_table = commit_index.table();
                if let Err(e) = ensure_unique(index_id, commit_index).and_then(|_| {
                    check_commit_unique_constraints(commit_table, del_table, index_id, tx_row_ref, old_ptr)
                }) {
                    break 'failed_rev_ins e;
                }

                // If the new row is the same as the old,
                // skip the update altogether to match the semantics of `Self::insert`.
                // SAFETY:
                // 1. `tx_table` is derived from `commit_table` so they have the same layouts.
                // 2. `old_ptr` was found in an index of `commit_table`, so we know it is valid.
                // 3. we just inserted `tx_row_ptr` into `tx_table`, so we know it is valid.
                if unsafe { Table::eq_row_in_page(commit_table, old_ptr, tx_table, tx_row_ptr) } {
                    // SAFETY: `self.is_row_present(tx_row_ptr)` holds, as noted in 3.
                    unsafe { tx_table.delete_internal_skip_pointer_map(tx_blob_store, tx_row_ptr) };
                    let commit_blob_store = &self.committed_state_write_lock.blob_store;
                    // SAFETY: `commit_table.is_row_present(old_ptr)` holds, as noted in 2.
                    let old_row_ref = unsafe { commit_table.get_row_ref_unchecked(commit_blob_store, old_ptr) };
                    return Ok((cols_to_gen, old_row_ref, update_flags));
                }

                // Check constraints and confirm the insertion of the new row.
                //
                // SAFETY: `self.is_row_present(row)` holds as we still haven't deleted the row,
                // in particular, the `write_gen_val_to_col` call does not remove the row.
                // On error, `tx_row_ptr` has already been removed, so don't do it again.
                let (_, tx_row_ptr) = unsafe { tx_table.confirm_insertion(tx_blob_store, tx_row_ptr, blob_bytes) }?;
                // Delete the old row.
                del_table.insert(old_ptr);
                tx_row_ptr
            } else if let Some(tx_index) =
                // Either the row was not found in the committed state index,
                // or the index was added in our tx state.
                // In the latter case, committed state rows will be present in the index,
                // so we must handle those specially.
                tx_table.get_index_by_id_with_table(tx_blob_store, index_id)
            {
                // 0. Find the old row.
                // 1. Ensure the index is unique.
                // 2. Ensure the new row doesn't violate any other committed state unique indices.
                let (old_ptr, needle) = find_old_row(tx_row_ref, tx_index);
                let commit_table = self.committed_state_write_lock.get_table(table_id);
                let res = old_ptr
                    // If we have an old committed state row, ensure it hasn't been deleted in our tx.
                    .filter(|ptr| ptr.squashed_offset() == SquashedOffset::TX_STATE || !del_table.contains(*ptr))
                    .ok_or_else(|| IndexError::KeyNotFound(index_id, needle).into())
                    .and_then(|old_ptr| {
                        ensure_unique(index_id, tx_index)?;
                        if let Some(commit_table) = commit_table {
                            check_commit_unique_constraints(commit_table, del_table, index_id, tx_row_ref, old_ptr)?;
                        }
                        Ok(old_ptr)
                    });
                let old_ptr = match res {
                    Err(e) => break 'failed_rev_ins e,
                    Ok(x) => x,
                };

                match old_ptr.squashed_offset() {
                    SquashedOffset::COMMITTED_STATE => {
                        if let Some(commit_table) = commit_table {
                            // If the new row is the same as the old,
                            // skip the update altogether to match the semantics of `Self::insert`.
                            // SAFETY:
                            // 1. `tx_table` is derived from `commit_table` so they have the same layouts.
                            // 2. `old_ptr` was found in an index of `tx_table`,
                            //     but we had `SquashedOffset::COMMITTED_STATE`,
                            //     so we know it is valid for `commit_table`.
                            // 3. we just inserted `tx_row_ptr` into `tx_table`, so we know it is valid.
                            if unsafe { Table::eq_row_in_page(commit_table, old_ptr, tx_table, tx_row_ptr) } {
                                // SAFETY: `self.is_row_present(tx_row_ptr)` holds, as noted in 3.
                                unsafe { tx_table.delete_internal_skip_pointer_map(tx_blob_store, tx_row_ptr) };
                                let commit_blob_store = &self.committed_state_write_lock.blob_store;
                                // SAFETY: `commit_table.is_row_present(old_ptr)` holds, as noted in 2.
                                let old_row_ref =
                                    unsafe { commit_table.get_row_ref_unchecked(commit_blob_store, old_ptr) };
                                return Ok((cols_to_gen, old_row_ref, update_flags));
                            }
                        }

                        // Check constraints and confirm the insertion of the new row.
                        //
                        // SAFETY: `self.is_row_present(row)` holds as we still haven't deleted the row,
                        // in particular, the `write_gen_val_to_col` call does not remove the row.
                        // On error, `tx_row_ptr` has already been removed, so don't do it again.
                        let (_, tx_row_ptr) =
                            unsafe { tx_table.confirm_insertion(tx_blob_store, tx_row_ptr, blob_bytes) }?;
                        // Delete the old row.
                        del_table.insert(old_ptr);
                        tx_row_ptr
                    }
                    SquashedOffset::TX_STATE => {
                        // Check constraints and confirm the update of the new row.
                        // This ensures that the old row is removed from the indices
                        // before attempting to insert the new row into the indices.
                        //
                        // SAFETY: `self.is_row_present(tx_row_ptr)` and `self.is_row_present(old_ptr)` both hold
                        // as we've deleted neither.
                        // In particular, the `write_gen_val_to_col` call does not remove the row.
                        unsafe { tx_table.confirm_update(tx_blob_store, tx_row_ptr, old_ptr, blob_bytes) }
                            .map_err(IndexError::UniqueConstraintViolation)?
                    }
                    _ => unreachable!("Invalid SquashedOffset for RowPointer: {:?}", old_ptr),
                }
            } else {
                break 'failed_rev_ins IndexError::NotFound(index_id).into();
            };

            // SAFETY: `tx_table.is_row_present(tx_row_ptr)` holds
            // per post-condition of `confirm_insertion` and `confirm_update`
            // in the if/else branches respectively.
            let tx_row_ref = unsafe { tx_table.get_row_ref_unchecked(tx_blob_store, tx_row_ptr) };
            return Ok((cols_to_gen, tx_row_ref, update_flags));
        };

        // When we reach here, we had an error and we need to revert the insertion of `tx_row_ref`.
        // SAFETY: `self.is_row_present(tx_row_ptr)` holds,
        // as we still haven't deleted the row physically.
        unsafe { tx_table.delete_internal_skip_pointer_map(tx_blob_store, tx_row_ptr) };
        Err(err)
    }

    pub(super) fn delete(&mut self, table_id: TableId, row_pointer: RowPointer) -> Result<bool> {
        match row_pointer.squashed_offset() {
            // For newly-inserted rows,
            // just delete them from the insert tables
            // - there's no reason to have them in both the insert and delete tables.
            SquashedOffset::TX_STATE => {
                let (table, blob_store) = self
                    .tx_state
                    .get_table_and_blob_store(table_id)
                    .ok_or(TableError::IdNotFoundState(table_id))?;
                Ok(table.delete(blob_store, row_pointer, |_| ()).is_some())
            }
            SquashedOffset::COMMITTED_STATE => {
                let commit_table = self
                    .committed_state_write_lock
                    .get_table(table_id)
                    .expect("there's a row in committed state so there should be a committed table");
                // NOTE: We trust the `row_pointer` refers to an extant row,
                // and check only that it hasn't yet been deleted.
                self.tx_state
                    .get_delete_table_mut(table_id, commit_table)
                    .insert(row_pointer);
                Ok(true)
            }
            _ => unreachable!("Invalid SquashedOffset for RowPointer: {:?}", row_pointer),
        }
    }

    pub(super) fn delete_by_row_value(&mut self, table_id: TableId, rel: &ProductValue) -> Result<bool> {
        // Four cases here:
        // - Table exists in both tx_state and committed_state.
        //   - Temporary insert into tx_state.
        //   - If match exists in tx_state, delete it immediately.
        //   - Else if match exists in committed_state, add to delete tables.
        //   - Roll back temp insertion.
        // - Table exists only in tx_state.
        //   - As above, but without else branch.
        // - Table exists only in committed_state.
        //   - Create table in tx_state, then as above.
        // - Table does not exist.
        //   - No such row; return false.

        let commit_table = self.committed_state_write_lock.get_table_mut(table_id);

        // If the tx table exists, get it.
        // If it doesn't exist, but the commit table does,
        // create the tx table using the commit table as a template.
        let Some((tx_table, tx_blob_store, ..)) = self
            .tx_state
            .get_table_and_blob_store_or_maybe_create_from(table_id, commit_table.as_deref())
        else {
            // If neither the committed table nor the tx table exists,
            // the row can't exist, so delete nothing.
            return Ok(false);
        };

        // We only want to physically insert the row here to get a row pointer.
        // We'd like to avoid any set semantic and unique constraint checks.
        let (row_ref, _) = tx_table.insert_physically_pv(tx_blob_store, rel)?;
        let ptr = row_ref.pointer();

        // First, check if a matching row exists in the `tx_table`.
        // If it does, no need to check the `commit_table`.
        //
        // SAFETY:
        // - `tx_table` trivially uses the same schema as itself.
        // - `ptr` is valid because we just inserted it.
        // - `hash` is correct because we just computed it.
        let (hash, to_delete) = unsafe { Table::find_same_row(tx_table, tx_table, tx_blob_store, ptr, None) };
        let to_delete = to_delete
            // Not present in insert tables? Check if present in the commit tables.
            .or_else(|| {
                commit_table.and_then(|commit_table| {
                    // SAFETY:
                    // - `commit_table` and `tx_table` use the same schema
                    // - `ptr` is valid because we just inserted it.
                    let (_, to_delete) =
                        unsafe { Table::find_same_row(commit_table, tx_table, tx_blob_store, ptr, hash) };
                    to_delete
                })
            });

        // Remove the temporary entry from the insert tables.
        // Do this before actually deleting to drop the borrows on the tables.
        // SAFETY: `ptr` is valid because we just inserted it and haven't deleted it since.
        unsafe {
            tx_table.delete_internal_skip_pointer_map(tx_blob_store, ptr);
        }

        // Mark the committed row to be deleted by adding it to the delete table.
        to_delete
            .map(|to_delete| self.delete(table_id, to_delete))
            .unwrap_or(Ok(false))
    }
}

impl StateView for MutTxId {
    type Iter<'a> = IterMutTx<'a>;
    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>> = IterByColRangeMutTx<'a, R>;
    type IterByColEq<'a, 'r>
        = IterByColEqMutTx<'a, 'r>
    where
        Self: 'a;

    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>> {
        // TODO(bikeshedding, docs): should this also check if the schema is in the system tables,
        // but the table hasn't been constructed yet?
        // If not, document why.
        self.tx_state
            .insert_tables
            .get(&table_id)
            .or_else(|| self.committed_state_write_lock.tables.get(&table_id))
            .map(|table| table.get_schema())
    }

    fn table_row_count(&self, table_id: TableId) -> Option<u64> {
        let commit_count = self.committed_state_write_lock.table_row_count(table_id);
        let (tx_ins_count, tx_del_count) = self.tx_state.table_row_count(table_id);
        let commit_count = commit_count.map(|cc| cc - tx_del_count);
        // Keep track of whether `table_id` exists.
        match (commit_count, tx_ins_count) {
            (Some(cc), Some(ic)) => Some(cc + ic),
            (Some(c), None) | (None, Some(c)) => Some(c),
            (None, None) => None,
        }
    }

    fn iter(&self, table_id: TableId) -> Result<Self::Iter<'_>> {
        if self.table_name(table_id).is_some() {
            return Ok(IterMutTx::new(
                table_id,
                &self.tx_state,
                &self.committed_state_write_lock,
            ));
        }
        Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into())
    }

    fn iter_by_col_range<R: RangeBounds<AlgebraicValue>>(
        &self,
        table_id: TableId,
        cols: ColList,
        range: R,
    ) -> Result<Self::IterByColRange<'_, R>> {
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
        if let Some(inserted_rows) = self.tx_state.index_seek_by_cols(table_id, &cols, &range) {
            let committed_rows = self.committed_state_write_lock.index_seek(table_id, &cols, &range);
            // The current transaction has modified this table, and the table is indexed.
            Ok(if let Some(del_table) = self.tx_state.get_delete_table(table_id) {
                IterByColRangeMutTx::IndexWithDeletes(IndexSeekIterIdWithDeletedMutTx {
                    inserted_rows,
                    committed_rows,
                    del_table,
                })
            } else {
                IterByColRangeMutTx::Index(IndexSeekIterIdMutTx {
                    inserted_rows,
                    committed_rows,
                })
            })
        } else {
            // Either the current transaction has not modified this table, or the table is not
            // indexed.
            match self.committed_state_write_lock.index_seek(table_id, &cols, &range) {
                Some(committed_rows) => Ok(if let Some(del_table) = self.tx_state.get_delete_table(table_id) {
                    IterByColRangeMutTx::CommittedIndexWithDeletes(CommittedIndexIterWithDeletedMutTx::new(
                        committed_rows,
                        del_table,
                    ))
                } else {
                    IterByColRangeMutTx::CommittedIndex(committed_rows)
                }),
                None => {
                    #[cfg(feature = "unindexed_iter_by_col_range_warn")]
                    match self.table_row_count(table_id) {
                        // TODO(ux): log these warnings to the module logs rather than host logs.
                        None => log::error!(
                            "iter_by_col_range on unindexed column, but couldn't fetch table `{table_id}`s row count",
                        ),
                        Some(num_rows) => {
                            const TOO_MANY_ROWS_FOR_SCAN: u64 = 1000;
                            if num_rows >= TOO_MANY_ROWS_FOR_SCAN {
                                let schema = self.schema_for_table(table_id).unwrap();
                                let table_name = &schema.table_name;
                                let col_names = cols
                                    .iter()
                                    .map(|col_id| {
                                        schema
                                            .columns()
                                            .get(col_id.idx())
                                            .map(|col| &col.col_name[..])
                                            .unwrap_or("[unknown column]")
                                    })
                                    .collect::<Vec<_>>();
                                log::warn!(
                                    "iter_by_col_range without index: table {table_name} has {num_rows} rows; scanning columns {col_names:?}",
                                );
                            }
                        }
                    }

                    Ok(IterByColRangeMutTx::Scan(ScanIterByColRangeMutTx::new(
                        self.iter(table_id)?,
                        cols,
                        range,
                    )))
                }
            }
        }
    }

    fn iter_by_col_eq<'a, 'r>(
        &'a self,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>> {
        self.iter_by_col_range(table_id, cols.into(), value)
    }
}
