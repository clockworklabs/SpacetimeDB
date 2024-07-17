use super::{
    datastore::Result,
    sequence::{Sequence, SequencesState},
    state_view::{Iter, IterByColRange, ScanIterByColRange, StateView},
    tx_state::{DeleteTable, TxState},
};
use crate::{
    db::{
        datastore::{
            system_tables::{
                system_tables, StColumnRow, StConstraintRow, StIndexRow, StSequenceRow, StTableFields, StTableRow,
                SystemTable, ST_CLIENTS_ID, ST_CLIENT_IDX, ST_COLUMNS_ID, ST_COLUMNS_IDX, ST_COLUMNS_NAME,
                ST_CONSTRAINTS_ID, ST_CONSTRAINTS_IDX, ST_CONSTRAINTS_NAME, ST_INDEXES_ID, ST_INDEXES_IDX,
                ST_INDEXES_NAME, ST_MODULE_ID, ST_MODULE_IDX, ST_RESERVED_SEQUENCE_RANGE, ST_SCHEDULED_ID,
                ST_SCHEDULED_IDX, ST_SEQUENCES_ID, ST_SEQUENCES_IDX, ST_SEQUENCES_NAME, ST_TABLES_ID, ST_TABLES_IDX,
                ST_VAR_ID, ST_VAR_IDX,
            },
            traits::TxData,
        },
        db_metrics::DB_METRICS,
    },
    error::TableError,
    execution_context::ExecutionContext,
};
use anyhow::anyhow;
use core::ops::RangeBounds;
use itertools::Itertools;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_lib::address::Address;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::{
    db::{
        auth::{StAccess, StTableType},
        def::TableSchema,
    },
    AlgebraicValue, ProductValue,
};
use spacetimedb_table::{
    blob_store::{BlobStore, HashMapBlobStore},
    indexes::{RowPointer, SquashedOffset},
    table::{IndexScanIter, InsertError, RowRef, Table},
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Contains the live, in-memory snapshot of a database. This structure
/// is exposed in order to support tools wanting to process the commit
/// logs directly. For normal usage, see the RelationalDB struct instead.
///
/// NOTE: unstable API, this may change at any point in the future.
#[derive(Default)]
pub struct CommittedState {
    pub(crate) next_tx_offset: u64,
    pub(crate) tables: IntMap<TableId, Table>,
    pub(crate) blob_store: HashMapBlobStore,
}

impl StateView for CommittedState {
    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>> {
        self.tables.get(&table_id).map(|table| table.get_schema())
    }
    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: TableId) -> Result<Iter<'a>> {
        if let Some(table_name) = self.table_name(table_id) {
            return Ok(Iter::new(ctx, table_id, table_name, None, self));
        }
        Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into())
    }
    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: TableId,
        cols: ColList,
        range: R,
    ) -> Result<IterByColRange<'a, R>> {
        // TODO: Why does this unconditionally return a `Scan` iter,
        // instead of trying to return a `CommittedIndex` iter?
        Ok(IterByColRange::Scan(ScanIterByColRange::new(
            self.iter(ctx, table_id)?,
            cols,
            range,
        )))
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
        // TODO(error-handling): impl From<InsertError> for DBError.
        Err(err) => Err(TableError::Insert(err).into()),
    }
}

impl CommittedState {
    pub fn bootstrap_system_tables(&mut self, database_address: Address) -> Result<()> {
        // NOTE: the `rdb_num_table_rows` metric is used by the query optimizer,
        // and therefore has performance implications and must not be disabled.
        let with_label_values = |table_id: TableId, table_name: &str| {
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &table_id.0, table_name)
        };

        let schemas = system_tables().map(Arc::new);
        let ref_schemas = schemas.each_ref().map(|s| &**s);

