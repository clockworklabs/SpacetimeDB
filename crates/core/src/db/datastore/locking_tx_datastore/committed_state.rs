use super::{
    datastore::Result,
    delete_table::DeleteTable,
    sequence::{Sequence, SequencesState},
    state_view::{IterByColRangeTx, IterTx, ScanIterByColRangeTx, StateView},
    tx_state::{IndexIdMap, PendingSchemaChange, TxState},
    IterByColEqTx,
};
use crate::{
    db::{
        datastore::{
            error::{IndexError, TableError},
            system_tables::{
                system_tables, StColumnRow, StConstraintData, StConstraintRow, StIndexRow, StSequenceRow,
                StTableFields, StTableRow, SystemTable, ST_CLIENT_ID, ST_CLIENT_IDX, ST_COLUMN_ID, ST_COLUMN_IDX,
                ST_COLUMN_NAME, ST_CONSTRAINT_ID, ST_CONSTRAINT_IDX, ST_CONSTRAINT_NAME, ST_INDEX_ID, ST_INDEX_IDX,
                ST_INDEX_NAME, ST_MODULE_ID, ST_MODULE_IDX, ST_RESERVED_SEQUENCE_RANGE, ST_ROW_LEVEL_SECURITY_ID,
                ST_ROW_LEVEL_SECURITY_IDX, ST_SCHEDULED_ID, ST_SCHEDULED_IDX, ST_SEQUENCE_ID, ST_SEQUENCE_IDX,
                ST_SEQUENCE_NAME, ST_TABLE_ID, ST_TABLE_IDX, ST_VAR_ID, ST_VAR_IDX,
            },
            traits::TxData,
        },
        db_metrics::DB_METRICS,
    },
    execution_context::ExecutionContext,
};
use anyhow::anyhow;
use core::ops::RangeBounds;
use itertools::Itertools;
use spacetimedb_data_structures::map::{HashSet, IntMap};
use spacetimedb_lib::{
    db::auth::{StAccess, StTableType},
    Identity,
};
use spacetimedb_primitives::{ColList, ColSet, IndexId, TableId};
use spacetimedb_sats::{AlgebraicValue, ProductValue};
use spacetimedb_schema::{def::IndexAlgorithm, schema::TableSchema};
use spacetimedb_table::{
    blob_store::{BlobStore, HashMapBlobStore},
    indexes::{RowPointer, SquashedOffset},
    page_pool::PagePool,
    table::{IndexScanRangeIter, InsertError, RowRef, Table, TableAndIndex},
    MemoryUsage,
};
use std::collections::BTreeMap;
use std::sync::Arc;

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
}

