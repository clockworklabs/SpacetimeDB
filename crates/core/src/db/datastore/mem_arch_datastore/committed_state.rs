use super::{
    blob_store::{BlobStore, HashMapBlobStore},
    btree_index::BTreeIndex,
    indexes::{RowPointer, SquashedOffset},
    sequence::{Sequence, SequencesState},
    table::{IndexScanIter, InsertError, RowRef, Table, TableScanIter},
    tx_state::TxState,
};
use crate::{
    address::Address,
    db::datastore::{
        system_tables::{
            st_columns_schema, st_constraints_schema, st_indexes_schema, st_module_schema, st_sequences_schema,
            st_table_schema, system_tables, StColumnFields, StColumnRow, StConstraintFields, StConstraintRow,
            StIndexFields, StIndexRow, StSequenceFields, StSequenceRow, StTableFields, StTableRow, SystemTable,
            ST_COLUMNS_ID, ST_CONSTRAINTS_ID, ST_INDEXES_ID, ST_MODULE_ID, ST_SEQUENCES_ID, ST_TABLES_ID,
        },
        traits::{TxData, TxOp, TxRecord},
    },
    error::{DBError, TableError},
};
use anyhow::anyhow;
use itertools::Itertools;
use nonempty::NonEmpty;
use spacetimedb_lib::metrics::METRICS;
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_sats::{
    bsatn,
    db::{
        auth::{StAccess, StTableType},
        def::{ColumnSchema, ConstraintSchema, IndexSchema, SequenceSchema, TableSchema},
    },
    AlgebraicValue, DataKey, ProductValue, ToDataKey,
};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::RangeBounds,
    sync::Arc,
};

pub struct CommittedState {
    pub(crate) tables: HashMap<TableId, Table>,
    pub(crate) blob_store: HashMapBlobStore,
}

fn ignore_duplicate_insert<T>(res: Result<T, InsertError>) -> Result<(), DBError> {
    match res {
        Ok(_) => Ok(()),
        Err(InsertError::Duplicate(_)) => Ok(()),
        // TODO(error-handling): impl From<InsertError> for DBError.
        Err(err) => Err(err.into()),
    }
}

