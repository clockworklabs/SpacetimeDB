use super::{
    committed_state::{CommittedIndexIter, CommittedState},
    datastore::{record_metrics, Result},
    sequence::{Sequence, SequencesState},
    state_view::{IndexSeekIterMutTxId, Iter, IterByColRange, ScanIterByColRange, StateView},
    tx::TxId,
    tx_state::{DeleteTable, IndexIdMap, TxState},
    SharedMutexGuard, SharedWriteGuard,
};
use crate::db::datastore::{
    system_tables::{
        StColumnFields, StColumnRow, StConstraintFields, StConstraintRow, StFields as _, StIndexFields, StIndexRow,
        StScheduledFields, StScheduledRow, StSequenceFields, StSequenceRow, StTableFields, StTableRow, SystemTable,
        ST_COLUMN_ID, ST_CONSTRAINT_ID, ST_INDEX_ID, ST_SCHEDULED_ID, ST_SEQUENCE_ID, ST_TABLE_ID,
    },
    traits::{RowTypeForTable, TxData},
};
use crate::execution_context::Workload;
use crate::{
    db::datastore::system_tables::{StRowLevelSecurityFields, StRowLevelSecurityRow, ST_ROW_LEVEL_SECURITY_ID},
    energy::DatastoreComputeDuration,
};
use crate::{
    error::{IndexError, SequenceError, TableError},
    execution_context::ExecutionContext,
};
use core::ops::RangeBounds;
use core::{iter, ops::Bound};
use smallvec::SmallVec;
use spacetimedb_lib::db::raw_def::v9::RawSql;
use spacetimedb_lib::{
    bsatn::Deserializer,
    db::{auth::StAccess, raw_def::SEQUENCE_ALLOCATION_STEP},
    de::DeserializeSeed,
};
use spacetimedb_primitives::{ColId, ColList, ColSet, ConstraintId, IndexId, ScheduleId, SequenceId, TableId};
use spacetimedb_sats::{
    bsatn::{self, DecodeError},
    de::WithBound,
    AlgebraicType, AlgebraicValue, ProductType, ProductValue, WithTypespace,
};
use spacetimedb_schema::{
    def::{BTreeAlgorithm, IndexAlgorithm},
    schema::{ConstraintSchema, IndexSchema, RowLevelSecuritySchema, SequenceSchema, TableSchema},
};
use spacetimedb_table::{
    blob_store::{BlobStore, HashMapBlobStore},
    indexes::{RowPointer, SquashedOffset},
    table::{IndexScanIter, InsertError, RowRef, Table},
};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
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

    /// Time spent performing operations that access the database,
    /// for which energy will be charged.
    ///
    /// Will be used as a shared counter with [`Ordering::Relaxed`] atomics.
    /// Does not provide any synchronization.
    /// This means that accesses should mostly compile into relatively-efficient unfenced operations.
    /// We expect these accesses to be uncontended, as all of `MutTxId`'s interesting methods take `&mut self`,
    /// thus requiring a single unique caller.
    ///
    /// Ideally we would use a non-atomic [`std::cell::Cell`] here,
    /// but our query engine is written to abstract away whether TXes are mutable or not at runtime,
    /// and also executes certain queries in parallel,
    /// meaning that we effectively assert `MutTxId: Sync`.
    pub(super) datastore_compute_time_microseconds: AtomicU64,
}

/// Non-panicking version of [`Instant::elapsed`].
/// Clamps to zero if [`Instant::now`] returns a time before `before`.
pub(super) fn elapsed_or_zero(before: Instant) -> Duration {
    // Don't call `before.elapsed`, as that method reserves the right to panic
    // if the `Instant` clock is non-monotonic.
    let after = Instant::now();
    after.checked_duration_since(before).unwrap_or_default()
}

impl MutTxId {
    /// Run `f` on `self` while tracking database compute time for `self`.
    ///
    /// Note that we are unable to allow `Res` to borrow from `self`,
    /// as this method needs access to `self` both before and after running `f`.
    /// For a variant which allows the result to borrow from `self`,
    /// see [`Self::while_tracking_compute_time`].
    ///
    /// This method must never be called re-entrantly from the `f` of either this method
    /// or [`Self::while_tracking_compute_time`].
    /// Doing so will double-count compute time in release builds.
    /// However, it is acceptable for multiple actors
    /// to call [`Self::while_tracking_compute_time`] in parallel.
    /// In this case, the total tracked compute time will be the sum of all the callers' durations.
    ///
    /// Most, if not all, `pub` methods of `MutTxId` should have their bodies enclosed
    /// in either this or [`Self::while_tracking_compute_time`].
    fn while_tracking_compute_time_mut<Res>(&mut self, f: impl FnOnce(&mut Self) -> Res) -> Res {
        let before = Instant::now();
        let res = f(self);
        let elapsed = elapsed_or_zero(before);

        self.datastore_compute_time_microseconds
            // `self.datastore_compute_time_microseconds` is not used for any synchronization;
            // it is strictly a shared counter. As such, `Ordering::Relaxed` is sufficient.
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);