        // Insert the table row into st_tables, creating st_tables if it's missing.
        let (st_tables, blob_store) = self.get_table_and_blob_store_or_create(ST_TABLES_ID, &schemas[ST_TABLES_IDX]);
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
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_TABLES.
            // If the row is already there, no-op.
            ignore_duplicate_insert_error(st_tables.insert(blob_store, &row))?;
        }

        // Insert the columns into `st_columns`
        let (st_columns, blob_store) = self.get_table_and_blob_store_or_create(ST_COLUMNS_ID, &schemas[ST_COLUMNS_IDX]);
        for col in ref_schemas.iter().flat_map(|x| x.columns()).cloned() {
            let row = StColumnRow {
                table_id: col.table_id,
                col_pos: col.col_pos,
                col_name: col.col_name,
                col_type: col.col_type,
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_COLUMNS.
            // If the row is already there, no-op.
            ignore_duplicate_insert_error(st_columns.insert(blob_store, &row))?;
            // Increment row count for st_columns.
            with_label_values(ST_COLUMNS_ID, ST_COLUMNS_NAME).inc();
        }

        // Insert the FK sorted by table/column so it show together when queried.

        // Insert constraints into `st_constraints`
        let (st_constraints, blob_store) =
            self.get_table_and_blob_store_or_create(ST_CONSTRAINTS_ID, &schemas[ST_CONSTRAINTS_IDX]);
        for (i, constraint) in ref_schemas
            .iter()
            .flat_map(|x| &x.constraints)
            .sorted_by_key(|x| (x.table_id, &x.columns))
            .cloned()
            .enumerate()
        {
            let row = StConstraintRow {
                constraint_id: i.into(),
                columns: constraint.columns,
                constraint_name: constraint.constraint_name,
                constraints: constraint.constraints,
                table_id: constraint.table_id,
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_CONSTRAINTS.
            // If the row is already there, no-op.
            ignore_duplicate_insert_error(st_constraints.insert(blob_store, &row))?;
            // Increment row count for st_constraints.
            with_label_values(ST_CONSTRAINTS_ID, ST_CONSTRAINTS_NAME).inc();
        }

        // Insert the indexes into `st_indexes`
        let (st_indexes, blob_store) = self.get_table_and_blob_store_or_create(ST_INDEXES_ID, &schemas[ST_INDEXES_IDX]);
        for (i, index) in ref_schemas
            .iter()
            .flat_map(|x| &x.indexes)
            .sorted_by_key(|x| (x.table_id, &x.columns))
            .cloned()
            .enumerate()
        {
            let row = StIndexRow {
                index_id: i.into(),
                table_id: index.table_id,
                index_type: index.index_type,
                columns: index.columns,
                index_name: index.index_name,
                is_unique: index.is_unique,
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_INDEXES.
            // If the row is already there, no-op.
            ignore_duplicate_insert_error(st_indexes.insert(blob_store, &row))?;
            // Increment row count for st_indexes.
            with_label_values(ST_INDEXES_ID, ST_INDEXES_NAME).inc();
        }

        // We don't add the row here but with `MutProgrammable::set_program_hash`, but we need to register the table
        // in the internal state.
        self.create_table(ST_MODULE_ID, schemas[ST_MODULE_IDX].clone());

        self.create_table(ST_CLIENTS_ID, schemas[ST_CLIENT_IDX].clone());

        self.create_table(ST_VAR_ID, schemas[ST_VAR_IDX].clone());

        self.create_table(ST_SCHEDULED_ID, schemas[ST_SCHEDULED_IDX].clone());
        // Insert the sequences into `st_sequences`
        let (st_sequences, blob_store) =
            self.get_table_and_blob_store_or_create(ST_SEQUENCES_ID, &schemas[ST_SEQUENCES_IDX]);
        // We create sequences last to get right the starting number
        // so, we don't sort here
        for (i, col) in ref_schemas.iter().flat_map(|x| &x.sequences).enumerate() {
            let row = StSequenceRow {
                sequence_id: i.into(),
                sequence_name: col.sequence_name.clone(),
                table_id: col.table_id,
                col_pos: col.col_pos,
                increment: col.increment,
                min_value: col.min_value,
                max_value: col.max_value,
                // All sequences for system tables start from the reserved
                // range + 1.
                // Logically, we thus have used up the default pre-allocation
                // and must initialize the sequence with twice the amount.
                start: ST_RESERVED_SEQUENCE_RANGE as i128 + 1,
                allocated: (ST_RESERVED_SEQUENCE_RANGE * 2) as i128,
            };
            let row = ProductValue::from(row);
            // Insert the meta-row into the in-memory ST_SEQUENCES.
            // If the row is already there, no-op.
            ignore_duplicate_insert_error(st_sequences.insert(blob_store, &row))?;
            // Increment row count for st_sequences
            with_label_values(ST_SEQUENCES_ID, ST_SEQUENCES_NAME).inc();
        }

        self.reset_system_table_schemas(database_address)?;

        Ok(())
    }

    /// Compute the system table schemas from the system tables,
    /// and store those schemas in the in-memory [`Table`] structures.
    ///
    /// Necessary during bootstrap because system tables include autoinc IDs
    /// for objects like indexes and constraints
    /// which are computed at insert-time,
    /// and therefore not included in the hardcoded schemas.
    pub(super) fn reset_system_table_schemas(&mut self, database_address: Address) -> Result<()> {
        // Re-read the schema with the correct ids...
        let ctx = ExecutionContext::internal(database_address);
        for schema in system_tables() {
            self.tables.get_mut(&schema.table_id).unwrap().schema =
                Arc::new(self.schema_for_table_raw(&ctx, schema.table_id)?);
        }

        Ok(())
    }

    pub fn replay_delete_by_rel(&mut self, table_id: TableId, rel: &ProductValue) -> Result<()> {
        let table = self
            .tables
            .get_mut(&table_id)
            .ok_or_else(|| TableError::IdNotFoundState(table_id))?;
        let blob_store = &mut self.blob_store;
        let skip_index_update = true;
        table
            .delete_equal_row(blob_store, rel, skip_index_update)
            .map_err(TableError::Insert)?
            .ok_or_else(|| anyhow!("Delete for non-existent row when replaying transaction"))?;
        Ok(())
    }

    pub fn replay_insert(&mut self, table_id: TableId, schema: &Arc<TableSchema>, row: &ProductValue) -> Result<()> {
        let (table, blob_store) = self.get_table_and_blob_store_or_create(table_id, schema);
        table.insert_internal(blob_store, row).map_err(TableError::Insert)?;
        Ok(())
    }

    pub(super) fn build_sequence_state(&mut self, sequence_state: &mut SequencesState) -> Result<()> {
        let st_sequences = self.tables.get(&ST_SEQUENCES_ID).unwrap();
        for row_ref in st_sequences.scan_rows(&self.blob_store) {
            let sequence = StSequenceRow::try_from(row_ref)?;
            // TODO: The system tables have initialized their value already, but this is wrong:
            // If we exceed  `SEQUENCE_PREALLOCATION_AMOUNT` we will get a unique violation
            let is_system_table = self
                .tables
                .get(&sequence.table_id)
                .map_or(false, |x| x.get_schema().table_type == StTableType::System);

            let mut seq = Sequence::new(sequence.into());
            // Now we need to recover the last allocation value.
            if !is_system_table && seq.value < seq.allocated() + 1 {
                seq.value = seq.allocated() + 1;
            }

            sequence_state.insert(seq.id(), seq);
        }
        Ok(())
    }

    pub(super) fn build_indexes(&mut self) -> Result<()> {
        let st_indexes = self.tables.get(&ST_INDEXES_ID).unwrap();
        let rows = st_indexes
            .scan_rows(&self.blob_store)
            .map(StIndexRow::try_from)
            .collect::<Result<Vec<_>>>()?;
        for index_row in rows {
            let Some((table, blob_store)) = self.get_table_and_blob_store(index_row.table_id) else {
                panic!("Cannot create index for table which doesn't exist in committed state");
            };
            let index = table.new_index(index_row.index_id, &index_row.columns, index_row.is_unique)?;
            table.insert_index(blob_store, index_row.columns, index);
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
            schemas.push(self.schema_for_table_raw(&ExecutionContext::default(), table_id)?);
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
            .get_table(ST_TABLES_ID)
            .unwrap()
            .scan_rows(&self.blob_store)
            .map(|r| r.read_col(StTableFields::TableId).unwrap())
            .filter(|table_id| self.get_table(*table_id).is_none())
            .collect::<Vec<_>>();

        // Construct their schemas and insert tables for them.
        for table_id in table_ids {
            let schema = self.schema_for_table(&ExecutionContext::default(), table_id)?;
            self.tables.insert(table_id, Self::make_table(schema));
        }
        Ok(())
    }

    pub(super) fn index_seek<'a>(
        &'a self,
        table_id: TableId,
        cols: &ColList,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<IndexScanIter<'a>> {
        self.tables.get(&table_id)?.index_seek(&self.blob_store, cols, range)
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
        tx_data.inserts().any(|(_, inserted_rows)| !inserted_rows.is_empty())
            || tx_data.deletes().any(|(_, deleted_rows)| !deleted_rows.is_empty())
            || matches!(
                ctx.reducer_context().map(|rcx| rcx.name.strip_prefix("__identity_")),
                Some(Some("connected__" | "disconnected__"))
            )
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
            if let Some((table, blob_store)) = self.get_table_and_blob_store(table_id) {
                let mut deletes = Vec::with_capacity(row_ptrs.len());

                // Note: we maintain the invariant that the delete_tables
                // holds only committed rows which should be deleted,
                // i.e. `RowPointer`s with `SquashedOffset::COMMITTED_STATE`,
                // so no need to check before applying the deletes.
                for row_ptr in row_ptrs.iter().copied() {
                    debug_assert!(row_ptr.squashed_offset().is_committed_state());

                    // TODO: re-write `TxData` to remove `ProductValue`s
                    let pv = table
                        .delete(blob_store, row_ptr, |row| row.to_product_value())
                        .expect("Delete for non-existent row!");
                    deletes.push(pv);
                }

                let table_name = &*table.get_schema().table_name;

                if !deletes.is_empty() {
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
            let (commit_table, commit_blob_store) =
                self.get_table_and_blob_store_or_create(table_id, tx_table.get_schema());

            // TODO(perf): Allocate with capacity?
            let mut inserts = vec![];
            // For each newly-inserted row, insert it into the committed state.
            for row_ref in tx_table.scan_rows(&tx_blob_store) {
                let pv = row_ref.to_product_value();
                commit_table
                    .insert(commit_blob_store, &pv)
                    .expect("Failed to insert when merging commit");

                inserts.push(pv);
            }

            let table_name = &*commit_table.get_schema().table_name;

            if !inserts.is_empty() {
                tx_data.set_inserts_for_table(table_id, table_name, inserts.into());
            }

            // Add all newly created indexes to the committed state.
            for (cols, mut index) in tx_table.indexes {
                if !commit_table.indexes.contains_key(&cols) {
                    index.clear();
                    commit_table.insert_index(commit_blob_store, cols, index);
                }
            }

            // The schema may have been modified in the transaction.
            // Update this last to placate borrowck and avoid a clone.
            // None of the above operations will inspect the schema.
            commit_table.schema = tx_table.schema;
        }
    }

    pub(super) fn get_table(&self, table_id: TableId) -> Option<&Table> {
        self.tables.get(&table_id)
    }

    pub(super) fn get_table_mut(&mut self, table_id: TableId) -> Option<&mut Table> {
        self.tables.get_mut(&table_id)
    }

    pub fn get_table_and_blob_store_immutable(&self, table_id: TableId) -> Option<(&Table, &dyn BlobStore)> {
        self.tables
            .get(&table_id)
            .map(|tbl| (tbl, &self.blob_store as &dyn BlobStore))
    }

    pub(super) fn get_table_and_blob_store(&mut self, table_id: TableId) -> Option<(&mut Table, &mut dyn BlobStore)> {
        self.tables
            .get_mut(&table_id)
            .map(|tbl| (tbl, &mut self.blob_store as &mut dyn BlobStore))
    }

    fn make_table(schema: Arc<TableSchema>) -> Table {
        Table::new(schema, SquashedOffset::COMMITTED_STATE)
    }

    fn create_table(&mut self, table_id: TableId, schema: Arc<TableSchema>) {
        self.tables.insert(table_id, Self::make_table(schema));
    }

    pub fn get_table_and_blob_store_or_create<'this>(
        &'this mut self,
        table_id: TableId,
        schema: &Arc<TableSchema>,
    ) -> (&'this mut Table, &'this mut dyn BlobStore) {
        let table = self
            .tables
            .entry(table_id)
            .or_insert_with(|| Self::make_table(schema.clone()));
        let blob_store = &mut self.blob_store;
        (table, blob_store)
    }
}

pub struct CommittedIndexIter<'a> {
    #[allow(dead_code)]
    ctx: &'a ExecutionContext,
    table_id: TableId,
    tx_state: Option<&'a TxState>,
    #[allow(dead_code)]
    committed_state: &'a CommittedState,
    committed_rows: IndexScanIter<'a>,
    num_committed_rows_fetched: u64,
}