impl CommittedState {
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            blob_store: HashMapBlobStore::default(),
        }
    }

    pub fn bootstrap_system_tables(&mut self, database_address: Address) -> Result<(), DBError> {
        let mut sequences_start: HashMap<TableId, i128> = HashMap::with_capacity(10);

        // Insert the table row into st_tables, creating st_tables if it's missing
        self.with_table_and_blob_store_or_create(ST_TABLES_ID, st_table_schema(), |st_tables, blob_store| {
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
                ignore_duplicate_insert(st_tables.insert(blob_store, row))?;

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
                ignore_duplicate_insert(st_columns.insert(blob_store, row))?;
                // Increment row count for st_columns
                METRICS
                    .rdb_num_table_rows
                    .with_label_values(&database_address, &ST_COLUMNS_ID.into())
                    .inc();
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
                    ignore_duplicate_insert(st_constraints.insert(blob_store, row))?;
                    // Increment row count for st_constraints
                    METRICS
                        .rdb_num_table_rows
                        .with_label_values(&database_address, &ST_CONSTRAINTS_ID.into())
                        .inc();

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
                ignore_duplicate_insert(st_indexes.insert(blob_store, row))?;
                // Increment row count for st_indexes
                METRICS
                    .rdb_num_table_rows
                    .with_label_values(&database_address, &ST_INDEXES_ID.into())
                    .inc();

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
                    ignore_duplicate_insert(st_sequences.insert(blob_store, row))?;
                    // Increment row count for st_sequences
                    METRICS
                        .rdb_num_table_rows
                        .with_label_values(&database_address, &ST_SEQUENCES_ID.into())
                        .inc();
                }
                Ok::<(), DBError>(())
            },
        )?;

        Ok(())
    }

    pub fn replay_delete_by_rel(&mut self, table_id: TableId, rel: ProductValue) -> Result<(), DBError> {
        let table = self
            .tables
            .get_mut(&table_id)
            .ok_or_else(|| TableError::IdNotFoundState(table_id))?;
        let blob_store = &mut self.blob_store;
        table
            .delete_equal_row(blob_store, rel)?
            .ok_or_else(|| anyhow!("Delete for non-existent row when replaying transaction"))?;
        Ok(())
    }

    pub fn build_sequence_state(&mut self, sequence_state: &mut SequencesState) -> Result<(), DBError> {
        let st_sequences = self.tables.get(&ST_SEQUENCES_ID).unwrap();
        for row_ref in st_sequences.scan_rows(&self.blob_store) {
            let row = row_ref.read_row();
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

            sequence_state.sequences.insert(sequence.sequence_id, seq);
        }
        Ok(())
    }

    pub fn build_indexes(&mut self) -> Result<(), DBError> {
        let st_indexes = self.tables.get(&ST_INDEXES_ID).unwrap();
        let rows = st_indexes
            .scan_rows(&self.blob_store)
            .map(|r| r.read_row())
            .collect::<Vec<_>>();
        for row in rows {
            let index_row = StIndexRow::try_from(&row)?;
            self.with_table_and_blob_store(index_row.table_id, |table, blob_store| {
                let mut index = BTreeIndex::new(
                    index_row.index_id,
                    index_row.table_id,
                    index_row.columns.clone(),
                    index_row.index_name.into(),
                    index_row.is_unique,
                );
                index.build_from_rows(table.scan_rows(blob_store))?;
                table.indexes.insert(index_row.columns, index);
                Ok::<(), DBError>(())
            })
            .unwrap()?;
        }
        Ok(())
    }

    pub fn get_schema(&self, table_id: TableId) -> Option<&TableSchema> {
        self.tables.get(&table_id).map(|t| t.get_schema())
    }

    pub fn schema_for_table(&self, table_id: TableId) -> Result<Cow<'_, TableSchema>, DBError> {
        if let Some(schema) = self.get_schema(table_id) {
            return Ok(Cow::Borrowed(schema));
        }

        // Look up the table_name for the table in question.
        let table_id_col = NonEmpty::new(StTableFields::TableId.col_id());

        // let table_id_col = NonEmpty::new(.col_id());
        let value: AlgebraicValue = table_id.into();
        let rows = self
            .iter_by_col_eq(&ST_TABLES_ID, &table_id_col, &value)?
            .collect::<Vec<_>>();
        let row_ref = rows
            .first()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let row = row_ref.read_row();
        let el = StTableRow::try_from(&row)?;
        let table_name = el.table_name.to_owned();
        let table_id = el.table_id;

        // Look up the columns for the table in question.
        let mut columns = self
            .iter_by_col_eq(&ST_COLUMNS_ID, &NonEmpty::new(StColumnFields::TableId.col_id()), &value)?
            .map(|row_ref| {
                let row = row_ref.read_row();
                let el = StColumnRow::try_from(&row)?;
                Ok(ColumnSchema {
                    table_id: el.table_id,
                    col_pos: el.col_pos,
                    col_name: el.col_name.into(),
                    col_type: el.col_type,
                })
            })
            .collect::<Result<Vec<_>, DBError>>()?;

        columns.sort_by_key(|col| col.col_pos);

        // Look up the constraints for the table in question.
        let mut constraints = Vec::new();
        for row_ref in self.iter_by_col_eq(
            &ST_CONSTRAINTS_ID,
            &NonEmpty::new(StConstraintFields::TableId.col_id()),
            &table_id.into(),
        )? {
            let row = row_ref.read_row();

            let el = StConstraintRow::try_from(&row)?;
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
        for row_ref in self.iter_by_col_eq(
            &ST_SEQUENCES_ID,
            &NonEmpty::new(StSequenceFields::TableId.col_id()),
            &AlgebraicValue::U32(table_id.into()),
        )? {
            let row = row_ref.read_row();

            let el = StSequenceRow::try_from(&row)?;
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
        for row_ref in self.iter_by_col_eq(
            &ST_INDEXES_ID,
            &NonEmpty::new(StIndexFields::TableId.col_id()),
            &table_id.into(),
        )? {
            let row = row_ref.read_row();

            let el = StIndexRow::try_from(&row)?;
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

    // TODO(shubham): Need to confirm, if indexes exist during bootstrap to be used here.
    /// Iter for`CommittedState`, Only to be used during bootstrap.
    /// For transaction, consider using MutTxId::Iters.
    fn iter_by_col_eq<'a>(
        &'a self,
        table_id: &'a TableId,
        table_id_col: &'a NonEmpty<ColId>,
        value: &'a AlgebraicValue,
    ) -> Result<CommittedStateIter<'a>, TableError> {
        let table = self
            .tables
            .get(table_id)
            .ok_or(TableError::IdNotFoundState(*table_id))?;

        Ok(CommittedStateIter {
            iter: table.scan_rows(&self.blob_store),
            table_id_col,
            value,
        })
    }

    /// After replaying all old transactions, tables which have rows will
    /// have been created in memory, but tables with no rows will not have
    /// been created. This function ensures that they are created.
    pub fn build_missing_tables(&mut self) -> Result<(), DBError> {
        let st_tables = self.get_table(ST_TABLES_ID).unwrap();
        let rows = st_tables
            .scan_rows(&self.blob_store)
            .map(|r| r.read_row())
            .collect::<Vec<_>>();
        for row in rows {
            let table_row = StTableRow::try_from(&row)?;
            let table_id = table_row.table_id;
            if self.get_table(table_id).is_none() {
                let schema = self.schema_for_table(table_id)?.into_owned();
                self.tables
                    .insert(table_id, Table::new(schema, SquashedOffset::COMMITTED_STATE));
            }
        }
        Ok(())
    }

    pub fn index_seek<'a>(
        &'a self,
        table_id: TableId,
        cols: &NonEmpty<ColId>,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<IndexScanIter<'a>> {
        if let Some(table) = self.tables.get(&table_id) {
            table.index_seek(&self.blob_store, cols, range)
        } else {
            None
        }
    }

    // TODO(perf, deep-integration):
    //   When [`Table::read_row`] and [`RowRef::new`] become `unsafe`,
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
        // TODO(perf, deep-integration):
        // See above. Once `RowRef::new` is unsafe, justify with:
        //
        // Our invariants satisfy `RowRef::new`.
        RowRef::new(table, &self.blob_store, row_ptr)
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

        for (table_id, mut tx_table) in insert_tables {
            self.with_table_and_blob_store_or_create(
                table_id,
                *tx_table.schema.clone(),
                |commit_table, commit_blob_store| {
                    if commit_table.schema != tx_table.schema {
                        todo!("Determine how to handle a modified schema")
                    }

                    for row_ref in tx_table.scan_rows(&tx_blob_store) {
                        let pv = row_ref.read_row();
                        commit_table
                            .insert(commit_blob_store, pv.clone())
                            .expect("Failed to insert when merging commit");
                        let bytes = bsatn::to_vec(&pv).expect("Failed to BSATN-serialize ProductValue");
                        let data_key = DataKey::from_data(&bytes);
                        tx_data.records.push(TxRecord {
                            op: TxOp::Insert(Arc::new(bytes)),
                            product_value: pv,
                            key: data_key,
                            table_id,
                        });
                    }

                    for (cols, mut index) in std::mem::take(&mut tx_table.indexes) {
                        if !commit_table.indexes.contains_key(&cols) {
                            index.clear();
                            commit_table.insert_index(commit_blob_store, index);
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
}

struct CommittedStateIter<'a> {
    iter: TableScanIter<'a>,
    table_id_col: &'a NonEmpty<ColId>,
    value: &'a AlgebraicValue,
}

impl<'a> Iterator for CommittedStateIter<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        for row_ref in &mut self.iter {
            let row = row_ref.read_row();
            let table_id = row.project_not_empty(self.table_id_col).unwrap();
            if table_id == *self.value {
                return Some(row_ref);
            }
        }

        None
    }
}
