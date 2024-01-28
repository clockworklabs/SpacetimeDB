use super::{
    datastore::Result,
    sequence::{Sequence, SequencesState},
    state_view::{Iter, IterByColRange, ScanIterByColRange, StateView},
    tx_state::TxState,
};
use crate::{
    address::Address,
    db::datastore::{
        system_tables::{
            st_columns_schema, st_constraints_schema, st_indexes_schema, st_module_schema, st_sequences_schema,
            st_table_schema, system_tables, StColumnRow, StConstraintRow, StIndexRow, StSequenceRow, StTableRow,
            SystemTable, ST_COLUMNS_ID, ST_CONSTRAINTS_ID, ST_INDEXES_ID, ST_MODULE_ID, ST_SEQUENCES_ID, ST_TABLES_ID,
        },
        traits::{TxData, TxOp, TxRecord},
    },
    error::{DBError, TableError},
    execution_context::ExecutionContext,
};
use anyhow::anyhow;
use itertools::Itertools;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::{
    bsatn,
    db::{
        auth::{StAccess, StTableType},
        def::TableSchema,
    },
    AlgebraicValue, DataKey, ProductValue, ToDataKey,
};
use spacetimedb_table::{
    blob_store::{BlobStore, HashMapBlobStore},
    btree_index::BTreeIndex,
    indexes::{RowPointer, SquashedOffset},
    table::{IndexScanIter, InsertError, RowRef, Table, TableScanIter},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::RangeBounds,
    sync::Arc,
};

pub struct CommittedState {
    pub(crate) tables: HashMap<TableId, Table>,
    pub(crate) blob_store: HashMapBlobStore,
}

fn ignore_duplicate_insert<T>(res: std::result::Result<T, InsertError>) -> Result<()> {
    match res {
        Ok(_) => Ok(()),
        Err(InsertError::Duplicate(_)) => Ok(()),
        // TODO(error-handling): impl From<InsertError> for DBError.
        Err(err) => Err(DBError::Other(anyhow!(err))),
    }
}

impl Default for CommittedState {
    fn default() -> Self {
        Self::new()
    }
}

