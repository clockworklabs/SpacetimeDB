use super::{
    datastore::Result,
    delete_table::DeleteTable,
    sequence::{Sequence, SequencesState},
    state_view::StateView,
    tx_state::{IndexIdMap, PendingSchemaChange, TxState},
    IterByColEqTx,
};
use crate::{
    db_metrics::DB_METRICS,
    error::{DatastoreError, IndexError, TableError, ViewError},
    execution_context::ExecutionContext,
    locking_tx_datastore::{
        mut_tx::ViewReadSets,
        state_view::{iter_st_column_for_table, ApplyFilter, EqOnColumn, RangeOnColumn, ScanOrIndex},
        IterByColRangeTx,
    },
    system_tables::{
        is_built_in_meta_row, system_tables, table_id_is_reserved, StColumnRow, StConstraintData, StConstraintRow,
        StFields, StIndexRow, StSequenceFields, StSequenceRow, StTableFields, StTableRow, StViewRow, SystemTable,
        ST_CLIENT_ID, ST_CLIENT_IDX, ST_COLUMN_ID, ST_COLUMN_IDX, ST_COLUMN_NAME, ST_CONSTRAINT_ID, ST_CONSTRAINT_IDX,
        ST_CONSTRAINT_NAME, ST_INDEX_ID, ST_INDEX_IDX, ST_INDEX_NAME, ST_MODULE_ID, ST_MODULE_IDX,
        ST_ROW_LEVEL_SECURITY_ID, ST_ROW_LEVEL_SECURITY_IDX, ST_SCHEDULED_ID, ST_SCHEDULED_IDX, ST_SEQUENCE_ID,
        ST_SEQUENCE_IDX, ST_SEQUENCE_NAME, ST_TABLE_ID, ST_TABLE_IDX, ST_VAR_ID, ST_VAR_IDX, ST_VIEW_ARG_ID,
        ST_VIEW_ARG_IDX,
    },
    traits::{EphemeralTables, TxData},
};
use crate::{
    locking_tx_datastore::ViewCallInfo,
    system_tables::{
        ST_CONNECTION_CREDENTIALS_ID, ST_CONNECTION_CREDENTIALS_IDX, ST_VIEW_COLUMN_ID, ST_VIEW_COLUMN_IDX, ST_VIEW_ID,
        ST_VIEW_IDX, ST_VIEW_PARAM_ID, ST_VIEW_PARAM_IDX, ST_VIEW_SUB_ID, ST_VIEW_SUB_IDX,
    },
};
use anyhow::anyhow;
use core::{convert::Infallible, ops::RangeBounds};
use spacetimedb_data_structures::map::{HashMap, HashSet, IntMap, IntSet};
use spacetimedb_durability::TxOffset;
use spacetimedb_lib::{db::auth::StTableType, Identity};
use spacetimedb_primitives::{ColId, ColList, ColSet, IndexId, SequenceId, TableId, ViewId};
use spacetimedb_sats::{algebraic_value::de::ValueDeserializer, memory_usage::MemoryUsage, Deserialize};
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_schema::{
    def::IndexAlgorithm,
    schema::{ColumnSchema, TableSchema},
};
use spacetimedb_table::{
    blob_store::{BlobStore, HashMapBlobStore},
    indexes::{RowPointer, SquashedOffset},
    page_pool::PagePool,
    table::{IndexScanPointIter, IndexScanRangeIter, InsertError, RowRef, Table, TableAndIndex, TableScanIter},
    table_index::IndexSeekRangeResult,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use thin_vec::ThinVec;

/// Contains the live, in-memory snapshot of a database. This structure
/// is exposed in order to support tools wanting to process the commit
/// logs directly. For normal usage, see the RelationalDB struct instead.
///
/// NOTE: unstable API, this may change at any point in the future.
///
/// Fields whose names are prefixed with `replay_` are used only while replaying a commitlog,
/// and are unused during live transaction processing.
/// TODO(centril): Only used during bootstrap and is otherwise unused.
/// We should split `CommittedState` into two types
/// where one, e.g., `ReplayCommittedState`, has this field.
pub struct CommittedState {
    pub(crate) next_tx_offset: u64,
    pub(crate) tables: IntMap<TableId, Table>,
    pub(crate) blob_store: HashMapBlobStore,
    /// Provides fast lookup for index id -> an index.
    pub(super) index_id_map: IndexIdMap,
    /// The page pool used to retrieve new/unused pages for tables.
    ///
    /// Between transactions, this is untouched.
    /// During transactions, the [`MutTxId`] can steal pages from the committed state.
    ///
    /// This is a handle on a shared structure.
    /// Pages are shared between all modules running on a particular host,
    /// not allocated per-module.
    pub(super) page_pool: PagePool,
    /// We track the read sets for each view in the committed state.
    /// We check each reducer's write set against these read sets.
    /// Any overlap will trigger a re-evaluation of the affected view,
    /// and its read set will be updated accordingly.
    read_sets: ViewReadSets,

    /// Tables which do not need to be made persistent.
    /// These include:
    ///     - system tables: `st_view_sub`, `st_view_arg`
    ///     - Tables which back views.
    pub(super) ephemeral_tables: EphemeralTables,

    /// Whether the table was dropped within the current transaction during replay.
    ///
    /// While processing a transaction which drops a table, we'll first see the `st_table` delete,
    /// then a series of deletes from the table itself.
    /// We track the table's ID here so we know to ignore the deletes.
    ///
    /// Cleared after the end of processing each transaction,
    /// as it should be impossible to ever see another reference to the table after that point.
    replay_table_dropped: IntSet<TableId>,

    /// Rows within `st_column` which should be ignored during replay
    /// due to having been superseded by a new row representing the same column.
    ///
    /// During replay, we visit all of the inserts table-by-table, followed by all of the deletes table-by-table.
    /// This means that, when multiple columns of a table change type within the same transaction,
    /// we see all of the newly-inserted `st_column` rows first, and then later, all of the deleted rows.
    /// We may even see inserts into the altered table before seeing the `st_column` deletes!
    ///
    /// In order to maintain a proper view of the schema of tables during replay,
    /// we must remember the old versions of the `st_column` rows when we insert the new ones,
    /// so that we can respect only the new versions.
    ///
    /// We insert into this set during [`Self::replay_insert`] of `st_column` rows
    /// and delete from it during [`Self::replay_delete`] of `st_column` rows.
    /// We assert this is empty at the end of each transaction.
    replay_columns_to_ignore: HashSet<RowPointer>,

    /// Set of tables whose `st_table` entries have been updated during the currently-replaying transaction,
    /// mapped to the current most-recent `st_table` row.
    ///
    /// When processing an insert to `st_table`, if the table already exists, we'll record it here.
    /// Then, when we see a corresponding delete, we know that the table has not been dropped,
    /// and so we won't delete the in-memory structure or insert its ID into [`Self::replay_table_dropped`].
    ///
    /// When looking up the `st_table` row for a table, if it has an entry here,
    /// that means there are two rows resident in `st_table` at this point in replay.
    /// We return the row recorded here rather than inspecting `st_table`.
    ///
    /// We remove from this set when we reach the matching delete,
    /// and assert this set is empty at the end of each transaction.
    ///
    /// [`RowPointer`]s from this set are passed to the `unsafe` [`Table::get_row_ref_unchecked`],
    /// so it's important to properly maintain only [`RowPointer`]s to valid, extant, non-deleted rows.
    replay_table_updated: IntMap<TableId, RowPointer>,
}

impl CommittedState {
    /// Returns the views that perform a full scan of this table
    pub(super) fn views_for_table_scan(&self, table_id: &TableId) -> impl Iterator<Item = &ViewCallInfo> {
        self.read_sets.views_for_table_scan(table_id)
    }

    /// Returns the views that perform an precise index seek on given `row_ref` of `table_id`
    pub fn views_for_index_seek<'a>(
        &'a self,
        table_id: &TableId,
        row_ref: RowRef<'a>,
    ) -> impl Iterator<Item = &'a ViewCallInfo> {
        self.read_sets.views_for_index_seek(table_id, row_ref)
    }
}

