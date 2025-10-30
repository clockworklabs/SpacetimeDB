use super::{
    datastore::Result,
    delete_table::DeleteTable,
    sequence::{Sequence, SequencesState},
    state_view::{IterByColRangeTx, IterTx, ScanIterByColRangeTx, StateView},
    tx_state::{IndexIdMap, PendingSchemaChange, TxState},
    IterByColEqTx,
};
use crate::{
    db_metrics::DB_METRICS,
    error::{DatastoreError, IndexError, TableError},
    execution_context::ExecutionContext,
    locking_tx_datastore::{mut_tx::ViewReadSets, state_view::iter_st_column_for_table},
    system_tables::{
        system_tables, StColumnRow, StConstraintData, StConstraintRow, StIndexRow, StSequenceRow, StTableFields,
        StTableRow, SystemTable, ST_CLIENT_ID, ST_CLIENT_IDX, ST_COLUMN_ID, ST_COLUMN_IDX, ST_COLUMN_NAME,
        ST_CONSTRAINT_ID, ST_CONSTRAINT_IDX, ST_CONSTRAINT_NAME, ST_INDEX_ID, ST_INDEX_IDX, ST_INDEX_NAME,
        ST_MODULE_ID, ST_MODULE_IDX, ST_ROW_LEVEL_SECURITY_ID, ST_ROW_LEVEL_SECURITY_IDX, ST_SCHEDULED_ID,
        ST_SCHEDULED_IDX, ST_SEQUENCE_ID, ST_SEQUENCE_IDX, ST_SEQUENCE_NAME, ST_TABLE_ID, ST_TABLE_IDX, ST_VAR_ID,
        ST_VAR_IDX, ST_VIEW_ARG_ID, ST_VIEW_ARG_IDX,
    },
    traits::TxData,
};
use crate::{
    locking_tx_datastore::mut_tx::ReadSet,
    system_tables::{
        ST_CONNECTION_CREDENTIALS_ID, ST_CONNECTION_CREDENTIALS_IDX, ST_VIEW_CLIENT_ID, ST_VIEW_CLIENT_IDX,
        ST_VIEW_COLUMN_ID, ST_VIEW_COLUMN_IDX, ST_VIEW_ID, ST_VIEW_IDX, ST_VIEW_PARAM_ID, ST_VIEW_PARAM_IDX,
    },
};
use anyhow::anyhow;
use core::{convert::Infallible, ops::RangeBounds};
use spacetimedb_data_structures::map::{HashMap, HashSet, IntMap, IntSet};
use spacetimedb_durability::TxOffset;
use spacetimedb_lib::{db::auth::StTableType, Identity};
use spacetimedb_primitives::{ColId, ColList, ColSet, IndexId, TableId, ViewId};
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
    table::{IndexScanRangeIter, InsertError, RowRef, Table, TableAndIndex},
};
use std::collections::BTreeMap;
use std::sync::Arc;
use thin_vec::ThinVec;

type IndexKeyReadSet = HashMap<AlgebraicValue, IntSet<ViewId>>;
type IndexColReadSet = HashMap<ColList, IndexKeyReadSet>;

#[derive(Default)]
struct CommittedReadSets {
    tables: IntMap<TableId, IntSet<ViewId>>,
    index_keys: IntMap<TableId, IndexColReadSet>,
}

impl MemoryUsage for CommittedReadSets {
    fn heap_usage(&self) -> usize {
        self.tables.heap_usage() + self.index_keys.heap_usage()
    }
}

impl CommittedReadSets {
    /// Record in the [`CommittedState`] that this view scans this table
    fn view_scans_table(&mut self, view_id: ViewId, table_id: TableId) {
        self.tables.entry(table_id).or_default().insert(view_id);
    }

    /// Record in the [`CommittedState`] that this view reads this index `key` for these table `cols`
    fn view_reads_index_key(&mut self, view_id: ViewId, table_id: TableId, cols: ColList, key: &AlgebraicValue) {
        self.index_keys
            .entry(table_id)
            .or_default()
            .entry(cols)
            .or_default()
            .entry(key.clone())
            .or_default()
            .insert(view_id);
    }
}