impl<'a> CommittedIndexIter<'a> {
    pub(super) fn new(
        ctx: &'a ExecutionContext,
        table_id: TableId,
        tx_state: Option<&'a TxState>,
        committed_state: &'a CommittedState,
        committed_rows: IndexScanIter<'a>,
    ) -> Self {
        Self {
            ctx,
            table_id,
            tx_state,
            committed_state,
            committed_rows,
            num_committed_rows_fetched: 0,
        }
    }
}

// TODO(shub): this runs parralely for subscriptions leading to lock contention.
// commenting until we find a way to batch them without lock.
// impl Drop for CommittedIndexIter<'_> {
//     fn drop(&mut self) {
//         let mut metrics = self.ctx.metrics.write();
//         let get_table_name = || {
//             self.committed_state
//                 .get_schema(&self.table_id)
//                 .map(|table| &*table.table_name)
//                 .unwrap_or_default()
//                 .to_string()
//         };

//         metrics.inc_by(self.table_id, MetricType::IndexSeeks, 1, get_table_name);
//         // Increment number of index keys scanned
//         metrics.inc_by(
//             self.table_id,
//             MetricType::KeysScanned,
//             self.committed_rows.num_pointers_yielded(),
//             get_table_name,
//         );
//         // Increment number of rows fetched
//         metrics.inc_by(
//             self.table_id,
//             MetricType::RowsFetched,
//             self.num_committed_rows_fetched,
//             get_table_name,
//         );
//     }
// }

impl<'a> Iterator for CommittedIndexIter<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row_ref) = self.committed_rows.find(|row_ref| {
            !self
                .tx_state
                .map(|tx_state| tx_state.is_deleted(self.table_id, row_ref.pointer()))
                .unwrap_or(false)
        }) {
            // TODO(metrics): This doesn't actually fetch a row.
            // Move this counter to `RowRef::read_row`.
            self.num_committed_rows_fetched += 1;
            return Some(row_ref);
        }

        None
    }
}