impl MemoryUsage for CommittedState {
    fn heap_usage(&self) -> usize {
        let Self {
            next_tx_offset,
            tables,
            blob_store,
            index_id_map,
            page_pool: _,
        } = self;
        // NOTE(centril): We do not want to include the heap usage of `page_pool` as it's a shared resource.
        next_tx_offset.heap_usage() + tables.heap_usage() + blob_store.heap_usage() + index_id_map.heap_usage()
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

/// Swallow `Err(TableError::Duplicate(_))`, which signals a set-semantic collision,
/// and convert it into `Ok(())`.
///
/// This necessarily drops any `Ok` payload, since a `Duplicate` will not hold that payload.
///
/// Used because the datastore's external APIs (as in, those that face the rest of SpacetimeDB)
/// treat duplicate insertion as a silent no-op,
/// whereas `Table` and friends treat it as an error.
fn ignore_duplicate_insert_error<T>(res: std::result::Result<T, InsertError>) -> Result<()> {
    match res {
        Ok(_) => Ok(()),
        Err(InsertError::Duplicate(_)) => Ok(()),
        Err(InsertError::Bflatn(e)) => Err(e.into()),
        Err(InsertError::IndexError(e)) => Err(IndexError::from(e).into()),
    }
}

impl CommittedState {
    pub(super) fn new(page_pool: PagePool) -> Self {
        Self {
            next_tx_offset: <_>::default(),
            tables: <_>::default(),
            blob_store: <_>::default(),
            index_id_map: <_>::default(),
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
                table_access: StAccess::Public,
                table_primary_key: schema.primary_key.map(Into::into),
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_TABLES.
            // If the row is already there, no-op.
            ignore_duplicate_insert_error(st_tables.insert(pool, blob_store, &row))?;
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
            // If the row is already there, no-op.
            ignore_duplicate_insert_error(st_columns.insert(pool, blob_store, &row))?;
            // Increment row count for st_columns.
            with_label_values(ST_COLUMN_ID, ST_COLUMN_NAME).inc();
        }

        // Insert the FK sorted by table/column so it show together when queried.

        // Insert constraints into `st_constraints`
        let (st_constraints, blob_store, pool) =
            self.get_table_and_blob_store_or_create(ST_CONSTRAINT_ID, &schemas[ST_CONSTRAINT_IDX]);
        for (i, constraint) in ref_schemas
            .iter()
            .flat_map(|x| &x.constraints)
            .sorted_by_key(|x| (x.table_id, x.data.unique_columns()))
            .cloned()
            .enumerate()
        {
            // Start sequence from 1,
            // to avoid any confusion with 0 as the autoinc sentinel value.
            let constraint_id = (i + 1).into();
            let row = StConstraintRow {
                constraint_id,
                constraint_name: constraint.constraint_name,
                table_id: constraint.table_id,
                constraint_data: constraint.data.into(),
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_CONSTRAINTS.
            // If the row is already there, no-op.
            ignore_duplicate_insert_error(st_constraints.insert(pool, blob_store, &row))?;
            // Increment row count for st_constraints.
            with_label_values(ST_CONSTRAINT_ID, ST_CONSTRAINT_NAME).inc();
        }

        // Insert the indexes into `st_indexes`
        let (st_indexes, blob_store, pool) =
            self.get_table_and_blob_store_or_create(ST_INDEX_ID, &schemas[ST_INDEX_IDX]);
        for (i, mut index) in ref_schemas
            .iter()
            .flat_map(|x| &x.indexes)
            .sorted_by_key(|x| (x.table_id, x.index_algorithm.columns()))
            .cloned()
            .enumerate()
        {
            // Start sequence from 1,
            // to avoid any confusion with 0 as the autoinc sentinel value.
            index.index_id = (i + 1).into();
            let row: StIndexRow = index.into();
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_INDEXES.
            // If the row is already there, no-op.
            ignore_duplicate_insert_error(st_indexes.insert(pool, blob_store, &row))?;
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

        // IMPORTANT: It is crucial that the `st_sequences` table is created last

        // Insert the sequences into `st_sequences`
        let (st_sequences, blob_store, pool) =
            self.get_table_and_blob_store_or_create(ST_SEQUENCE_ID, &schemas[ST_SEQUENCE_IDX]);
        // We create sequences last to get right the starting number
        // so, we don't sort here
        for (i, col) in ref_schemas.iter().flat_map(|x| &x.sequences).enumerate() {
            // Start sequence from 1,
            // to avoid any confusion with 0 as the autoinc sentinel value.
            let sequence_id = (i + 1).into();
            let row = StSequenceRow {
                sequence_id,
                sequence_name: col.sequence_name.clone(),
                table_id: col.table_id,
                col_pos: col.col_pos,
                increment: col.increment,
                min_value: col.min_value,
                max_value: col.max_value,
                // All sequences for system tables start from the reserved
                // range + 1.
                // Logically, we thus have used up the default pre-allocation
                // and must allocate again on the next increment.
                start: ST_RESERVED_SEQUENCE_RANGE as i128 + 1,
                allocated: ST_RESERVED_SEQUENCE_RANGE as i128,
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_SEQUENCES.
            // If the row is already there, no-op.
            ignore_duplicate_insert_error(st_sequences.insert(pool, blob_store, &row))?;
            // Increment row count for st_sequences
            with_label_values(ST_SEQUENCE_ID, ST_SEQUENCE_NAME).inc();
        }

        self.reset_system_table_schemas()?;

        Ok(())
    }

    /// Compute the system table schemas from the system tables,
    /// and store those schemas in the in-memory [`Table`] structures.
    ///
    /// Necessary during bootstrap because system tables include auto_inc IDs
    /// for objects like indexes and constraints
    /// which are computed at insert-time,
    /// and therefore not included in the hardcoded schemas.
    pub(super) fn reset_system_table_schemas(&mut self) -> Result<()> {
        // Re-read the schema with the correct ids...
        for schema in system_tables() {
            self.tables.get_mut(&schema.table_id).unwrap().schema =
                Arc::new(self.schema_for_table_raw(schema.table_id)?);
        }

        Ok(())
    }

    pub(super) fn replay_delete_by_rel(&mut self, table_id: TableId, rel: &ProductValue) -> Result<()> {
        let table = self
            .tables
            .get_mut(&table_id)
            .ok_or(TableError::IdNotFoundState(table_id))?;
        let blob_store = &mut self.blob_store;
        table
            .delete_equal_row(&self.page_pool, blob_store, rel)
            .map_err(TableError::Bflatn)?
            .ok_or_else(|| anyhow!("Delete for non-existent row when replaying transaction"))?;
        Ok(())
    }

    pub(super) fn replay_insert(
        &mut self,
        table_id: TableId,
        schema: &Arc<TableSchema>,
        row: &ProductValue,
    ) -> Result<()> {
        let (table, blob_store, pool) = self.get_table_and_blob_store_or_create(table_id, schema);
        table.insert(pool, blob_store, row).map(drop).map_err(|e| match e {
            InsertError::Bflatn(e) => TableError::Bflatn(e).into(),
            InsertError::Duplicate(e) => TableError::Duplicate(e).into(),
            InsertError::IndexError(e) => IndexError::UniqueConstraintViolation(e).into(),
        })
    }

    pub(super) fn build_sequence_state(&mut self, sequence_state: &mut SequencesState) -> Result<()> {
        let st_sequences = self.tables.get(&ST_SEQUENCE_ID).unwrap();
        for row_ref in st_sequences.scan_rows(&self.blob_store) {
            let sequence = StSequenceRow::try_from(row_ref)?;
            let mut seq = Sequence::new(sequence.into());

            // Now we need to recover the last allocation value.
            if seq.value < seq.allocated() + 1 {
                seq.value = seq.allocated() + 1;
            }

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
        Ok(())
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
            let (Some(table), blob_store, index_id_map) = self.get_table_and_blob_store_mut(table_id) else {
                panic!("Cannot create index for table which doesn't exist in committed state");
            };
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
            .tables
            .get(&table_id)
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

    pub(super) fn merge(&mut self, tx_state: TxState, ctx: &ExecutionContext) -> TxData {
        let mut tx_data = TxData::default();

        // First, apply deletes. This will free up space in the committed tables.
        self.merge_apply_deletes(&mut tx_data, tx_state.delete_tables);

        // Then, apply inserts. This will re-fill the holes freed by deletions
        // before allocating new pages.
        self.merge_apply_inserts(&mut tx_data, tx_state.insert_tables, tx_state.blob_store);

        // If the TX will be logged, record its projected tx offset,
        // then increment the counter.
        if self.tx_consumes_offset(&tx_data, ctx) {
            tx_data.set_tx_offset(self.next_tx_offset);
            self.next_tx_offset += 1;
        }

        tx_data
    }

    fn merge_apply_deletes(&mut self, tx_data: &mut TxData, delete_tables: BTreeMap<TableId, DeleteTable>) {
        for (table_id, row_ptrs) in delete_tables {
            if let (Some(table), blob_store, _) = self.get_table_and_blob_store_mut(table_id) {
                let mut deletes = Vec::with_capacity(row_ptrs.len());

                // Note: we maintain the invariant that the delete_tables
                // holds only committed rows which should be deleted,
                // i.e. `RowPointer`s with `SquashedOffset::COMMITTED_STATE`,
                // so no need to check before applying the deletes.
                for row_ptr in row_ptrs.iter() {
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
                }
            } else if !row_ptrs.is_empty() {
                panic!("Deletion for non-existent table {:?}... huh?", table_id);
            }
        }
    }

    fn merge_apply_inserts(
        &mut self,
        tx_data: &mut TxData,
        insert_tables: BTreeMap<TableId, Table>,
        tx_blob_store: impl BlobStore,
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
    pub(super) fn rollback(&mut self, seq_state: &mut SequencesState, tx_state: TxState) {
        // Roll back the changes in the reverse order in which they were made
        // so that e.g., the last change is undone first.
        for change in tx_state.pending_schema_changes.into_iter().rev() {
            self.rollback_pending_schema_change(seq_state, change);
        }
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
    ) -> (Option<&mut Table>, &mut dyn BlobStore, &mut IndexIdMap) {
        (
            self.tables.get_mut(&table_id),
            &mut self.blob_store as &mut dyn BlobStore,
            &mut self.index_id_map,
        )
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
        use crate::db::db_metrics::data_size::DATA_SIZE_METRICS;

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