/// Contains the live, in-memory snapshot of a database. This structure
/// is exposed in order to support tools wanting to process the commit
/// logs directly. For normal usage, see the RelationalDB struct instead.
///
/// NOTE: unstable API, this may change at any point in the future.
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
    /// Whether the table was dropped during replay.
    /// TODO(centril): Only used during bootstrap and is otherwise unused.
    /// We should split `CommittedState` into two types
    /// where one, e.g., `ReplayCommittedState`, has this field.
    table_dropped: IntSet<TableId>,
    /// We track the read sets for each view in the committed state.
    /// We check each reducer's write set against these read sets.
    /// Any overlap will trigger a re-evaluation of the affected view,
    /// and its read set will be updated accordingly.
    read_sets: CommittedReadSets,
}

impl MemoryUsage for CommittedState {
    fn heap_usage(&self) -> usize {
        let Self {
            next_tx_offset,
            tables,
            blob_store,
            index_id_map,
            page_pool: _,
            table_dropped,
            read_sets,
        } = self;
        // NOTE(centril): We do not want to include the heap usage of `page_pool` as it's a shared resource.
        next_tx_offset.heap_usage()
            + tables.heap_usage()
            + blob_store.heap_usage()
            + index_id_map.heap_usage()
            + table_dropped.heap_usage()
            + read_sets.heap_usage()
    }
}