        res
    }

    /// Run `f` on `self` while tracking database compute time for `self`.
    ///
    /// Odd signature with explicit lifetimes here to allow `Res` to borrow from `self`.
    ///
    /// This method must never be called re-entrantly from the `f` of either this method
    /// or [`Self::while_tracking_compute_time_mut`].
    /// Doing so will double-count compute time in release builds.
    /// However, it is acceptable for multiple actors to call this method in parallel.
    /// In this case, the total tracked compute time will be the sum of all the callers' durations.
    ///
    /// Most, if not all, `pub` methods of `MutTxId` should have their bodies enclosed
    /// in either this or [`Self::while_tracking_compute_time`].
    fn while_tracking_compute_time<'a, Res>(&'a self, f: impl FnOnce(&'a Self) -> Res) -> Res {
        let before = Instant::now();
        let res = f(self);
        let elapsed = elapsed_or_zero(before);

        self.datastore_compute_time_microseconds
            // `self.datastore_compute_time_microseconds` is not used for any synchronization;
            // it is strictly a shared counter. As such, `Ordering::Relaxed` is sufficient.
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);

        res
    }

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
        self.while_tracking_compute_time_mut(|this| {
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
            let table_id = this
                .insert(ST_TABLE_ID, &mut row.into())?
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
                this.insert(ST_COLUMN_ID, &mut row.into())?;
            }

            let mut schema_internal = table_schema.clone();
            // Remove all indexes, constraints, and sequences from the schema; we will add them back later with correct index_id, ...
            schema_internal.clear_adjacent_schemas();

            // Create the in memory representation of the table
            // NOTE: This should be done before creating the indexes
            // NOTE: This `TableSchema` will be updated when we call `create_...` below.
            //       This allows us to create the indexes, constraints, and sequences with the correct `index_id`, ...
            this.create_table_internal(schema_internal.into());

            // Insert the scheduled table entry into `st_scheduled`
            if let Some(schedule) = table_schema.schedule {
                let row = StScheduledRow {
                    table_id: schedule.table_id,
                    schedule_id: ScheduleId::SENTINEL,
                    schedule_name: schedule.schedule_name,
                    reducer_name: schedule.reducer_name,
                    at_column: schedule.at_column,
                };
                let (generated, ..) = this.insert(ST_SCHEDULED_ID, &mut row.into())?;
                let id = generated.as_u32();

                if let Some(&id) = id {
                    let (table, ..) = this.get_or_create_insert_table_mut(table_id)?;
                    table.with_mut_schema(|s| s.schedule.as_mut().unwrap().schedule_id = id.into());
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to generate a schedule ID for table: {}, generated: {:#?}",
                        table_schema.table_name,
                        generated
                    )
                    .into());
                }
            }

            // Insert constraints into `st_constraints`
            for constraint in table_schema.constraints.iter().cloned() {
                this.create_constraint(constraint)?;
            }

            // Insert sequences into `st_sequences`
            for seq in table_schema.sequences {
                this.create_sequence_internal(seq)?;
            }

            // Create the indexes for the table
            for index in table_schema.indexes {
                let col_set = ColSet::from(index.index_algorithm.columns());
                let is_unique = table_schema
                    .constraints
                    .iter()
                    .any(|c| c.data.unique_columns() == Some(&col_set));
                this.create_index_internal(index, is_unique)?;
            }

            log::trace!("TABLE CREATED: {}, table_id: {table_id}", table_schema.table_name);

            Ok(table_id)
        })
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
        self.while_tracking_compute_time(|this| {
            // Fetch the `ProductType` from the in memory table if it exists.
            // The `ProductType` is invalidated if the schema of the table changes.
            if let Some(row_type) = this.get_row_type(table_id) {
                return Ok(RowTypeForTable::Ref(row_type));
            }

            // Look up the columns for the table in question.
            // NOTE: This is quite an expensive operation, although we only need
            // to do this in situations where there is not currently an in memory
            // representation of a table. This would happen in situations where
            // we have created the table in the database, but have not yet
            // represented in memory or inserted any rows into it.
            Ok(RowTypeForTable::Arc(this.schema_for_table(table_id)?))
        })
    }

    pub fn drop_table(&mut self, table_id: TableId) -> Result<()> {
        self.while_tracking_compute_time_mut(|this| {
            let schema = &*this.schema_for_table(table_id)?;

            for row in &schema.indexes {
                this.drop_index(row.index_id)?;
            }

            for row in &schema.sequences {
                this.drop_sequence(row.sequence_id)?;
            }

            for row in &schema.constraints {
                this.drop_constraint(row.constraint_id)?;
            }

            // Drop the table and their columns
            this.drop_col_eq(ST_TABLE_ID, StTableFields::TableId.col_id(), &table_id.into())?;
            this.drop_col_eq(ST_COLUMN_ID, StColumnFields::TableId.col_id(), &table_id.into())?;

            if let Some(schedule) = &schema.schedule {
                this.drop_col_eq(
                    ST_SCHEDULED_ID,
                    StScheduledFields::ScheduleId.col_id(),
                    &schedule.schedule_id.into(),
                )?;
            }

            // Delete the table and its rows and indexes from memory.
            // TODO: This needs to not remove it from the committed state, because it can still be rolled back.
            // We will have to store the deletion in the TxState and then apply it to the CommittedState in commit.

            // NOT use unwrap
            this.committed_state_write_lock.tables.remove(&table_id);
            Ok(())
        })
    }

    pub fn rename_table(&mut self, table_id: TableId, new_name: &str) -> Result<()> {
        self.while_tracking_compute_time_mut(|this| {
            // Update the table's name in st_tables.
            this.update_st_table_row(table_id, |st| st.table_name = new_name.into())
        })
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
        self.insert(ST_TABLE_ID, &mut row.into())?;

        Ok(())
    }

    pub fn table_id_from_name(&self, table_name: &str) -> Result<Option<TableId>> {
        self.while_tracking_compute_time(|this| {
            let table_name = &table_name.into();
            let row = this
                .iter_by_col_eq(ST_TABLE_ID, StTableFields::TableName, table_name)?
                .next();
            Ok(row.map(|row| row.read_col(StTableFields::TableId).unwrap()))
        })
    }

    pub fn table_name_from_id(&self, table_id: TableId) -> Result<Option<Box<str>>> {
        self.while_tracking_compute_time(|this| {
            this.iter_by_col_eq(ST_TABLE_ID, StTableFields::TableId, &table_id.into())
                .map(|mut iter| iter.next().map(|row| row.read_col(StTableFields::TableName).unwrap()))
        })
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
        self.while_tracking_compute_time_mut(|this| {
            // Write to the table in the tx state.
            let (table, ..) = this.get_or_create_insert_table_mut(table_id)?;
            table.with_mut_schema(|s| s.table_access = access);

            // Update system tables.
            this.update_st_table_row(table_id, |st| st.table_access = access)?;
            Ok(())
        })
    }

    /// Called by [`Self::create_index`] under [`Self::while_tracking_compute_time_mut`],
    /// and also by [`Self::create_table`].
    ///
    /// Because [`Self::while_tracking_compute_time_mut`] is not re-entrant,
    /// we need to separate the public interface which does timing measurements
    /// from the internal implementation.
    fn create_index_internal(&mut self, mut index: IndexSchema, is_unique: bool) -> Result<IndexId> {
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
            .insert(ST_INDEX_ID, &mut row.into())?
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
            _ => unimplemented!(),
        };
        // Create and build the index.
        let mut insert_index = table.new_index(index.index_id, &columns, is_unique)?;
        insert_index.build_from_rows(&columns, table.scan_rows(blob_store))?;
        // NOTE: Also add all the rows in the already committed table to the index.
        // FIXME: Is this correct? Index scan iterators (incl. the existing `Locking` versions)
        // appear to assume that a table's index refers only to rows within that table,
        // and does not handle the case where a `TxState` index refers to `CommittedState` rows.
        if let Some(committed_table) = commit_table {
            insert_index.build_from_rows(&columns, committed_table.scan_rows(commit_blob_store))?;
        }
        table.indexes.insert(columns.clone(), insert_index);
        // Associate `index_id -> (table_id, col_list)` for fast lookup.
        idx_map.insert(index_id, (table_id, columns.clone()));

        log::trace!(
            "INDEX CREATED: {} for table: {} and col(s): {:?}",
            index_id,
            table_id,
            columns
        );
        // Update the table's schema.
        // This won't clone-write when creating a table but likely to otherwise.
        table.with_mut_schema(|s| s.indexes.push(index));

        Ok(index_id)
    }

    /// Create an index.
    ///
    /// Thin public wrapper around [`Self::create_index_internal`] which does compute timing.
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
    pub fn create_index(&mut self, index: IndexSchema, is_unique: bool) -> Result<IndexId> {
        self.while_tracking_compute_time_mut(|this| this.create_index_internal(index, is_unique))
    }

    pub fn drop_index(&mut self, index_id: IndexId) -> Result<()> {
        self.while_tracking_compute_time_mut(|this| {
            log::trace!("INDEX DROPPING: {}", index_id);
            // Find the index in `st_indexes`.
            let st_index_ref = this
                .iter_by_col_eq(ST_INDEX_ID, StIndexFields::IndexId, &index_id.into())?
                .next()
                .ok_or_else(|| TableError::IdNotFound(SystemTable::st_index, index_id.into()))?;
            let st_index_row = StIndexRow::try_from(st_index_ref)?;
            let st_index_ptr = st_index_ref.pointer();
            let table_id = st_index_row.table_id;

            // Remove the index from st_indexes.
            this.delete(ST_INDEX_ID, st_index_ptr)?;

            // Remove the index in the transaction's insert table.
            // By altering the insert table, this gets moved over to the committed state on merge.
            let (table, _, idx_map, ..) = this.get_or_create_insert_table_mut(table_id)?;
            if let Some(col) = table
                .indexes
                .iter()
                .find(|(_, idx)| idx.index_id == index_id)
                .map(|(cols, _)| cols.clone())
            {
                // This likely will do a clone-write as over time?
                // The schema might have found other referents.
                table.with_mut_schema(|s| s.indexes.retain(|x| x.index_algorithm.columns() != &col));
                table.indexes.remove(&col);
            }
            // Remove the `index_id -> (table_id, col_list)` association.
            idx_map.remove(&index_id);
            this.tx_state
                .index_id_map_removals
                .get_or_insert_with(Default::default)
                .insert(index_id);

            log::trace!("INDEX DROPPED: {}", index_id);
            Ok(())
        })
    }

    pub fn index_id_from_name(&self, index_name: &str) -> Result<Option<IndexId>> {
        self.while_tracking_compute_time(|this| {
            let name = &index_name.into();
            let row = this.iter_by_col_eq(ST_INDEX_ID, StIndexFields::IndexName, name)?.next();
            Ok(row.map(|row| row.read_col(StIndexFields::IndexId).unwrap()))
        })
    }

    /// Returns an iterator yielding rows by performing a btree index scan
    /// on the btree index identified by `index_id`.
    ///
    /// The `prefix` is equated to the first `prefix_elems` values of the index key
    /// and then `prefix_elem`th value is bounded to the left by by `rstart`
    /// and to the right by `rend`.
    pub fn btree_scan<'a>(
        &'a self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<(TableId, BTreeScan<'a>)> {
        // TODO(energy): track compute time spent in the iterator.
        // This is challenging to do while maintaining non-reentrancy,
        // and there are concerns about overhead - wrapping the body of `next`
        // in (the equivalent of) `while_tracking_compute_time` means two clock reads for every row,
        // which is potentially too much.

        // Extract the table and index type for the tx state.
        let (table_id, col_list, tx_idx_key_type) = self
            .get_table_and_index_type(index_id)
            .ok_or_else(|| IndexError::NotFound(index_id))?;

        // TODO(centril): Once we have more index types than `btree`,
        // we'll need to enforce that `index_id` refers to a btree index.

        // We have the index key type, so we can decode everything.
        let bounds = Self::btree_decode_bounds(tx_idx_key_type, prefix, prefix_elems, rstart, rend)
            .map_err(IndexError::Decode)?;

        // Get an index seek iterator for the tx and committed state.
        let tx_iter = self.tx_state.index_seek(table_id, col_list, &bounds);
        let commit_iter = self.committed_state_write_lock.index_seek(table_id, col_list, &bounds);

        // Chain together the indexed rows in the tx and committed state,
        // but don't yield rows deleted in the tx state.
        use itertools::Either::*;
        use BTreeScanInner::*;
        let commit_iter = commit_iter.map(|iter| match self.tx_state.delete_tables.get(&table_id) {
            None => Left(iter),
            Some(deletes) => Right(IndexScanFilterDeleted { iter, deletes }),
        });
        // this is effectively just `tx_iter.into_iter().flatten().chain(commit_iter.into_iter().flatten())`,
        // but with all the branching and `Option`s flattened to just one layer.
        let iter = match (tx_iter, commit_iter) {
            (None, None) => Empty(iter::empty()),
            (Some(tx_iter), None) => TxOnly(tx_iter),
            (None, Some(Left(commit_iter))) => CommitOnly(commit_iter),
            (None, Some(Right(commit_iter))) => CommitOnlyWithDeletes(commit_iter),
            (Some(tx_iter), Some(Left(commit_iter))) => Both(tx_iter.chain(commit_iter)),
            (Some(tx_iter), Some(Right(commit_iter))) => BothWithDeletes(tx_iter.chain(commit_iter)),
        };
        Ok((table_id, BTreeScan { inner: iter }))
    }

    /// Translate `index_id` to the table id, the column list and index key type.
    fn get_table_and_index_type(&self, index_id: IndexId) -> Option<(TableId, &ColList, &AlgebraicType)> {
        // The order of querying the committed vs. tx state for the translation is not important.
        // But it is vastly more likely that it is in the committed state,
        // so query that first to avoid two lookups.
        let &(table_id, ref col_list) = self
            .committed_state_write_lock
            .index_id_map
            .get(&index_id)
            .or_else(|| self.tx_state.index_id_map.get(&index_id))?;

        // The tx state must have the index.
        // If the index was e.g., dropped from the tx state but exists physically in the committed state,
        // the index does not exist, semantically.
        // TODO: handle the case where the table has been dropped in this transaction.
        let key_ty = if let Some(key_ty) = self
            .committed_state_write_lock
            .get_table_and_index_type(table_id, col_list)
        {
            if self
                .tx_state
                .index_id_map_removals
                .as_ref()
                .is_some_and(|s| s.contains(&index_id))
            {
                return None;
            }
            key_ty
        } else {
            self.tx_state.get_table_and_index_type(table_id, col_list)?
        };

        Some((table_id, col_list, key_ty))
    }

    /// Decode the bounds for a btree scan for an index typed at `key_type`.
    fn btree_decode_bounds(
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
                // TODO(centril): replace with `.split_at_checked(...)`.
                if key_types.len() < prefix_elems.idx() {
                    return Err(DecodeError::Other(
                        "index key type has too few fields compared to prefix".into(),
                    ));
                }
                let (prefix_types, rest_types) = key_types.split_at(prefix_elems.idx());

                // The `rstart` and `rend`s must be typed at `Bound<range_type>`.
                // Extract that type and determine the length of the suffix.
                let Some((range_type, suffix_types)) = rest_types.split_first() else {
                    return Err(DecodeError::Other(
                        "prefix length leaves no room for a range in btree index scan".into(),
                    ));
                };
                let suffix_len = suffix_types.len();

                // We now have the types,
                // so proceed to decoding the prefix, and the start/end bounds.
                // Finally combine all of these to a single bound pair.
                let prefix = bsatn::decode(prefix_types, &mut prefix)?;
                let (start, end) = Self::btree_decode_ranges(&range_type.algebraic_type, rstart, rend)?;
                Ok(Self::btree_combine_prefix_and_bounds(prefix, start, end, suffix_len))
            }
            // Single-column index case. We implicitly have a PT of len 1.
            _ if !prefix.is_empty() && prefix_elems.idx() != 0 => Err(DecodeError::Other(
                "a single-column index cannot be prefix scanned".into(),
            )),
            ty => Self::btree_decode_ranges(ty, rstart, rend),
        }
    }

    /// Decode `rstart` and `rend` as `Bound<ty>`.
    fn btree_decode_ranges(
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
    fn btree_combine_prefix_and_bounds(
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

    /// External logic of [`Self::get_next_sequence_value`].
    ///
    /// Because [`Self::while_tracking_compute_time_mut`] is non-reentrant,
    /// internal callers need to access the inner logic without double-counting compute time.
    fn get_next_sequence_value_internal(&mut self, seq_id: SequenceId) -> Result<i128> {
        {
            let Some(sequence) = self.sequence_state_lock.get_sequence_mut(seq_id) else {
                return Err(SequenceError::NotFound(seq_id).into());
            };

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

            let Some(sequence) = self.sequence_state_lock.get_sequence_mut(seq_id) else {
                return Err(SequenceError::NotFound(seq_id).into());
            };
            seq_row.allocated = sequence.nth_value(SEQUENCE_ALLOCATION_STEP as usize);
            sequence.set_allocation(seq_row.allocated);
            seq_row
        };

        self.delete(ST_SEQUENCE_ID, old_seq_row_ptr)?;
        // `insert_row_internal` rather than `insert` because:
        // - We have already checked unique constraints during `create_sequence`.
        // - Similarly, we have already applied autoinc sequences.
        // - We do not want to apply autoinc sequences again,
        //   since the system table sequence `seq_st_table_table_id_primary_key_auto`
        //   has ID 0, and would otherwise trigger autoinc.
        self.insert_row_internal(ST_SEQUENCE_ID, &ProductValue::from(seq_row))?;

        let Some(sequence) = self.sequence_state_lock.get_sequence_mut(seq_id) else {
            return Err(SequenceError::NotFound(seq_id).into());
        };
        if let Some(value) = sequence.gen_next_value() {
            return Ok(value);
        }
        Err(SequenceError::UnableToAllocate(seq_id).into())
    }

    /// External interface to [`Self::get_next_sequence_value_internal`],
    /// which wraps that method in [`Self::while_tracking_compute_time_mut`].
    ///
    /// Because [`Self::while_tracking_compute_time_mut`] is non-reentrant,
    /// internal callers need to access the inner logic without double-counting compute time.
    pub fn get_next_sequence_value(&mut self, seq_id: SequenceId) -> Result<i128> {
        self.while_tracking_compute_time_mut(|this| this.get_next_sequence_value_internal(seq_id))
    }

    /// External logic of [`Self::create_sequence`].
    ///
    /// Because [`Self::while_tracking_compute_time_mut`] is non-reentrant,
    /// internal callers need to access the inner logic without double-counting compute time.
    fn create_sequence_internal(&mut self, seq: SequenceSchema) -> Result<SequenceId> {
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
        let row = self.insert(ST_SEQUENCE_ID, &mut sequence_row.clone().into())?;
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

    /// Create a sequence.
    /// Requires:
    /// - `seq.sequence_id == SequenceId::SENTINEL`
    /// - `seq.table_id != TableId::SENTINEL`
    /// - `seq.sequence_name` must not be used for any other database entity.
    ///
    /// Ensures:
    /// - The sequence metadata is inserted into the system tables (and other data structures reflecting them).
    /// - The returned ID is unique and not `SequenceId::SENTINEL`.
    ///
    /// External interface to [`Self::create_sequence_internal`],
    /// which wraps that method in [`Self::while_tracking_compute_time_mut`].
    ///
    /// Because [`Self::while_tracking_compute_time_mut`] is non-reentrant,
    /// internal callers need to access the inner logic without double-counting compute time.
    pub fn create_sequence(&mut self, seq: SequenceSchema) -> Result<SequenceId> {
        self.while_tracking_compute_time_mut(|this| this.create_sequence_internal(seq))
    }

    pub fn drop_sequence(&mut self, sequence_id: SequenceId) -> Result<()> {
        self.while_tracking_compute_time_mut(|this| {
            let st_sequence_ref = this
                .iter_by_col_eq(ST_SEQUENCE_ID, StSequenceFields::SequenceId, &sequence_id.into())?
                .next()
                .ok_or_else(|| TableError::IdNotFound(SystemTable::st_sequence, sequence_id.into()))?;
            let table_id = st_sequence_ref.read_col(StSequenceFields::TableId)?;

            this.delete(ST_SEQUENCE_ID, st_sequence_ref.pointer())?;

            // TODO: Transactionality.
            // Currently, a TX which drops a sequence then aborts
            // will leave the sequence deleted,
            // rather than restoring it during rollback.
            this.sequence_state_lock.remove(sequence_id);
            if let Some((insert_table, _)) = this.tx_state.get_table_and_blob_store(table_id) {
                // This likely will do a clone-write as over time?
                // The schema might have found other referents.
                insert_table.with_mut_schema(|s| s.remove_sequence(sequence_id));
            }
            Ok(())
        })
    }

    pub fn sequence_id_from_name(&self, seq_name: &str) -> Result<Option<SequenceId>> {
        self.while_tracking_compute_time(|this| {
            let name = &<Box<str>>::from(seq_name).into();
            this.iter_by_col_eq(ST_SEQUENCE_ID, StSequenceFields::SequenceName, name)
                .map(|mut iter| {
                    iter.next()
                        .map(|row| row.read_col(StSequenceFields::SequenceId).unwrap())
                })
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

        let constraint_row = self.insert(ST_CONSTRAINT_ID, &mut ProductValue::from(constraint_row))?;
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
        self.while_tracking_compute_time_mut(|this| {
            // Delete row in `st_constraint`.
            let st_constraint_ref = this
                .iter_by_col_eq(
                    ST_CONSTRAINT_ID,
                    StConstraintFields::ConstraintId,
                    &constraint_id.into(),
                )?
                .next()
                .ok_or_else(|| TableError::IdNotFound(SystemTable::st_constraint, constraint_id.into()))?;
            let table_id = st_constraint_ref.read_col(StConstraintFields::TableId)?;
            this.delete(ST_CONSTRAINT_ID, st_constraint_ref.pointer())?;

            // Remove constraint in transaction's insert table.
            let (table, ..) = this.get_or_create_insert_table_mut(table_id)?;
            // This likely will do a clone-write as over time?
            // The schema might have found other referents.
            table.with_mut_schema(|s| s.remove_constraint(constraint_id));
            // TODO(1.0): we should also re-initialize `table` without a unique constraint.
            // unless some other unique constraint on the same columns exists.

            Ok(())
        })
    }

    pub fn constraint_id_from_name(&self, constraint_name: &str) -> Result<Option<ConstraintId>> {
        self.while_tracking_compute_time(|this| {
            this.iter_by_col_eq(
                ST_CONSTRAINT_ID,
                StConstraintFields::ConstraintName,
                &<Box<str>>::from(constraint_name).into(),
            )
            .map(|mut iter| {
                iter.next()
                    .map(|row| row.read_col(StConstraintFields::ConstraintId).unwrap())
            })
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
        self.while_tracking_compute_time_mut(|this| {
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

            let row = this.insert(ST_ROW_LEVEL_SECURITY_ID, &mut ProductValue::from(row))?;
            let row_level_security_sql = row.1.collapse().read_col(StRowLevelSecurityFields::Sql)?;
            let existed = matches!(row.1, RowRefInsertion::Existed(_));

            // Add the row level security to the transaction's insert table.
            this.get_or_create_insert_table_mut(row_level_security_schema.table_id)?;

            if existed {
                log::trace!("ROW LEVEL SECURITY ALREADY EXISTS: {row_level_security_sql}");
            } else {
                log::trace!("ROW LEVEL SECURITY CREATED: {row_level_security_sql}");
            }

            Ok(row_level_security_sql)
        })
    }

    pub fn row_level_security_for_table_id(&self, table_id: TableId) -> Result<Vec<RowLevelSecuritySchema>> {
        self.while_tracking_compute_time(|this| {
            Ok(this
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
        })
    }

    pub fn drop_row_level_security(&mut self, sql: RawSql) -> Result<()> {
        self.while_tracking_compute_time_mut(|this| {
            let st_rls_ref = this
                .iter_by_col_eq(
                    ST_ROW_LEVEL_SECURITY_ID,
                    StRowLevelSecurityFields::Sql,
                    &sql.clone().into(),
                )?
                .next()
                .ok_or_else(|| TableError::RawSqlNotFound(SystemTable::st_row_level_security, sql))?;
            this.delete(ST_ROW_LEVEL_SECURITY_ID, st_rls_ref.pointer())?;

            Ok(())
        })
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
        self.while_tracking_compute_time(|this| {
            if this.table_name(table_id).is_none() {
                return Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into());
            }
            Ok(match row_ptr.squashed_offset() {
                SquashedOffset::TX_STATE => Some(
                    // TODO(perf, deep-integration):
                    // See above. Once `TxState::get` is unsafe, justify with:
                    //
                    // Our invariants satisfy `TxState::get`.
                    this.tx_state.get(table_id, row_ptr),
                ),
                SquashedOffset::COMMITTED_STATE => {
                    if this.tx_state.is_deleted(table_id, row_ptr) {
                        None
                    } else {
                        Some(
                            // TODO(perf, deep-integration):
                            // See above. Once `CommittedState::get` is unsafe, justify with:
                            //
                            // Our invariants satisfy `CommittedState::get`.
                            this.committed_state_write_lock.get(table_id, row_ptr),
                        )
                    }
                }
                _ => unreachable!("Invalid SquashedOffset for row pointer: {:?}", row_ptr),
            })
        })
    }

    /// Returns as two values:
    /// - A [`TxData`] containing all the mutations performed by the TX,
    ///   which can be used to compute incremental queries.
    /// - A [`Duration`] representing the total time spent performing datastore operations
    ///   during the transaction, for which energy should be charged.
    pub fn commit(self) -> (TxData, DatastoreComputeDuration) {
        let Self {
            mut committed_state_write_lock,
            tx_state,
            datastore_compute_time_microseconds,
            ..
        } = self;

        let before_merge = Instant::now();
        let tx_data = committed_state_write_lock.merge(tx_state, &self.ctx);
        let merge_elapsed = elapsed_or_zero(before_merge);

        // `self.datastore_compute_time_microseconds` is not used for any synchronization;
        // it is strictly a shared counter. As such, `Ordering::Relaxed` is sufficient.
        //
        // Note that the use of `Ordering::Relaxed` here means that we may "lose" some `fetch_add`s,
        // as the thread which does `Self::release` may observe them after its load.
        let non_merge_duration = Duration::from_micros(datastore_compute_time_microseconds.into_inner());

        let total_compute_time = non_merge_duration.saturating_add(merge_elapsed);

        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_metrics(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            true,
            Some(&tx_data),
            Some(&committed_state_write_lock),
        );
        (tx_data, DatastoreComputeDuration(total_compute_time))
    }

    /// Commit the mutations in `self` and convert it into an immutable [`TxId`].
    ///
    /// The returned [`TxId`] will inherit the mutable transaction's datastore compute time.
    /// Care should be taken to charge energy when releasing the [`TxId`].
    pub fn commit_downgrade(mut self, workload: Workload) -> (TxData, TxId) {
        let Self {
            mut committed_state_write_lock,
            tx_state,
            datastore_compute_time_microseconds,
            ..
        } = self;

        let before_merge = Instant::now();
        let tx_data = committed_state_write_lock.merge(tx_state, &self.ctx);
        let merge_elapsed = elapsed_or_zero(before_merge);

        // `self.datastore_compute_time_microseconds` is not used for any synchronization;
        // it is strictly a shared counter. As such, `Ordering::Relaxed` is sufficient.
        datastore_compute_time_microseconds.fetch_add(merge_elapsed.as_micros() as u64, Ordering::Relaxed);

        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_metrics(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            true,
            Some(&tx_data),
            Some(&committed_state_write_lock),
        );
        // Update the workload type of the execution context
        self.ctx.workload = workload.into();
        let tx = TxId {
            committed_state_shared_lock: SharedWriteGuard::downgrade(committed_state_write_lock),
            lock_wait_time: Duration::ZERO,
            timer: Instant::now(),
            ctx: self.ctx,
            datastore_compute_time_microseconds,
        };

        (tx_data, tx)
    }

    /// Returns a [`Duration`] representing the total time spent performing datastore operations
    /// during the transaction, for which energy should be charged.
    pub fn rollback(self) -> DatastoreComputeDuration {
        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_metrics(&self.ctx, self.timer, self.lock_wait_time, false, None, None);

        DatastoreComputeDuration::from_micros(self.datastore_compute_time_microseconds.into_inner())
    }

    /// Roll back the mutations performed by `self` and convert it into an immutable [`TxId`].
    ///
    /// The returned [`TxId`] will inherit the mutable transaction's datastore compute time.
    /// Care should be taken to charge energy when releasing the [`TxId`].
    pub fn rollback_downgrade(mut self, workload: Workload) -> TxId {
        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_metrics(&self.ctx, self.timer, self.lock_wait_time, false, None, None);
        // Update the workload type of the execution context
        self.ctx.workload = workload.into();

        TxId {
            committed_state_shared_lock: SharedWriteGuard::downgrade(self.committed_state_write_lock),
            lock_wait_time: Duration::ZERO,
            timer: Instant::now(),
            ctx: self.ctx,
            datastore_compute_time_microseconds: self.datastore_compute_time_microseconds,
        }
    }
}

/// Either a row just inserted to a table or a row that already existed in some table.
#[derive(Clone, Copy)]
pub(super) enum RowRefInsertion<'a> {
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

/// The iterator returned by [`MutTx::btree_scan`].
pub struct BTreeScan<'a> {
    inner: BTreeScanInner<'a>,
}

enum BTreeScanInner<'a> {
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

impl<'a> Iterator for BTreeScan<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            BTreeScanInner::Empty(it) => it.next(),
            BTreeScanInner::TxOnly(it) => it.next(),
            BTreeScanInner::CommitOnly(it) => it.next(),
            BTreeScanInner::CommitOnlyWithDeletes(it) => it.next(),
            BTreeScanInner::Both(it) => it.next(),
            BTreeScanInner::BothWithDeletes(it) => it.next(),
        }
    }
}

impl<'a> Iterator for IndexScanFilterDeleted<'a> {
    type Item = RowRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.find(|row| !self.deletes.contains(&row.pointer()))
    }
}

impl MutTxId {
    /// Insert a row into a table.
    ///
    /// Requires:
    /// - `TableId` must refer to a valid table for the database at `database_address`.
    /// - `row` must be a valid row for the table at `table_id`.
    ///
    /// Returns:
    /// - a product value with a projection of the row containing only the generated column values.
    /// - a ref to the inserted row.
    pub(super) fn insert<'a>(
        &'a mut self,
        table_id: TableId,
        row: &mut ProductValue,
    ) -> Result<(AlgebraicValue, RowRefInsertion<'a>)> {
        let generated = self.write_sequence_values(table_id, row)?;
        let row_ref = self.insert_row_internal(table_id, row)?;
        Ok((generated, row_ref))
    }

    /// Generate and write sequence values to `row`
    /// and return a projection of `row` with only the generated column values.
    fn write_sequence_values(&mut self, table_id: TableId, row: &mut ProductValue) -> Result<AlgebraicValue> {
        // TODO: Executing schema_for_table for every row insert is expensive.
        // However we ask for the schema in the [Table] struct instead.
        let schema = self.schema_for_table(table_id)?;

        // Collect all the columns with sequences that need generation.
        let (cols_to_update, seqs_to_use): (ColList, SmallVec<[_; 1]>) = schema
            .sequences
            .iter()
            .filter(|seq| row.elements[seq.col_pos.idx()].is_numeric_zero())
            .map(|seq| (seq.col_pos, seq.sequence_id))
            .unzip();

        // Update every column in the row that needs it.
        // We assume here that column with a sequence is of a sequence-compatible type.
        for (col_id, sequence_id) in cols_to_update.iter().zip(seqs_to_use) {
            let seq_val = self.get_next_sequence_value_internal(sequence_id)?;
            let col_typ = &schema.columns()[col_id.idx()].col_type;
            let gen_val = AlgebraicValue::from_sequence_value(col_typ, seq_val);
            row.elements[col_id.idx()] = gen_val;
        }

        Ok(row.project(&cols_to_update)?)
    }

    pub(super) fn insert_row_internal(&mut self, table_id: TableId, row: &ProductValue) -> Result<RowRefInsertion<'_>> {
        let commit_table = self.committed_state_write_lock.get_table(table_id);

        // Check for constraint violations as early as possible,
        // to ensure that `UniqueConstraintViolation` errors have precedence over other errors.
        // `tx_table.insert` will later perform the same check on the tx table,
        // so this method needs only to check the committed state.
        if let Some(commit_table) = commit_table {
            commit_table
                .check_unique_constraints(row, |maybe_conflict| self.tx_state.is_deleted(table_id, maybe_conflict))
                .map_err(IndexError::from)?;
        }

        // Get the insert table, so we can write the row into it.
        let (tx_table, tx_blob_store, _, delete_table) = self
            .tx_state
            .get_table_and_blob_store_or_maybe_create_from(table_id, commit_table)
            .ok_or(TableError::IdNotFoundState(table_id))?;

        match tx_table.insert(tx_blob_store, row) {
            Ok((hash, row_ref)) => {
                // `row` not previously present in insert tables,
                // but may still be a set-semantic conflict with a row
                // in the committed state.

                let ptr = row_ref.pointer();
                if let Some(commit_table) = commit_table {
                    // Safety:
                    // - `commit_table` and `tx_table` use the same schema
                    //   because `tx_table` is derived from `commit_table`.
                    // - `ptr` and `hash` are correct because we just got them from `tx_table.insert`.
                    if let Some(committed_ptr) = unsafe { Table::find_same_row(commit_table, tx_table, ptr, hash) } {
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
                            .delete(tx_blob_store, ptr, |_| ())
                            .expect("Failed to delete a row we just inserted");

                        // It's possible that `row` appears in the committed state,
                        // but is marked as deleted.
                        // In this case, undelete it, so it remains in the committed state.
                        delete_table.remove(&committed_ptr);

                        // No new row was inserted, but return `committed_ptr`.
                        let blob_store = &self.committed_state_write_lock.blob_store;
                        return Ok(RowRefInsertion::Existed(
                            // SAFETY: `find_same_row` told us that `ptr` refers to a valid row in `commit_table`.
                            unsafe { commit_table.get_row_ref_unchecked(blob_store, committed_ptr) },
                        ));
                    }
                }

                Ok(RowRefInsertion::Inserted(unsafe {
                    // SAFETY: `ptr` came from `tx_table.insert` just now without any interleaving calls.
                    tx_table.get_row_ref_unchecked(tx_blob_store, ptr)
                }))
            }
            // `row` previously present in insert tables; do nothing but return `ptr`.
            Err(InsertError::Duplicate(ptr)) => Ok(RowRefInsertion::Existed(
                // SAFETY: `tx_table` told us that `ptr` refers to a valid row in it.
                unsafe { tx_table.get_row_ref_unchecked(tx_blob_store, ptr) },
            )),

            // Index error: unbox and return `TableError::IndexError`
            // rather than `TableError::Insert(InsertError::IndexError)`.
            Err(InsertError::IndexError(e)) => Err(IndexError::from(e).into()),

            // Misc. insertion error; fail.
            Err(e) => Err(TableError::Insert(e).into()),
        }
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
                    .ok_or_else(|| TableError::IdNotFoundState(table_id))?;
                Ok(table.delete(blob_store, row_pointer, |_| ()).is_some())
            }
            SquashedOffset::COMMITTED_STATE => {
                // NOTE: We trust the `row_pointer` refers to an extant row,
                // and check only that it hasn't yet been deleted.
                let delete_table = self.tx_state.get_delete_table_mut(table_id);

                Ok(delete_table.insert(row_pointer))
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

        // We need `insert_internal_allow_duplicate` rather than `insert` here
        // to bypass unique constraint checks.
        match tx_table.insert_internal_allow_duplicate(tx_blob_store, rel) {
            Err(err @ InsertError::Bflatn(_)) => Err(TableError::Insert(err).into()),
            Err(e) => unreachable!(
                "Table::insert_internal_allow_duplicates returned error of unexpected variant: {:?}",
                e
            ),
            Ok((row_ref, _)) => {
                let hash = row_ref.row_hash();
                let ptr = row_ref.pointer();

                // First, check if a matching row exists in the `tx_table`.
                // If it does, no need to check the `commit_table`.
                //
                // Safety:
                // - `tx_table` trivially uses the same schema as itself.
                // - `ptr` is valid because we just inserted it.
                // - `hash` is correct because we just computed it.
                let to_delete = unsafe { Table::find_same_row(tx_table, tx_table, ptr, hash) }
                    // Not present in insert tables; check if present in the commit tables.
                    .or_else(|| {
                        commit_table.and_then(|commit_table| {
                            // Safety:
                            // - `commit_table` and `tx_table` use the same schema
                            // - `ptr` is valid because we just inserted it.
                            // - `hash` is correct because we just computed it.
                            unsafe { Table::find_same_row(commit_table, tx_table, ptr, hash) }
                        })
                    });

                debug_assert_ne!(to_delete, Some(ptr));

                // Remove the temporary entry from the insert tables.
                // Do this before actually deleting to drop the borrows on the tables.
                // Safety: `ptr` is valid because we just inserted it and haven't deleted it since.
                unsafe {
                    tx_table.delete_internal_skip_pointer_map(tx_blob_store, ptr);
                }

                // Mark the committed row to be deleted by adding it to the delete table.
                to_delete
                    .map(|to_delete| self.delete(table_id, to_delete))
                    .unwrap_or(Ok(false))
            }
        }
    }
}