impl CommittedState {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            blob_store: HashMapBlobStore::default(),
        }
    }

    pub fn bootstrap_system_tables(&mut self, _database_address: Address) -> Result<()> {
        let mut sequences_start: HashMap<TableId, i128> = HashMap::with_capacity(10);

        // Insert the table row into st_tables, creating st_tables if it's missing
        self.with_table_and_blob_store_or_create(ST_TABLES_ID, st_table_schema(), |st_tables, blob_store| {
            // Insert the table row into `st_tables` for all system tables
            for schema in system_tables() {
                let table_id = schema.table_id;
                // Reset the row count metric for this system table
                // METRICS
                //     .rdb_num_table_rows
                //     .with_label_values(&database_address, &table_id.0)
                //     .set(0);

                let row = StTableRow {
                    table_id,
                    table_name: schema.table_name,
                    table_type: StTableType::System,
                    table_access: StAccess::Public,
                };
                let row = ProductValue::from(row);
                ignore_duplicate_insert(st_tables.insert(blob_store, &row))?;

                *sequences_start.entry(ST_TABLES_ID).or_default() += 1;
            }
            Ok::<(), DBError>(())
        })?;

        // Insert the columns into `st_columns`
        self.with_table_and_blob_store_or_create(ST_COLUMNS_ID, st_columns_schema(), |st_columns, blob_store| {
            for col in system_tables().into_iter().flat_map(|x| x.columns().to_vec()) {
                let row = StColumnRow {
                    table_id: col.table_id,
                    col_pos: col.col_pos,
                    col_name: col.col_name.clone(),
                    col_type: col.col_type.clone(),
                };
                let row = ProductValue::from(row);
                ignore_duplicate_insert(st_columns.insert(blob_store, &row))?;
                // Increment row count for st_columns
                // METRICS
                //     .rdb_num_table_rows
                //     .with_label_values(&database_address, &ST_COLUMNS_ID.into())
                //     .inc();
            }
            Ok::<(), DBError>(())
        })?;

        // Insert the FK sorted by table/column so it show together when queried.

        // Insert constraints into `st_constraints`
        self.with_table_and_blob_store_or_create(
            ST_CONSTRAINTS_ID,
            st_constraints_schema(),
            |st_constraints, blob_store| {
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
                    ignore_duplicate_insert(st_constraints.insert(blob_store, &row))?;
                    // Increment row count for st_constraints
                    // METRICS
                    //     .rdb_num_table_rows
                    //     .with_label_values(&database_address, &ST_CONSTRAINTS_ID.into())
                    //     .inc();

                    *sequences_start.entry(ST_CONSTRAINTS_ID).or_default() += 1;
                }
                Ok::<(), DBError>(())
            },
        )?;

        // Insert the indexes into `st_indexes`
        self.with_table_and_blob_store_or_create(ST_INDEXES_ID, st_indexes_schema(), |st_indexes, blob_store| {
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
                ignore_duplicate_insert(st_indexes.insert(blob_store, &row))?;
                // Increment row count for st_indexes
                // METRICS
                //     .rdb_num_table_rows
                //     .with_label_values(&database_address, &ST_INDEXES_ID.into())
                //     .inc();

                *sequences_start.entry(ST_INDEXES_ID).or_default() += 1;
            }
            Ok::<(), DBError>(())
        })?;

        // We don't add the row here but with `MutProgrammable::set_program_hash`, but we need to register the table
        // in the internal state.
        self.create_table(ST_MODULE_ID, st_module_schema());

        self.with_table_and_blob_store_or_create(
            ST_SEQUENCES_ID,
            st_sequences_schema(),
            |st_sequences, blob_store| {
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
                    ignore_duplicate_insert(st_sequences.insert(blob_store, &row))?;
                    // Increment row count for st_sequences
                    // METRICS
                    //     .rdb_num_table_rows
                    //     .with_label_values(&database_address, &ST_SEQUENCES_ID.into())
                    //     .inc();
                }
                Ok::<(), DBError>(())
            },
        )?;

        Ok(())
    }

    pub fn replay_delete_by_rel(&mut self, table_id: TableId, rel: &ProductValue) -> Result<()> {
        let table = self
            .tables
            .get_mut(&table_id)
            .ok_or_else(|| TableError::IdNotFoundState(table_id))?;
        let blob_store = &mut self.blob_store;
        table
            .delete_equal_row(blob_store, rel)
            .map_err(|e| anyhow!("Table::delete_equal_row failed: {:?}", e))?
            .ok_or_else(|| anyhow!("Delete for non-existent row when replaying transaction"))?;
        Ok(())
    }

    pub fn build_sequence_state(&mut self, sequence_state: &mut SequencesState) -> Result<()> {
        let st_sequences = self.tables.get(&ST_SEQUENCES_ID).unwrap();
        for row_ref in st_sequences.scan_rows(&self.blob_store) {
            let row = row_ref.to_product_value();
            let sequence = StSequenceRow::try_from(&row)?;
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

            sequence_state.insert(sequence.sequence_id, seq);
        }
        Ok(())
    }

    pub fn build_indexes(&mut self) -> Result<()> {
        let st_indexes = self.tables.get(&ST_INDEXES_ID).unwrap();
        let rows = st_indexes
            .scan_rows(&self.blob_store)
            .map(|r| r.to_product_value())
            .collect::<Vec<_>>();
        for row in rows {
            let index_row = StIndexRow::try_from(&row)?;
            self.with_table_and_blob_store(index_row.table_id, |table, blob_store| {
                let mut index = BTreeIndex::new(index_row.index_id, table.get_row_type(), &index_row.columns, index_row.is_unique, index_row.index_name);
                index.build_from_rows(&index_row.columns, table.scan_rows(blob_store))?;
                table.indexes.insert(index_row.columns, index);
                Ok::<(), DBError>(())
            })
            .unwrap()?;
        }
        Ok(())
    }

    /// After replaying all old transactions, tables which have rows will
    /// have been created in memory, but tables with no rows will not have
    /// been created. This function ensures that they are created.
    pub fn build_missing_tables(&mut self) -> Result<()> {
        let st_tables = self.get_table(ST_TABLES_ID).unwrap();
        let rows = st_tables
            .scan_rows(&self.blob_store)
            .map(|r| r.to_product_value())
            .collect::<Vec<_>>();
        for row in rows {
            let table_row = StTableRow::try_from(&row)?;
            let table_id = table_row.table_id;
            if self.get_table(table_id).is_none() {
                let schema = self
                    .schema_for_table(&ExecutionContext::default(), table_id)?
                    .into_owned();
                self.tables
                    .insert(table_id, Table::new(schema, SquashedOffset::COMMITTED_STATE));
            }
        }
        Ok(())
    }

    pub fn index_seek<'a>(
        &'a self,
        table_id: TableId,
        cols: &ColList,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<IndexScanIter<'a>> {
        if let Some(table) = self.tables.get(&table_id) {
            table.index_seek(&self.blob_store, cols, range)
        } else {
            None
        }
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
    pub fn get(&self, table_id: TableId, row_ptr: RowPointer) -> RowRef<'_> {
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

    pub fn merge(&mut self, tx_state: TxState) -> TxData {
        // TODO(perf): pre-allocate `Vec::with_capacity`?
        let mut tx_data = TxData { records: vec![] };

        // First, apply deletes. This will free up space in the committed tables.
        self.merge_apply_deletes(&mut tx_data, tx_state.delete_tables);

        // Then, apply inserts. This will re-fill the holes freed by deletions
        // before allocating new pages.

        self.merge_apply_inserts(&mut tx_data, tx_state.insert_tables, tx_state.blob_store);

        tx_data
    }

    fn merge_apply_deletes(&mut self, tx_data: &mut TxData, delete_tables: BTreeMap<TableId, BTreeSet<RowPointer>>) {
        for (table_id, row_ptrs) in delete_tables {
            if self
                .with_table_and_blob_store(table_id, |table, blob_store| {
                    for row_ptr in row_ptrs.iter().copied() {
                        debug_assert!(row_ptr.squashed_offset().is_committed_state());

                        // TODO: re-write `TxRecord` to remove `product_value`, or at least `key`.
                        let pv = table.delete(blob_store, row_ptr).expect("Delete for non-existent row!");
                        let data_key = pv.to_data_key();
                        tx_data.records.push(TxRecord {
                            op: TxOp::Delete,
                            table_name: table.schema.table_name.clone(),
                            table_id,
                            key: data_key,
                            product_value: pv,
                        });
                    }
                })
                .is_none()
                && !row_ptrs.is_empty()
            {
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
        let mut bytes = Vec::with_capacity(1024);

        for (table_id, mut tx_table) in insert_tables {
            self.with_table_and_blob_store_or_create(
                table_id,
                *tx_table.schema.clone(),
                |commit_table, commit_blob_store| {
                    if commit_table.schema != tx_table.schema {
                        todo!("Determine how to handle a modified schema")
                    }

                    for row_ref in tx_table.scan_rows(&tx_blob_store) {
                        let pv = row_ref.to_product_value();
                        // commit_table
                        //     .insert(commit_blob_store, &pv)
                        //     .expect("Failed to insert when merging commit");
                        bsatn::to_writer(&mut bytes, &pv).expect("Failed to BSATN-serialize ProductValue");
                        // let bytes = bsatn::to_vec(&pv).expect("Failed to BSATN-serialize ProductValue");
                        let data_key = DataKey::from_data(&bytes);
                        // tx_data.records.push(TxRecord {
                        //     op: TxOp::Insert(Arc::from(bytes.clone().into_boxed_slice())),
                        //     product_value: pv,
                        //     key: data_key,
                        //     table_name: commit_table.schema.table_name.clone(),
                        //     table_id,
                        // });
                        bytes.clear();
                    }

                    for (cols, mut index) in std::mem::take(&mut tx_table.indexes) {
                        if !commit_table.indexes.contains_key(&cols) {
                            index.clear();
                            commit_table.insert_index(commit_blob_store, cols, index);
                        }
                    }
                },
            );
        }
    }

    pub fn get_table(&self, table_id: TableId) -> Option<&Table> {
        self.tables.get(&table_id)
    }

    pub fn get_table_mut(&mut self, table_id: TableId) -> Option<&mut Table> {
        self.tables.get_mut(&table_id)
    }

    pub fn with_table_and_blob_store<Res>(
        &mut self,
        table_id: TableId,
        f: impl FnOnce(&mut Table, &mut dyn BlobStore) -> Res,
    ) -> Option<Res> {
        let table = self.tables.get_mut(&table_id)?;
        let blob_store = &mut self.blob_store;
        Some(f(table, blob_store))
    }

    pub fn get_table_and_blob_store(&mut self, table_id: TableId) -> Option<(&mut Table, &mut dyn BlobStore)> {
        self.tables
            .get_mut(&table_id)
            .map(|tbl| (tbl, &mut self.blob_store as &mut dyn BlobStore))
    }

    fn create_table(&mut self, table_id: TableId, schema: TableSchema) {
        self.tables
            .insert(table_id, Table::new(schema, SquashedOffset::COMMITTED_STATE));
    }

    pub fn with_table_and_blob_store_or_create_ref_schema<Res>(
        &mut self,
        table_id: TableId,
        schema: &TableSchema,
        f: impl FnOnce(&mut Table, &mut dyn BlobStore) -> Res,
    ) -> Res {
        let table = self
            .tables
            .entry(table_id)
            .or_insert_with(|| Table::new(schema.clone(), SquashedOffset::COMMITTED_STATE));
        let blob_store = &mut self.blob_store;
        f(table, blob_store)
    }

    pub fn with_table_and_blob_store_or_create<Res>(
        &mut self,
        table_id: TableId,
        schema: TableSchema,
        f: impl FnOnce(&mut Table, &mut dyn BlobStore) -> Res,
    ) -> Res {
        let table = self
            .tables
            .entry(table_id)
            .or_insert_with(|| Table::new(schema, SquashedOffset::COMMITTED_STATE));
        let blob_store = &mut self.blob_store;
        f(table, blob_store)
    }

    pub fn iter_by_col_range_maybe_index<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: ColList,
        range: R,
    ) -> Result<IterByColRange<'a, R>> {
        match self.index_seek(*table_id, &cols, &range) {
            Some(committed_rows) => Ok(IterByColRange::CommittedIndex(CommittedIndexIter::new(
                ctx,
                *table_id,
                None,
                self,
                committed_rows,
            ))),
            None => self.iter_by_col_range(ctx, table_id, cols, range),
        }
    }
}