impl StateView for CommittedState {
    type Iter<'a> = IterTx<'a>;
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
        if self.table_name(table_id).is_some() {
            return Ok(IterTx::new(table_id, self));
        }
        Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into())
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
        match self.index_seek(table_id, &cols, &range) {
            Some(iter) => Ok(IterByColRangeTx::Index(iter)),
            None => Ok(IterByColRangeTx::Scan(ScanIterByColRangeTx::new(
                self.iter(table_id)?,
                cols,
                range,
            ))),
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

impl CommittedState {
    pub(super) fn new(page_pool: PagePool) -> Self {
        Self {
            next_tx_offset: <_>::default(),
            tables: <_>::default(),
            blob_store: <_>::default(),
            index_id_map: <_>::default(),
            table_dropped: <_>::default(),
            read_sets: <_>::default(),
            page_pool,
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
        self.create_table(ST_VIEW_CLIENT_ID, schemas[ST_VIEW_CLIENT_IDX].clone());
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
        if self.table_dropped.contains(&table_id) {
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
        if self.table_dropped.contains(&table_id) {
            return Ok(());
        }

        // Get the table for mutation.
        let (table, blob_store, _, page_pool) = self.get_table_and_blob_store_mut(table_id)?;

        // Delete the row.
        table
            .delete_equal_row(page_pool, blob_store, row)
            .map_err(TableError::Bflatn)?
            .ok_or_else(|| anyhow!("Delete for non-existent row when replaying transaction"))?;

        if table_id == ST_TABLE_ID {
            // A row was removed from `st_table`, so a table was dropped.
            // Remove that table from the in-memory structures.
            let dropped_table_id = Self::read_table_id(row);
            self.tables
                .remove(&dropped_table_id)
                .unwrap_or_else(|| panic!("table {} to remove should exist", dropped_table_id));
            // Mark the table as dropped so that when
            // processing row deletions for that table later,
            // they are simply ignored in (1).
            self.table_dropped.insert(dropped_table_id);
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
        let (_, row_ref) = table.insert(pool, blob_store, row).map_err(|e| -> DatastoreError {
            match e {
                InsertError::Bflatn(e) => TableError::Bflatn(e).into(),
                InsertError::Duplicate(e) => TableError::Duplicate(e).into(),
                InsertError::IndexError(e) => IndexError::UniqueConstraintViolation(e).into(),
            }
        })?;

        if table_id == ST_COLUMN_ID {
            // We've made a modification to `st_column`.
            // The type of a table has changed, so figure out which.
            // The first column in `StColumnRow` is `table_id`.
            let row_ptr = row_ref.pointer();
            self.st_column_changed(row, row_ptr)?;
        }

        Ok(())
    }

    /// Refreshes the columns and layout of a table
    /// when a `row` has been inserted from `st_column`.
    ///
    /// The `row_ptr` is a pointer to `row`.
    fn st_column_changed(&mut self, row: &ProductValue, row_ptr: RowPointer) -> Result<()> {
        let target_table_id = Self::read_table_id(row);
        let target_col_id = ColId::deserialize(ValueDeserializer::from_ref(&row.elements[1]))
            .expect("second field in `st_column` should decode to a `ColId`");

        // We're replaying and we don't have unique constraints yet.
        // Due to replay handling all inserts first and deletes after,
        // when processing `st_column` insert/deletes,
        // we may end up with two definitions for the same `col_pos`.
        // Of those two, we're interested in the one we just inserted
        // and not the other one, as it is being replaced.
        let mut columns = iter_st_column_for_table(self, &target_table_id.into())?
            .filter_map(|row_ref| {
                StColumnRow::try_from(row_ref)
                    .map(|c| (c.col_pos != target_col_id || row_ref.pointer() == row_ptr).then(|| c.into()))
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()?;

        // Columns in `st_column` are not in general sorted by their `col_pos`,
        // though they will happen to be for tables which have never undergone migrations
        // because their initial insertion order matches their `col_pos` order.
        columns.sort_by_key(|col: &ColumnSchema| col.col_pos);

        // Update the columns and layout of the the in-memory table.
        if let Some(table) = self.tables.get_mut(&target_table_id) {
            table.change_columns_to(columns).map_err(TableError::from)?;
        }

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

    /// When there's an index on `cols`,
    /// returns an iterator over the [TableIndex] that yields all the [`RowRef`]s
    /// that match the specified `range` in the indexed column.
    ///
    /// Matching is defined by `Ord for AlgebraicValue`.
    ///
    /// For a unique index this will always yield at most one `RowRef`.
    /// When there is no index this returns `None`.
    pub(super) fn index_seek<'a>(
        &'a self,
        table_id: TableId,
        cols: &ColList,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<IndexScanRangeIter<'a>> {
        self.tables
            .get(&table_id)?
            .get_index_by_cols_with_table(&self.blob_store, cols)
            .map(|i| i.seek_range(range))
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
        tx_data.has_rows_or_connect_disconnect(ctx.reducer_context())
    }

    pub(super) fn merge(&mut self, tx_state: TxState, read_sets: ViewReadSets, ctx: &ExecutionContext) -> TxData {
        let mut tx_data = TxData::default();
        let mut truncates = IntSet::default();

        // Merge read sets from the `MutTxId` into the `CommittedState`
        self.merge_read_sets(read_sets);

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

        // If the TX will be logged, record its projected tx offset,
        // then increment the counter.
        if self.tx_consumes_offset(&tx_data, ctx) {
            tx_data.set_tx_offset(self.next_tx_offset);
            self.next_tx_offset += 1;
        }

        tx_data
    }

    fn merge_read_set(&mut self, view_id: ViewId, read_set: ReadSet) {
        for table_id in read_set.tables_scanned() {
            self.read_sets.view_scans_table(view_id, *table_id);
        }
        for (table_id, index_id, key) in read_set.index_keys_scanned() {
            if let Some(cols) = self
                .get_schema(*table_id)
                .map(|table_schema| table_schema.col_list_for_index_id(*index_id))
            {
                self.read_sets.view_reads_index_key(view_id, *table_id, cols, key);
            }
        }
    }

    fn merge_read_sets(&mut self, read_sets: ViewReadSets) {
        for (view_id, read_set) in read_sets {
            self.merge_read_set(view_id, read_set);
        }
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
                // We don't need to deal with sub-components.
                // That is, we don't need to add back indices and such.
                // Instead, there will be separate pending schema changes like `IndexRemoved`.
                self.tables.insert(table_id, table);
            }
            // A table was added. Remove it.
            TableAdded(table_id) => {
                // We don't need to deal with sub-components.
                // That is, we don't need to remove indices and such.
                // Instead, there will be separate pending schema changes like `IndexAdded`.
                self.tables.remove(&table_id);
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