impl MemoryUsage for CommittedState {
    fn heap_usage(&self) -> usize {
        let Self {
            next_tx_offset,
            tables,
            blob_store,
            index_id_map,
            page_pool: _,
            read_sets,
            ephemeral_tables,
            replay_table_dropped,
            replay_columns_to_ignore,
            replay_table_updated,
        } = self;
        // NOTE(centril): We do not want to include the heap usage of `page_pool` as it's a shared resource.
        next_tx_offset.heap_usage()
            + tables.heap_usage()
            + blob_store.heap_usage()
            + index_id_map.heap_usage()
            + read_sets.heap_usage()
            + ephemeral_tables.heap_usage()
            + replay_columns_to_ignore.heap_usage()
            + replay_table_dropped.heap_usage()
            + replay_table_updated.heap_usage()
    }
}

impl StateView for CommittedState {
    type Iter<'a> = TableScanIter<'a>;
    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>> = IterByColRangeTx<'a, R>;
    type IterByColEq<'a, 'r>
        = IterByColEqTx<'a, 'r>
    where
        Self: 'a;

    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>> {
        self.tables.get(&table_id).map(|table| table.get_schema())
    }

    fn table_row_count(&self, table_id: TableId) -> Option<u64> {
        self.get_table(table_id).map(|table| table.row_count)
    }

    fn iter(&self, table_id: TableId) -> Result<Self::Iter<'_>> {
        self.table_scan(table_id)
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.0).into())
    }

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<R: RangeBounds<AlgebraicValue>>(
        &self,
        table_id: TableId,
        cols: ColList,
        range: R,
    ) -> Result<Self::IterByColRange<'_, R>> {
        match self.index_seek_range(table_id, &cols, &range) {
            Some(Ok(iter)) => Ok(ScanOrIndex::Index(iter)),
            None | Some(Err(_)) => Ok(ScanOrIndex::Scan(ApplyFilter::new(
                RangeOnColumn { cols, range },
                self.iter(table_id)?,
            ))),
        }
    }

    fn iter_by_col_eq<'a, 'r>(
        &'a self,
        table_id: TableId,
        cols: impl Into<ColList>,
        val: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>> {
        let cols = cols.into();
        match self.index_seek_point(table_id, &cols, val) {
            Some(iter) => Ok(ScanOrIndex::Index(iter)),
            None => Ok(ScanOrIndex::Scan(ApplyFilter::new(
                EqOnColumn { cols, val },
                self.iter(table_id)?,
            ))),
        }
    }

    /// Find the `st_table` row for `table_id`, first inspecting [`Self::replay_table_updated`],
    /// then falling back to [`Self::iter_by_col_eq`] of `st_table`.
    fn find_st_table_row(&self, table_id: TableId) -> Result<StTableRow> {
        let row_ref = if let Some(row_ptr) = self.replay_table_updated.get(&table_id) {
            let (table, blob_store, _) = self.get_table_and_blob_store(table_id)?;
            // Safety: `row_ptr` is stored in `self.replay_table_updated`,
            // meaning it was inserted into `st_table` by `replay_insert`
            // and has not yet been deleted by `replay_delete_by_rel`.
            unsafe { table.get_row_ref_unchecked(blob_store, *row_ptr) }
        } else {
            self.iter_by_col_eq(ST_TABLE_ID, StTableFields::TableId, &table_id.into())?
                .next()
                .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?
        };

        StTableRow::try_from(row_ref)
    }
}

impl CommittedState {
    pub(super) fn new(page_pool: PagePool) -> Self {
        Self {
            next_tx_offset: <_>::default(),
            tables: <_>::default(),
            blob_store: <_>::default(),
            index_id_map: <_>::default(),
            read_sets: <_>::default(),
            page_pool,
            ephemeral_tables: <_>::default(),
            replay_table_dropped: <_>::default(),
            replay_columns_to_ignore: <_>::default(),
            replay_table_updated: <_>::default(),
        }
    }

    /// Delete all but the highest-allocation `st_sequence` row for each system sequence.
    ///
    /// Prior versions of `RelationalDb::migrate_system_tables` (defined in the `core` crate)
    /// initialized newly-created system sequences to `allocation: 4097`,
    /// while `committed_state::bootstrap_system_tables` sets `allocation: 4096`.
    /// This affected the system table migration which added
    /// `st_view_view_id_seq` and `st_view_arg_id_seq`.
    /// As a result, when replaying these databases' commitlogs without a snapshot,
    /// we will end up with two rows in `st_sequence` for each of these sequences,
    /// resulting in a unique constraint violation in `CommittedState::build_indexes`.
    /// We call this method in [`super::datastore::Locking::rebuild_state_after_replay`]
    /// to avoid that unique constraint violation.
    pub(super) fn fixup_delete_duplicate_system_sequence_rows(&mut self) {
        struct StSequenceRowInfo {
            sequence_id: SequenceId,
            allocated: i128,
            row_pointer: RowPointer,
        }

        // Get all the `st_sequence` rows which refer to sequences on system tables,
        // including any duplicates caused by the bug described above.
        let sequence_rows = self
            .table_scan(ST_SEQUENCE_ID)
            .expect("`st_sequence` should exist")
            .filter_map(|row_ref| {
                // Read the table ID to which the sequence refers,
                // in order to determine if this is a system sequence or not.
                let table_id = row_ref
                    .read_col::<TableId>(StSequenceFields::TableId)
                    .expect("`st_sequence` row should conform to `st_sequence` schema");

                // If this sequence refers to a system table, it may need a fixup.
                // User tables' sequences will never need fixups.
                table_id_is_reserved(table_id).then(|| {
                    let allocated = row_ref
                        .read_col::<i128>(StSequenceFields::Allocated)
                        .expect("`st_sequence` row should conform to `st_sequence` schema");
                    let sequence_id = row_ref
                        .read_col::<SequenceId>(StSequenceFields::SequenceId)
                        .expect("`st_sequence` row should conform to `st_sequence` schema");
                    StSequenceRowInfo {
                        allocated,
                        sequence_id,
                        row_pointer: row_ref.pointer(),
                    }
                })
            })
            .collect::<Vec<_>>();

        let (st_sequence, blob_store, ..) = self
            .get_table_and_blob_store_mut(ST_SEQUENCE_ID)
            .expect("`st_sequence` should exist");

        // Track the row with the highest allocation for each sequence.
        let mut highest_allocations: HashMap<SequenceId, (i128, RowPointer)> = HashMap::default();

        for StSequenceRowInfo {
            sequence_id,
            allocated,
            row_pointer,
        } in sequence_rows
        {
            // For each `st_sequence` row which refers to a system table,
            // if we've already seen a row for the same sequence,
            // keep only the row with the higher allocation.
            if let Some((prev_allocated, prev_row_pointer)) =
                highest_allocations.insert(sequence_id, (allocated, row_pointer))
            {
                // We have a duplicate row. We want to keep whichever has the higher `allocated`,
                // and delete the other.
                let row_pointer_to_delete = if prev_allocated > allocated {
                    // The previous row has a higher allocation than the new row,
                    // so delete the new row and restore `previous` to `highest_allocations`.
                    highest_allocations.insert(sequence_id, (prev_allocated, prev_row_pointer));
                    row_pointer
                } else {
                    // The previous row does not have a higher allocation than the new,
                    // so delete the previous row and keep the new one.
                    prev_row_pointer
                };

                st_sequence.delete(blob_store, row_pointer_to_delete, |_| ())
                    .expect("Duplicated `st_sequence` row at `row_pointer_to_delete` should be present in `st_sequence` during fixup");
            }
        }
    }