impl StateView for MutTxId {
    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>> {
        self.while_tracking_compute_time(|this| {
            // TODO(bikeshedding, docs): should this also check if the schema is in the system tables,
            // but the table hasn't been constructed yet?
            // If not, document why.
            this.tx_state
                .insert_tables
                .get(&table_id)
                .or_else(|| this.committed_state_write_lock.tables.get(&table_id))
                .map(|table| table.get_schema())
        })
    }

    fn table_row_count(&self, table_id: TableId) -> Option<u64> {
        self.while_tracking_compute_time(|this| {
            let commit_count = this.committed_state_write_lock.table_row_count(table_id);
            let (tx_ins_count, tx_del_count) = this.tx_state.table_row_count(table_id);
            let commit_count = commit_count.map(|cc| cc - tx_del_count);
            // Keep track of whether `table_id` exists.
            match (commit_count, tx_ins_count) {
                (Some(cc), Some(ic)) => Some(cc + ic),
                (Some(c), None) | (None, Some(c)) => Some(c),
                (None, None) => None,
            }
        })
    }

    fn iter(&self, table_id: TableId) -> Result<Iter<'_>> {
        // TODO(energy): track compute time spent in the iterator.
        // This is challenging to do while maintaining non-reentrancy,
        // and there are concerns about overhead - wrapping the body of `next`
        // in (the equivalent of) `while_tracking_compute_time` means two clock reads for every row,
        // which is potentially too much.