impl StateView for CommittedState {
    fn get_schema(&self, table_id: &TableId) -> Option<&TableSchema> {
        self.tables.get(table_id).map(|table| table.get_schema())
    }
    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: &TableId) -> Result<Iter<'a>> {
        if let Some(table_name) = self.table_exists(table_id) {
            return Ok(Iter::new(ctx, *table_id, table_name, None, self));
        }
        Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into())
    }
    fn table_exists(&self, table_id: &TableId) -> Option<&str> {
        if let Some(table) = self.tables.get(table_id) {
            Some(&table.schema.table_name)
        } else {
            None
        }
    }
    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
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

struct CommittedStateIter<'a> {
    iter: TableScanIter<'a>,
    table_id_col: &'a ColList,
    value: &'a AlgebraicValue,
}

impl<'a> Iterator for CommittedStateIter<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        for row_ref in &mut self.iter {
            let row = row_ref.to_product_value();
            let table_id = row.project_not_empty(self.table_id_col).unwrap();
            if table_id == *self.value {
                return Some(row_ref);
            }
        }

        None
    }
}

#[allow(unused)]
pub struct CommittedIndexIter<'a> {
    ctx: &'a ExecutionContext<'a>,
    table_id: TableId,
    tx_state: Option<&'a TxState>,
    committed_state: &'a CommittedState,
    committed_rows: IndexScanIter<'a>,
    num_committed_rows_fetched: u64,
}