    /// Extremely delicate function to bootstrap the system tables.
    /// Don't update this unless you know what you're doing.
    pub(super) fn bootstrap_system_tables(&mut self, database_identity: Identity) -> Result<()> {
        // NOTE: the `rdb_num_table_rows` metric is used by the query optimizer,
        // and therefore has performance implications and must not be disabled.
        let with_label_values = |table_id: TableId, table_name: &str| {
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&database_identity, &table_id.0, table_name)
        };

        let schemas = system_tables().map(Arc::new);
        let ref_schemas = schemas.each_ref().map(|s| &**s);

        // Insert the table row into st_tables, creating st_tables if it's missing.
        let (st_tables, blob_store, pool) =
            self.get_table_and_blob_store_or_create(ST_TABLE_ID, &schemas[ST_TABLE_IDX]);
        // Insert the table row into `st_tables` for all system tables
        for schema in ref_schemas {
            let table_id = schema.table_id;
            // Metric for this system table.
            with_label_values(table_id, &schema.table_name).set(0);

            let row = StTableRow {
                table_id,
                table_name: schema.table_name.clone(),
                table_type: StTableType::System,
                table_access: schema.table_access,
                table_primary_key: schema.primary_key.map(Into::into),
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_TABLES.
            st_tables.insert(pool, blob_store, &row)?;
        }

        // Insert the columns into `st_columns`
        let (st_columns, blob_store, pool) =
            self.get_table_and_blob_store_or_create(ST_COLUMN_ID, &schemas[ST_COLUMN_IDX]);
        for col in ref_schemas.iter().flat_map(|x| x.columns()).cloned() {
            let row = StColumnRow {
                table_id: col.table_id,
                col_pos: col.col_pos,
                col_name: col.col_name,
                col_type: col.col_type.into(),
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_COLUMNS.
            st_columns.insert(pool, blob_store, &row)?;
            // Increment row count for st_columns.
            with_label_values(ST_COLUMN_ID, ST_COLUMN_NAME).inc();
        }

        // Insert the FK sorted by table/column so it show together when queried.

        // Insert constraints into `st_constraints`
        let (st_constraints, blob_store, pool) =
            self.get_table_and_blob_store_or_create(ST_CONSTRAINT_ID, &schemas[ST_CONSTRAINT_IDX]);
        for constraint in ref_schemas.iter().flat_map(|x| &x.constraints) {
            let row = StConstraintRow {
                constraint_id: constraint.constraint_id,
                constraint_name: constraint.constraint_name.clone(),
                table_id: constraint.table_id,
                constraint_data: constraint.data.clone().into(),
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_CONSTRAINTS.
            st_constraints.insert(pool, blob_store, &row)?;
            // Increment row count for st_constraints.
            with_label_values(ST_CONSTRAINT_ID, ST_CONSTRAINT_NAME).inc();
        }

        // Insert the indexes into `st_indexes`
        let (st_indexes, blob_store, pool) =
            self.get_table_and_blob_store_or_create(ST_INDEX_ID, &schemas[ST_INDEX_IDX]);

        for index in ref_schemas.iter().flat_map(|x| &x.indexes) {
            let row: StIndexRow = index.clone().into();
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_INDEXES.
            st_indexes.insert(pool, blob_store, &row)?;
            // Increment row count for st_indexes.
            with_label_values(ST_INDEX_ID, ST_INDEX_NAME).inc();
        }

        // We don't add the row here but with `MutProgrammable::set_program_hash`, but we need to register the table
        // in the internal state.
        self.create_table(ST_MODULE_ID, schemas[ST_MODULE_IDX].clone());

        self.create_table(ST_CLIENT_ID, schemas[ST_CLIENT_IDX].clone());

        self.create_table(ST_VAR_ID, schemas[ST_VAR_IDX].clone());

        self.create_table(ST_SCHEDULED_ID, schemas[ST_SCHEDULED_IDX].clone());

        self.create_table(ST_ROW_LEVEL_SECURITY_ID, schemas[ST_ROW_LEVEL_SECURITY_IDX].clone());
        self.create_table(
            ST_CONNECTION_CREDENTIALS_ID,
            schemas[ST_CONNECTION_CREDENTIALS_IDX].clone(),
        );

        self.create_table(ST_VIEW_ID, schemas[ST_VIEW_IDX].clone());
        self.create_table(ST_VIEW_PARAM_ID, schemas[ST_VIEW_PARAM_IDX].clone());
        self.create_table(ST_VIEW_COLUMN_ID, schemas[ST_VIEW_COLUMN_IDX].clone());
        self.create_table(ST_VIEW_SUB_ID, schemas[ST_VIEW_SUB_IDX].clone());
        self.create_table(ST_VIEW_ARG_ID, schemas[ST_VIEW_ARG_IDX].clone());

        // Insert the sequences into `st_sequences`
        let (st_sequences, blob_store, pool) =
            self.get_table_and_blob_store_or_create(ST_SEQUENCE_ID, &schemas[ST_SEQUENCE_IDX]);
        for seq in ref_schemas.iter().flat_map(|x| &x.sequences) {
            let row = StSequenceRow {
                sequence_id: seq.sequence_id,
                sequence_name: seq.sequence_name.clone(),
                table_id: seq.table_id,
                col_pos: seq.col_pos,
                increment: seq.increment,
                min_value: seq.min_value,
                max_value: seq.max_value,
                start: seq.start,
                // In practice, this means we will actually start at start - 1, since `allocated`
                // overrides start, but we keep these fields set this way to match databases
                // that were bootstrapped before 1.4 and don't have a snapshot.
                // This is covered with the test:
                //   db::relational_db::tests::load_1_2_quickstart_without_snapshot_test
                allocated: seq.start - 1,
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_SEQUENCES.
            st_sequences.insert(pool, blob_store, &row)?;
            // Increment row count for st_sequences
            with_label_values(ST_SEQUENCE_ID, ST_SEQUENCE_NAME).inc();
        }

        // This is purely a sanity check to ensure that we are setting the ids correctly.
        self.assert_system_table_schemas_match()?;
        Ok(())
    }

    pub(super) fn assert_system_table_schemas_match(&self) -> Result<()> {
        for schema in system_tables() {
            let table_id = schema.table_id;
            let in_memory = self
                .tables
                .get(&table_id)
                .ok_or(TableError::IdNotFoundState(table_id))?
                .schema
                .clone();
            let mut in_st_tables = self.schema_for_table_raw(table_id)?;
            // We normalize so that the orders will match when checking for equality.
            in_st_tables.normalize();
            let in_memory = {
                let mut s = in_memory.as_ref().clone();
                s.normalize();
                s
            };

            if in_memory != in_st_tables {
                return Err(anyhow!(
                    "System table schema mismatch for table id {table_id}. Expected: {schema:?}, found: {in_memory:?}"
                )
                .into());
            }
        }

        Ok(())
    }

    pub(super) fn replay_truncate(&mut self, table_id: TableId) -> Result<()> {
        // (1) Table dropped? Avoid an error and just ignore the row instead.
        if self.replay_table_dropped.contains(&table_id) {
            return Ok(());
        }

        // Get the table for mutation.
        let (table, blob_store, ..) = self.get_table_and_blob_store_mut(table_id)?;

        // We do not need to consider a truncation of `st_table` itself,
        // as if that happens, the database is bricked.

        table.clear(blob_store);

        Ok(())
    }

    pub(super) fn replay_delete_by_rel(&mut self, table_id: TableId, row: &ProductValue) -> Result<()> {
        // (1) Table dropped? Avoid an error and just ignore the row instead.
        if self.replay_table_dropped.contains(&table_id) {
            return Ok(());
        }

        // Get the table for mutation.
        let (table, blob_store, _, page_pool) = self.get_table_and_blob_store_mut(table_id)?;

        // Delete the row.
        let row_ptr = table
            .delete_equal_row(page_pool, blob_store, row)
            .map_err(TableError::Bflatn)?
            .ok_or_else(|| anyhow!("Delete for non-existent row when replaying transaction"))?;

        if table_id == ST_TABLE_ID {
            let referenced_table_id = row
                .elements
                .get(StTableFields::TableId.col_idx())
                .expect("`st_table` row should conform to `st_table` schema")
                .as_u32()
                .expect("`st_table` row should conform to `st_table` schema");
            if self
                .replay_table_updated
                .remove(&TableId::from(*referenced_table_id))
                .is_some()
            {
                // This delete is part of an update to an `st_table` row,
                // i.e. earlier in this transaction we inserted a new version of the row.
                // That means it's not a dropped table.
            } else {
                // A row was removed from `st_table`, so a table was dropped.
                // Remove that table from the in-memory structures.
                let dropped_table_id = Self::read_table_id(row);
                // It's safe to ignore the case where we don't have an in-memory structure for the deleted table.
                // This can happen if a table is initially empty at the snapshot or its creation,
                // and never has any rows inserted into or deleted from it.
                self.tables.remove(&dropped_table_id);

                // Mark the table as dropped so that when
                // processing row deletions for that table later,
                // they are simply ignored in (1).
                self.replay_table_dropped.insert(dropped_table_id);
            }
        }

        if table_id == ST_COLUMN_ID {
            // We may have reached the corresponding delete to an insert in `st_column`
            // as the result of a column-type-altering migration.
            // Now that the outdated `st_column` row isn't present any more,
            // we can stop ignoring it.
            //
            // It's also possible that we're deleting this column as the result of a deleted table,
            // and that there wasn't any corresponding insert at all.
            // If that's the case, `row_ptr` won't be in `self.replay_columns_to_ignore`,
            // which is fine.
            self.replay_columns_to_ignore.remove(&row_ptr);
        }

        Ok(())
    }

    pub(super) fn replay_insert(
        &mut self,
        table_id: TableId,
        schema: &Arc<TableSchema>,
        row: &ProductValue,
    ) -> Result<()> {
        let (table, blob_store, pool) = self.get_table_and_blob_store_or_create(table_id, schema);

        let (_, row_ref) = match table.insert(pool, blob_store, row) {
            Ok(stuff) => stuff,
            Err(InsertError::Duplicate(e)) => {
                if is_built_in_meta_row(table_id, row)? {
                    // If this is a meta-descriptor for a system object,
                    // and it already exists exactly, then we can safely ignore the insert.
                    // Any error other than `Duplicate` means the commitlog
                    // has system table schemas which do not match our expectations,
                    // which is almost certainly an unrecoverable error.
                    return Ok(());
                } else {
                    return Err(TableError::Duplicate(e).into());
                }
            }
            Err(InsertError::Bflatn(e)) => return Err(TableError::Bflatn(e).into()),
            Err(InsertError::IndexError(e)) => return Err(IndexError::UniqueConstraintViolation(e).into()),
        };

        // `row_ref` is treated as having a mutable borrow on `self`
        // because it derives from `self.get_table_and_blob_store_or_create`,
        // so we have to downgrade it to a pointer and then re-upgrade it again as an immutable row pointer later.
        let row_ptr = row_ref.pointer();

        if table_id == ST_TABLE_ID {
            // For `st_table` inserts, we need to check if this is a new table or an update to an existing table.
            // For new tables there's nothing more to do, as we'll automatically create it later on
            // when we first `get_table_and_blob_store_or_create` on that table,
            // but for updates to existing tables we need additional bookkeeping.

            // Upgrade `row_ptr` back again, to break the mutable borrow.
            let (table, blob_store, _) = self.get_table_and_blob_store(ST_TABLE_ID)?;

            // Safety: We got `row_ptr` from a valid `RowRef` just above, and haven't done any mutations since,
            // so it must still be valid.
            let row_ref = unsafe { table.get_row_ref_unchecked(blob_store, row_ptr) };

            if self.replay_does_table_already_exist(row_ref) {
                // We've inserted a new `st_table` row for an existing table.
                // We'll expect to see the previous row deleted later in this transaction.
                // For now, mark the table as updated so that we don't confuse it for a deleted table in `replay_delete_by_rel`.

                let st_table_row = StTableRow::try_from(row_ref)?;
                let referenced_table_id = st_table_row.table_id;
                self.replay_table_updated.insert(referenced_table_id, row_ptr);
                self.reschema_table_for_st_table_update(st_table_row)?;
            }
        }

        if table_id == ST_COLUMN_ID {
            // We've made a modification to `st_column`.
            // The type of a table has changed, so figure out which.
            // The first column in `StColumnRow` is `table_id`.
            let referenced_table_id = self.ignore_previous_versions_of_column(row, row_ptr)?;
            self.st_column_changed(referenced_table_id)?;
        }

        Ok(())
    }

    /// Does another row other than `new_st_table_entry` exist in `st_table`
    /// which refers to the same [`TableId`] as `new_st_table_entry`?
    ///
    /// Used during [`Self::replay_insert`] of `st_table` rows to maintain [`Self::replay_table_updated`].
    fn replay_does_table_already_exist(&self, new_st_table_entry: RowRef<'_>) -> bool {
        fn get_table_id(row_ref: RowRef<'_>) -> TableId {
            row_ref
                .read_col(StTableFields::TableId)
                .expect("`st_table` row should conform to `st_table` schema")
        }

        let referenced_table_id = get_table_id(new_st_table_entry);
        self.iter_by_col_eq(ST_TABLE_ID, StTableFields::TableId, &referenced_table_id.into())
            .expect("`st_table` should exist")
            .any(|row_ref| row_ref.pointer() != new_st_table_entry.pointer())
    }

    /// Update the in-memory table structure for the table described by `row`,
    /// in response to replay of a schema-altering migration.
    fn reschema_table_for_st_table_update(&mut self, row: StTableRow) -> Result<()> {
        // We only need to update if we've already constructed the in-memory table structure.
        // If we haven't yet, then `self.get_table_and_blob_store_or_create` will see the correct schema
        // when it eventually runs.
        if let Ok((table, ..)) = self.get_table_and_blob_store_mut(row.table_id) {
            table.with_mut_schema(|schema| -> Result<()> {
                schema.table_access = row.table_access;
                schema.primary_key = row.table_primary_key.map(|col_list| col_list.as_singleton().ok_or_else(|| anyhow::anyhow!("When replaying `st_column` update: `table_primary_key` should be a single column, but found {col_list:?}"))).transpose()?;
                schema.table_name = row.table_name;
                if row.table_type == schema.table_type {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                    "When replaying `st_column` update: `table_type` should not have changed, but previous schema has {:?} and new schema has {:?}",
                    schema.table_type,
                    row.table_type,
                ).into())
}
            })?;
        }
        Ok(())
    }

    /// Mark all `st_column` rows which refer to the same column as `st_column_row`
    /// other than the one at `row_pointer` as outdated
    /// by storing them in [`Self::replay_columns_to_ignore`].
    ///
    /// Returns the ID of the table to which `st_column_row` belongs.
    fn ignore_previous_versions_of_column(
        &mut self,
        st_column_row: &ProductValue,
        row_ptr: RowPointer,
    ) -> Result<TableId> {
        let target_table_id = Self::read_table_id(st_column_row);
        let target_col_id = ColId::deserialize(ValueDeserializer::from_ref(&st_column_row.elements[1]))
            .expect("second field in `st_column` should decode to a `ColId`");

        let outdated_st_column_rows = iter_st_column_for_table(self, &target_table_id.into())?
            .filter_map(|row_ref| {
                StColumnRow::try_from(row_ref)
                    .map(|c| (c.col_pos == target_col_id && row_ref.pointer() != row_ptr).then(|| row_ref.pointer()))
                    .transpose()
            })
            .collect::<Result<Vec<RowPointer>>>()?;

        for row in outdated_st_column_rows {
            self.replay_columns_to_ignore.insert(row);
        }

        Ok(target_table_id)
    }

    /// Refreshes the columns and layout of a table
    /// when a `row` has been inserted from `st_column`.
    ///
    /// The `row_ptr` is a pointer to `row`.
    fn st_column_changed(&mut self, table_id: TableId) -> Result<()> {
        // We're replaying and we don't have unique constraints yet.
        // Due to replay handling all inserts first and deletes after,
        // when processing `st_column` insert/deletes,
        // we may end up with two definitions for the same `col_pos`.
        // Of those two, we're interested in the one we just inserted
        // and not the other one, as it is being replaced.
        // `Self::ignore_previous_version_of_column` has marked the old version as ignored,
        // so filter only the non-ignored columns.
        let mut columns = iter_st_column_for_table(self, &table_id.into())?
            .filter(|row_ref| !self.replay_columns_to_ignore.contains(&row_ref.pointer()))
            .map(|row_ref| StColumnRow::try_from(row_ref).map(Into::into))
            .collect::<Result<Vec<_>>>()?;

        // Columns in `st_column` are not in general sorted by their `col_pos`,
        // though they will happen to be for tables which have never undergone migrations
        // because their initial insertion order matches their `col_pos` order.
        columns.sort_by_key(|col: &ColumnSchema| col.col_pos);

        // Update the columns and layout of the the in-memory table.
        if let Some(table) = self.tables.get_mut(&table_id) {
            table.change_columns_to(columns).map_err(TableError::from)?;
        }

        Ok(())
    }

    pub(super) fn replay_end_tx(&mut self) -> Result<()> {
        self.next_tx_offset += 1;

        if !self.replay_columns_to_ignore.is_empty() {
            return Err(anyhow::anyhow!(
                "`CommittedState::replay_columns_to_ignore` should be empty at the end of a commit, but found {} entries",
                self.replay_columns_to_ignore.len(),
            ).into());
        }

        if !self.replay_table_updated.is_empty() {
            return Err(anyhow::anyhow!(
                "`CommittedState::replay_table_updated` should be empty at the end of a commit, but found {} entries",
                self.replay_table_updated.len(),
            )
            .into());
        }

        // Any dropped tables should be fully gone by the end of a transaction;
        // if we see any reference to them in the future we should error, not ignore.
        self.replay_table_dropped.clear();

        Ok(())
    }

    /// Assuming that a `TableId` is stored as the first field in `row`, read it.
    fn read_table_id(row: &ProductValue) -> TableId {
        TableId::deserialize(ValueDeserializer::from_ref(&row.elements[0]))
            .expect("first field in `st_column` should decode to a `TableId`")
    }

    /// Builds the in-memory state of sequences from `st_sequence` system table.
    /// The tables store the lasted allocated value, which tells us where to start generating.
    pub(super) fn build_sequence_state(&mut self) -> Result<SequencesState> {
        let mut sequence_state = SequencesState::default();
        let st_sequences = self.tables.get(&ST_SEQUENCE_ID).unwrap();
        for row_ref in st_sequences.scan_rows(&self.blob_store) {
            let sequence = StSequenceRow::try_from(row_ref)?;
            let seq = Sequence::new(sequence.clone().into(), Some(sequence.allocated));

            // Clobber any existing in-memory `Sequence`.
            // Such a value may exist because, when replaying without a snapshot,
            // `build_sequence_state` is called twice:
            // once when bootstrapping the empty datastore,
            // and then again after replaying the commitlog.
            // At this latter time, `sequence_state.get(seq.id())` for the system table sequences
            // will return a sequence with incorrect `allocated`,
            // as it will reflect the state after initializing the system tables,
            // but before creating any user tables.
            // The `sequence` we read out of `row_ref` above, and used to construct `seq`,
            // will correctly reflect the state after creating user tables.
            sequence_state.insert(seq);
        }
        Ok(sequence_state)
    }

    pub(super) fn build_indexes(&mut self) -> Result<()> {
        let st_indexes = self.tables.get(&ST_INDEX_ID).unwrap();
        let rows = st_indexes
            .scan_rows(&self.blob_store)
            .map(StIndexRow::try_from)
            .collect::<Result<Vec<_>>>()?;

        let st_constraints = self.tables.get(&ST_CONSTRAINT_ID).unwrap();
        let unique_constraints: HashSet<(TableId, ColSet)> = st_constraints
            .scan_rows(&self.blob_store)
            .map(StConstraintRow::try_from)
            .filter_map(Result::ok)
            .filter_map(|constraint| match constraint.constraint_data {
                StConstraintData::Unique { columns } => Some((constraint.table_id, columns)),
                _ => None,
            })
            .collect();

        for index_row in rows {
            let index_id = index_row.index_id;
            let table_id = index_row.table_id;
            let (table, blob_store, index_id_map, _) = self
                .get_table_and_blob_store_mut(table_id)
                .expect("index should exist in committed state; cannot create it");
            let algo: IndexAlgorithm = index_row.index_algorithm.into();
            let columns: ColSet = algo.columns().into();
            let is_unique = unique_constraints.contains(&(table_id, columns));

            let index = table.new_index(&algo, is_unique)?;
            // SAFETY: `index` was derived from `table`.
            unsafe { table.insert_index(blob_store, index_id, index) };
            index_id_map.insert(index_id, table_id);
        }
        Ok(())
    }

    pub(super) fn collect_ephemeral_tables(&mut self) -> Result<()> {
        self.ephemeral_tables = self.ephemeral_tables()?.into_iter().collect();
        Ok(())
    }

    fn ephemeral_tables(&self) -> Result<Vec<TableId>> {
        let mut tables = vec![ST_VIEW_SUB_ID, ST_VIEW_ARG_ID];

        let Some(st_view) = self.tables.get(&ST_VIEW_ID) else {
            return Ok(tables);
        };
        let backing_tables = st_view
            .scan_rows(&self.blob_store)
            .map(|row_ref| {
                let view_row = StViewRow::try_from(row_ref)?;
                view_row
                    .table_id
                    .ok_or_else(|| DatastoreError::View(ViewError::TableNotFound(view_row.view_id)))
            })
            .collect::<Result<Vec<_>>>()?;

        tables.extend(backing_tables);

        Ok(tables)
    }

    /// After replaying all old transactions,
    /// inserts and deletes into the system tables
    /// might not be reflected in the schemas of the built tables.
    /// So we must re-schema every built table.
    pub(super) fn reschema_tables(&mut self) -> Result<()> {
        // For already built tables, we need to reschema them to account for constraints et al.
        let mut schemas = Vec::with_capacity(self.tables.len());
        for table_id in self.tables.keys().copied() {
            schemas.push(self.schema_for_table_raw(table_id)?);
        }
        for (table, schema) in self.tables.values_mut().zip(schemas) {
            table.with_mut_schema(|s| *s = schema);
        }
        Ok(())
    }

    /// After replaying all old transactions, tables which have rows will
    /// have been created in memory, but tables with no rows will not have
    /// been created. This function ensures that they are created.
    pub(super) fn build_missing_tables(&mut self) -> Result<()> {
        // Find all ids of tables that are in `st_tables` but haven't been built.
        let table_ids = self
            .get_table(ST_TABLE_ID)
            .unwrap()
            .scan_rows(&self.blob_store)
            .map(|r| r.read_col(StTableFields::TableId).unwrap())
            .filter(|table_id| self.get_table(*table_id).is_none())
            .collect::<Vec<_>>();

        // Construct their schemas and insert tables for them.
        for table_id in table_ids {
            let schema = self.schema_for_table(table_id)?;
            self.create_table(table_id, schema);
        }
        Ok(())
    }

    /// Returns an iterator doing a full table scan on `table_id`.
    pub(super) fn table_scan<'a>(&'a self, table_id: TableId) -> Option<TableScanIter<'a>> {
        Some(self.get_table(table_id)?.scan_rows(&self.blob_store))
    }

    /// When there's an index on `cols`,
    /// returns an iterator over the [TableIndex] that yields all the [`RowRef`]s
    /// that match the specified `range` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    ///
    /// For a unique index this will always yield at most one `RowRef`
    /// when `range` is a point.
    /// When there is no index this returns `None`.
    pub(super) fn index_seek_range<'a>(
        &'a self,
        table_id: TableId,
        cols: &ColList,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<IndexSeekRangeResult<IndexScanRangeIter<'a>>> {
        self.tables
            .get(&table_id)?
            .get_index_by_cols_with_table(&self.blob_store, cols)
            .map(|i| i.seek_range(range))
    }

    /// When there's an index on `cols`,
    /// returns an iterator over the [TableIndex] that yields all the [`RowRef`]s
    /// that equal `value` in the indexed column.
    ///
    /// Matching is defined by `Eq for AlgebraicValue`.
    ///
    /// For a unique index this will always yield at most one `RowRef`.
    /// When there is no index this returns `None`.
    pub(super) fn index_seek_point<'a>(
        &'a self,
        table_id: TableId,
        cols: &ColList,
        value: &AlgebraicValue,
    ) -> Option<IndexScanPointIter<'a>> {
        self.tables
            .get(&table_id)?
            .get_index_by_cols_with_table(&self.blob_store, cols)
            .map(|i| i.seek_point(value))
    }

    /// Returns the table associated with the given `index_id`, if any.
    pub(super) fn get_table_for_index(&self, index_id: IndexId) -> Option<TableId> {
        self.index_id_map.get(&index_id).copied()
    }

    /// Returns the table for `table_id` combined with the index for `index_id`, if both exist.
    pub(super) fn get_index_by_id_with_table(&self, table_id: TableId, index_id: IndexId) -> Option<TableAndIndex<'_>> {
        self.tables
            .get(&table_id)?
            .get_index_by_id_with_table(&self.blob_store, index_id)
    }

    // TODO(perf, deep-integration): Make this method `unsafe`. Add the following to the docs:
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
    // within the current transaction (i.e. without an intervening call to self.merge)
    // is sufficient to demonstrate that a call to `self.get` is safe.
    pub(super) fn get(&self, table_id: TableId, row_ptr: RowPointer) -> RowRef<'_> {
        debug_assert!(
            row_ptr.squashed_offset().is_committed_state(),
            "Cannot get TX_STATE RowPointer from CommittedState.",
        );
        let table = self
            .get_table(table_id)
            .expect("Attempt to get COMMITTED_STATE row from table not present in tables.");
        // TODO(perf, deep-integration): Use `get_row_ref_unchecked`.
        table.get_row_ref(&self.blob_store, row_ptr).unwrap()
    }

    /// True if the transaction `(tx_data, ctx)` will be written to the commitlog,
    /// and therefore consumes a value from `self.next_tx_offset`.
    ///
    /// A TX is written to the logs if any of the following holds:
    /// - The TX inserted at least one row.
    /// - The TX deleted at least one row.
    /// - The TX was the result of the reducers `__identity_connected__` or `__identity_disconnected__`.
    fn tx_consumes_offset(&self, tx_data: &TxData, ctx: &ExecutionContext) -> bool {
        // Avoid appending transactions to the commitlog which don't modify
        // any tables.
        //
        // An exception are connect / disconnect calls, which we always want
        // paired in the log, so as to be able to disconnect clients
        // automatically after a server crash. See:
        // [`crate::host::ModuleHost::call_identity_connected_disconnected`]
        //
        // Note that this may change in the future: some analytics and/or
        // timetravel queries may benefit from seeing all inputs, even if
        // the database state did not change.
        tx_data.has_rows_or_connect_disconnect(ctx.reducer_context().map(|rcx| &*rcx.name))
    }

    pub(super) fn drop_view_from_read_sets(&mut self, view_id: ViewId, sender: Option<Identity>) {
        self.read_sets.remove_view(view_id, sender)
    }

    pub(super) fn merge(&mut self, tx_state: TxState, read_sets: ViewReadSets, ctx: &ExecutionContext) -> TxData {
        let mut tx_data = TxData::default();
        let mut truncates = IntSet::default();

        // First, apply deletes. This will free up space in the committed tables.
        self.merge_apply_deletes(
            &mut tx_data,
            tx_state.delete_tables,
            tx_state.pending_schema_changes,
            &mut truncates,
        );

        // Then, apply inserts. This will re-fill the holes freed by deletions
        // before allocating new pages.
        self.merge_apply_inserts(
            &mut tx_data,
            tx_state.insert_tables,
            tx_state.blob_store,
            &mut truncates,
        );

        // Record any truncated tables in the `TxData`.
        tx_data.add_truncates(truncates);

        // Merge read sets from the `MutTxId` into the `CommittedState`.
        // It's important that this happens after applying the changes to `tx_data`,
        // which implies `tx_data` already contains inserts and deletes for view tables
        // so that we can pass updated set of table ids.
        self.merge_read_sets(read_sets);

        // Store in `tx_data` which of the updated tables are ephemeral.
        // NOTE: This must be called before `tx_consumes_offset`, so that
        // all-ephemeral transactions do not consume a tx offset.
        tx_data.set_ephemeral_tables(&self.ephemeral_tables);

        // If the TX will be logged, record its projected tx offset,
        // then increment the counter.
        if self.tx_consumes_offset(&tx_data, ctx) {
            tx_data.set_tx_offset(self.next_tx_offset);
            self.next_tx_offset += 1;
        }

        tx_data
    }

    fn merge_read_sets(&mut self, read_sets: ViewReadSets) {
        self.read_sets.merge(read_sets)
    }

    fn merge_apply_deletes(
        &mut self,
        tx_data: &mut TxData,
        delete_tables: BTreeMap<TableId, DeleteTable>,
        pending_schema_changes: ThinVec<PendingSchemaChange>,
        truncates: &mut IntSet<TableId>,
    ) {
        fn delete_rows(
            tx_data: &mut TxData,
            table_id: TableId,
            table: &mut Table,
            blob_store: &mut dyn BlobStore,
            row_ptrs_len: usize,
            row_ptrs: impl Iterator<Item = RowPointer>,
            truncates: &mut IntSet<TableId>,
        ) {
            let mut deletes = Vec::with_capacity(row_ptrs_len);

            // Note: we maintain the invariant that the delete_tables
            // holds only committed rows which should be deleted,
            // i.e. `RowPointer`s with `SquashedOffset::COMMITTED_STATE`,
            // so no need to check before applying the deletes.
            for row_ptr in row_ptrs {
                debug_assert!(row_ptr.squashed_offset().is_committed_state());

                // TODO: re-write `TxData` to remove `ProductValue`s
                let pv = table
                    .delete(blob_store, row_ptr, |row| row.to_product_value())
                    .expect("Delete for non-existent row!");
                deletes.push(pv);
            }

            if !deletes.is_empty() {
                let table_name = &*table.get_schema().table_name;
                tx_data.set_deletes_for_table(table_id, table_name, deletes.into());
                let truncated = table.row_count == 0;
                if truncated {
                    truncates.insert(table_id);
                }
            }
        }

        for (table_id, row_ptrs) in delete_tables {
            match self.get_table_and_blob_store_mut(table_id) {
                Ok((table, blob_store, ..)) => delete_rows(
                    tx_data,
                    table_id,
                    table,
                    blob_store,
                    row_ptrs.len(),
                    row_ptrs.iter(),
                    truncates,
                ),
                Err(_) if !row_ptrs.is_empty() => panic!("Deletion for non-existent table {table_id:?}... huh?"),
                Err(_) => {}
            }
        }

        // Delete all tables marked for deletion.
        // The order here does not matter as once a `table_id` has been dropped
        // it will never be re-created.
        for change in pending_schema_changes {
            if let PendingSchemaChange::TableRemoved(table_id, mut table) = change {
                let row_ptrs = table.scan_all_row_ptrs();
                truncates.insert(table_id);
                delete_rows(
                    tx_data,
                    table_id,
                    &mut table,
                    &mut self.blob_store,
                    row_ptrs.len(),
                    row_ptrs.into_iter(),
                    truncates,
                );
            }
        }
    }

    fn merge_apply_inserts(
        &mut self,
        tx_data: &mut TxData,
        insert_tables: BTreeMap<TableId, Table>,
        tx_blob_store: impl BlobStore,
        truncates: &mut IntSet<TableId>,
    ) {
        // TODO(perf): Consider moving whole pages from the `insert_tables` into the committed state,
        //             rather than copying individual rows out of them.
        //             This will require some magic to get the indexes right,
        //             and may lead to a large number of mostly-empty pages in the committed state.
        //             Likely we want to decide dynamically whether to move a page or copy its contents,
        //             based on the available holes in the committed state
        //             and the fullness of the page.

        for (table_id, tx_table) in insert_tables {
            let (commit_table, commit_blob_store, page_pool) =
                self.get_table_and_blob_store_or_create(table_id, tx_table.get_schema());

            // For each newly-inserted row, insert it into the committed state.
            let mut inserts = Vec::with_capacity(tx_table.row_count as usize);
            for row_ref in tx_table.scan_rows(&tx_blob_store) {
                let pv = row_ref.to_product_value();
                commit_table
                    .insert(page_pool, commit_blob_store, &pv)
                    .expect("Failed to insert when merging commit");

                inserts.push(pv);
            }

            // Add the table to `TxData` if there were insertions.
            if !inserts.is_empty() {
                let table_name = &*commit_table.get_schema().table_name;
                tx_data.set_inserts_for_table(table_id, table_name, inserts.into());

                // if table has inserted rows, it cannot be truncated
                if truncates.contains(&table_id) {
                    truncates.remove(&table_id);
                }
            }

            let (schema, _indexes, pages) = tx_table.consume_for_merge();

            // The schema may have been modified in the transaction.
            // Update this last to placate borrowck and avoid a clone.
            // None of the above operations will inspect the schema.
            commit_table.schema = schema;

            // Put all the pages in the table back into the pool.
            self.page_pool.put_many(pages);
        }
    }

    /// Rolls back the changes immediately made to the committed state during a transaction.
    pub(super) fn rollback(&mut self, seq_state: &mut SequencesState, tx_state: TxState) -> TxOffset {
        // Roll back the changes in the reverse order in which they were made
        // so that e.g., the last change is undone first.
        for change in tx_state.pending_schema_changes.into_iter().rev() {
            self.rollback_pending_schema_change(seq_state, change);
        }
        self.next_tx_offset.saturating_sub(1)
    }

    fn rollback_pending_schema_change(
        &mut self,
        seq_state: &mut SequencesState,
        change: PendingSchemaChange,
    ) -> Option<()> {
        use PendingSchemaChange::*;
        match change {
            // An index was removed. Add it back.
            IndexRemoved(table_id, index_id, table_index, index_schema) => {
                let table = self.tables.get_mut(&table_id)?;
                // SAFETY: `table_index` was derived from `table`.
                unsafe { table.add_index(index_id, table_index) };
                table.with_mut_schema(|s| s.update_index(index_schema));
                self.index_id_map.insert(index_id, table_id);
            }
            // An index was added. Remove it.
            IndexAdded(table_id, index_id, pointer_map) => {
                let table = self.tables.get_mut(&table_id)?;
                table.delete_index(&self.blob_store, index_id, pointer_map);
                table.with_mut_schema(|s| s.remove_index(index_id));
                self.index_id_map.remove(&index_id);
            }
            // A table was removed. Add it back.
            TableRemoved(table_id, table) => {
                let is_view_table = table.schema.is_view();
                // We don't need to deal with sub-components.
                // That is, we don't need to add back indices and such.
                // Instead, there will be separate pending schema changes like `IndexRemoved`.
                self.tables.insert(table_id, table);

                // Incase, the table was ephemeral, add it back to that set as well.
                if is_view_table {
                    self.ephemeral_tables.insert(table_id);
                }
            }
            // A table was added. Remove it.
            TableAdded(table_id) => {
                // We don't need to deal with sub-components.
                // That is, we don't need to remove indices and such.
                // Instead, there will be separate pending schema changes like `IndexAdded`.
                self.tables.remove(&table_id);
                // Incase, the table was ephemeral, remove it from that set as well.
                self.ephemeral_tables.remove(&table_id);
            }
            // A table's access was changed. Change back to the old one.
            TableAlterAccess(table_id, access) => {
                let table = self.tables.get_mut(&table_id)?;
                table.with_mut_schema(|s| s.table_access = access);
            }
            // A table's row type was changed. Change back to the old one.
            // The row representation of old rows hasn't changed,
            // so it's safe to not rewrite the rows and merely change the type back.
            TableAlterRowType(table_id, column_schemas) => {
                let table = self.tables.get_mut(&table_id)?;
                // SAFETY:
                // Let the "old" type/schema be the one in `column_schemas`.
                // Let the "new" type/schema be the one used by the table which we are rolling back.
                // There's no need to validate "old",
                // as it was the row type prior to the change which we're rolling back.
                // We can use "old", as this is the commit table,
                // which is immutable to row addition during a transaction,
                // and thus will only have rows compatible with it.
                // The rows in the tx state might not be, as they may use e.g., a new variant.
                // However, we don't care about that, as the tx state is being discarded.
                unsafe { table.change_columns_to_unchecked(column_schemas, |_, _, _| Ok::<_, Infallible>(())) }
                    .unwrap_or_else(|e| match e {});
            }
            // A constraint was removed. Add it back.
            ConstraintRemoved(table_id, constraint_schema) => {
                let table = self.tables.get_mut(&table_id)?;
                table.with_mut_schema(|s| s.update_constraint(constraint_schema));
            }
            // A constraint was added. Remove it.
            ConstraintAdded(table_id, constraint_id) => {
                let table = self.tables.get_mut(&table_id)?;
                table.with_mut_schema(|s| s.remove_constraint(constraint_id));
            }
            // A sequence was removed. Add it back.
            SequenceRemoved(table_id, seq, schema) => {
                let table = self.tables.get_mut(&table_id)?;
                table.with_mut_schema(|s| s.update_sequence(schema));
                seq_state.insert(seq);
            }
            // A sequence was added. Remove it.
            SequenceAdded(table_id, sequence_id) => {
                let table = self.tables.get_mut(&table_id)?;
                table.with_mut_schema(|s| s.remove_sequence(sequence_id));
                seq_state.remove(sequence_id);
            }
        }

        Some(())
    }

    pub(super) fn get_table(&self, table_id: TableId) -> Option<&Table> {
        self.tables.get(&table_id)
    }

    #[allow(clippy::unnecessary_lazy_evaluations)]
    pub fn get_table_and_blob_store(&self, table_id: TableId) -> Result<CommitTableForInsertion<'_>> {
        let table = self
            .get_table(table_id)
            .ok_or_else(|| TableError::IdNotFoundState(table_id))?;
        Ok((table, &self.blob_store as &dyn BlobStore, &self.index_id_map))
    }

    pub(super) fn get_table_and_blob_store_mut(
        &mut self,
        table_id: TableId,
    ) -> Result<(&mut Table, &mut dyn BlobStore, &mut IndexIdMap, &PagePool)> {
        // NOTE(centril): `TableError` is a fairly large type.
        // Not making this lazy made `TableError::drop` show up in perf.
        // TODO(centril): Box all the errors.
        #[allow(clippy::unnecessary_lazy_evaluations)]
        let table = self
            .tables
            .get_mut(&table_id)
            .ok_or_else(|| TableError::IdNotFoundState(table_id))?;
        Ok((
            table,
            &mut self.blob_store as &mut dyn BlobStore,
            &mut self.index_id_map,
            &self.page_pool,
        ))
    }

    fn make_table(schema: Arc<TableSchema>) -> Table {
        Table::new(schema, SquashedOffset::COMMITTED_STATE)
    }

    fn create_table(&mut self, table_id: TableId, schema: Arc<TableSchema>) {
        self.tables.insert(table_id, Self::make_table(schema));
    }

    pub(super) fn get_table_and_blob_store_or_create<'this>(
        &'this mut self,
        table_id: TableId,
        schema: &Arc<TableSchema>,
    ) -> (&'this mut Table, &'this mut dyn BlobStore, &'this PagePool) {
        let table = self
            .tables
            .entry(table_id)
            .or_insert_with(|| Self::make_table(schema.clone()));
        let blob_store = &mut self.blob_store;
        let pool = &self.page_pool;
        (table, blob_store, pool)
    }

    /// Returns an iterator over all persistent tables (i.e., non-ephemeral tables)
    pub(super) fn persistent_tables_and_blob_store(&mut self) -> (impl Iterator<Item = &mut Table>, &HashMapBlobStore) {
        (
            self.tables
                .iter_mut()
                .filter(|(table_id, _)| !self.ephemeral_tables.contains(*table_id))
                .map(|(_, table)| table),
            &self.blob_store,
        )
    }

    pub fn report_data_size(&self, database_identity: Identity) {
        use crate::db_metrics::data_size::DATA_SIZE_METRICS;

        for (_, table) in &self.tables {
            let table_name = &table.schema.table_name;
            DATA_SIZE_METRICS
                .data_size_table_num_rows
                .with_label_values(&database_identity, table_name)
                .set(table.num_rows() as _);
            DATA_SIZE_METRICS
                .data_size_table_bytes_used_by_rows
                .with_label_values(&database_identity, table_name)
                .set(table.bytes_used_by_rows() as _);
            DATA_SIZE_METRICS
                .data_size_table_num_rows_in_indexes
                .with_label_values(&database_identity, table_name)
                .set(table.num_rows_in_indexes() as _);
            DATA_SIZE_METRICS
                .data_size_table_bytes_used_by_index_keys
                .with_label_values(&database_identity, table_name)
                .set(table.bytes_used_by_index_keys() as _);
        }

        DATA_SIZE_METRICS
            .data_size_blob_store_num_blobs
            .with_label_values(&database_identity)
            .set(self.blob_store.num_blobs() as _);
        DATA_SIZE_METRICS
            .data_size_blob_store_bytes_used_by_blobs
            .with_label_values(&database_identity)
            .set(self.blob_store.bytes_used_by_blobs() as _);
    }
}

pub(super) type CommitTableForInsertion<'a> = (&'a Table, &'a dyn BlobStore, &'a IndexIdMap);