        if let Some(table_name) = self.table_name(table_id) {
            return Ok(Iter::new(
                table_id,
                table_name,
                Some(&self.tx_state),
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
    ) -> Result<IterByColRange<'_, R>> {
        // TODO(energy): track compute time spent in the iterator.
        // This is challenging to do while maintaining non-reentrancy,
        // and there are concerns about overhead - wrapping the body of `next`
        // in (the equivalent of) `while_tracking_compute_time` means two clock reads for every row,
        // which is potentially too much.

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
                table_id,
                tx_state: &self.tx_state,
                inserted_rows,
                committed_rows: self.committed_state_write_lock.index_seek(table_id, &cols, &range),
                num_committed_rows_fetched: 0,
            }))
        } else {
            // Either the current transaction has not modified this table, or the table is not
            // indexed.
            match self.committed_state_write_lock.index_seek(table_id, &cols, &range) {
                Some(committed_rows) => Ok(IterByColRange::CommittedIndex(CommittedIndexIter::new(
                    table_id,
                    Some(&self.tx_state),
                    &self.committed_state_write_lock,
                    committed_rows,
                ))),
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

                    Ok(IterByColRange::Scan(ScanIterByColRange::new(
                        self.iter(table_id)?,
                        cols,
                        range,
                    )))
                }
            }
        }
    }
}