impl<'a> CommittedIndexIter<'a> {
    pub fn new(
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

impl Drop for CommittedIndexIter<'_> {
    fn drop(&mut self) {}
    // {
    //     DB_METRICS
    //         .rdb_num_index_seeks
    //         .with_label_values(
    //             &self.ctx.workload(),
    //             &self.ctx.database(),
    //             self.ctx.reducer_or_query(),
    //             &self.table_id.0,
    //         )
    //         .inc();

    //     // Increment number of index keys scanned
    //     DB_METRICS
    //         .rdb_num_keys_scanned
    //         .with_label_values(
    //             &self.ctx.workload(),
    //             &self.ctx.database(),
    //             self.ctx.reducer_or_query(),
    //             &self.table_id.0,
    //         )
    //         .inc_by(self.committed_rows.keys_scanned());

    //     // Increment number of rows fetched
    //     DB_METRICS
    //         .rdb_num_rows_fetched
    //         .with_label_values(
    //             &self.ctx.workload(),
    //             &self.ctx.database(),
    //             self.ctx.reducer_or_query(),
    //             &self.table_id.0,
    //         )
    //         .inc_by(self.num_committed_rows_fetched);
    // }
}

impl<'a> Iterator for CommittedIndexIter<'a> {
    type Item = RowRef<'a>;

    // #[tracing::instrument(skip_all)]
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
