use crate::{
    db::{
        datastore::{
            system_tables::{
                st_columns_schema, st_constraints_schema, st_indexes_schema, st_module_schema, st_sequences_schema,
                st_table_schema, system_tables, StColumnRow, StConstraintRow, StIndexRow, StSequenceRow, StTableRow,
                SystemTable, ST_COLUMNS_ID, ST_COLUMNS_NAME, ST_CONSTRAINTS_ID, ST_CONSTRAINTS_NAME, ST_INDEXES_ID,
                ST_INDEXES_NAME, ST_MODULE_ID, ST_SEQUENCES_ID, ST_SEQUENCES_NAME, ST_TABLES_ID,
            },
            traits::{TxData, TxOp, TxRecord},
            Result,
        },
        db_metrics::DB_METRICS,
    },
    error::TableError,
    execution_context::ExecutionContext,
};

use super::{
    btree_index::{BTreeIndex, BTreeIndexRangeIter}, sequence::{Sequence, SequencesState}, table::Table, tx_state::TxState, DataRef, Iter, IterByColRange, RowId, ScanIterByColRange, StateView
};
use itertools::Itertools as _;
use spacetimedb_lib::Address;
use spacetimedb_primitives::{ColList, TableId};
use spacetimedb_sats::{
    db::{
        auth::{StAccess, StTableType},
        def::TableSchema,
    },
    AlgebraicValue, DataKey, ProductValue, ToDataKey as _,
};
use std::{
    collections::{BTreeMap, HashMap},
    ops::RangeBounds,
    sync::Arc,
};

#[derive(Default)]
pub(crate) struct CommittedState {
    pub(crate) tables: HashMap<TableId, Table>,
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

    /// Returns an iterator,
    /// yielding every row in the table identified by `table_id`,
    /// where the values of `cols` are contained in `range`.
    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: &TableId,
        cols: ColList,
        range: R,
    ) -> Result<IterByColRange<'a, R>> {
        Ok(IterByColRange::Scan(ScanIterByColRange {
            range,
            cols,
            scan_iter: self.iter(ctx, table_id)?,
        }))
    }
}

impl CommittedState {
    fn get_or_create_table(&mut self, table_id: TableId, schema: TableSchema) -> &mut Table {
        self.tables.entry(table_id).or_insert_with(|| Table::new(schema))
    }

    pub(crate) fn get_table(&mut self, table_id: &TableId) -> Option<&mut Table> {
        self.tables.get_mut(table_id)
    }

    pub(crate) fn merge(&mut self, tx_state: TxState, memory: BTreeMap<DataKey, Arc<Vec<u8>>>) -> TxData {
        let mut tx_data = TxData { records: vec![] };
        for (table_id, table) in tx_state.insert_tables {
            let commit_table = self.get_or_create_table(table_id, table.schema.clone());
            // The schema may have been modified in the transaction.
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
                    table_name: commit_table.schema.table_name.clone(),
                    key: row_id.0,
                    product_value: pv,
                }
            }));

            // Add all newly created indexes to the committed state
            for (_, index) in table.indexes {
                if !commit_table.indexes.contains_key(&index.cols) {
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
                            table_name: table.schema.table_name.clone(),
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
        cols: &ColList,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> Option<BTreeIndexRangeIter<'a>> {
        if let Some(table) = self.tables.get(table_id) {
            table.index_seek(cols, range)
        } else {
            None
        }
    }

    pub(crate) fn bootstrap_system_tables(&mut self, database_address: Address) -> Result<()> {
        let mut sequences_start: HashMap<TableId, i128> = HashMap::with_capacity(10);

        // Insert the table row into st_tables, creating st_tables if it's missing
        let st_tables = self.get_or_create_table(ST_TABLES_ID, st_table_schema());

        // Insert the table row into `st_tables` for all system tables
        for schema in system_tables() {
            let table_id = schema.table_id;
            // Reset the row count metric for this system table
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &table_id.0, &schema.table_name)
                .set(0);

            let row = StTableRow {
                table_id,
                table_name: schema.table_name,
                table_type: StTableType::System,
                table_access: StAccess::Public,
            };
            let row = ProductValue::from(row);
            let data_key = row.to_data_key();
            st_tables.rows.insert(RowId(data_key), row);

            *sequences_start.entry(ST_TABLES_ID).or_default() += 1;
        }

        // Insert the columns into `st_columns`
        let st_columns = self.get_or_create_table(ST_COLUMNS_ID, st_columns_schema());

        for col in system_tables().into_iter().flat_map(|x| x.columns().to_vec()) {
            let row = StColumnRow {
                table_id: col.table_id,
                col_pos: col.col_pos,
                col_name: col.col_name.clone(),
                col_type: col.col_type.clone(),
            };
            let row = ProductValue::from(row);
            let data_key = row.to_data_key();

            st_columns.rows.insert(RowId(data_key), row);
            // Increment row count for st_columns
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &ST_COLUMNS_ID.into(), ST_COLUMNS_NAME)
                .inc();
        }

        // Insert the FK sorted by table/column so it show together when queried.

        // Insert constraints into `st_constraints`
        let st_constraints = self.get_or_create_table(ST_CONSTRAINTS_ID, st_constraints_schema());

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
            let data_key = row.to_data_key();
            st_constraints.rows.insert(RowId(data_key), row);
            // Increment row count for st_constraints
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &ST_CONSTRAINTS_ID.into(), ST_CONSTRAINTS_NAME)
                .inc();

            *sequences_start.entry(ST_CONSTRAINTS_ID).or_default() += 1;
        }

        // Insert the indexes into `st_indexes`
        let st_indexes = self.get_or_create_table(ST_INDEXES_ID, st_indexes_schema());

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
            let data_key = row.to_data_key();
            st_indexes.rows.insert(RowId(data_key), row);
            // Increment row count for st_indexes
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &ST_INDEXES_ID.into(), ST_INDEXES_NAME)
                .inc();

            *sequences_start.entry(ST_INDEXES_ID).or_default() += 1;
        }

        // We don't add the row here but with `MutProgrammable::set_program_hash`, but we need to register the table
        // in the internal state.
        self.get_or_create_table(ST_MODULE_ID, st_module_schema());

        let st_sequences = self.get_or_create_table(ST_SEQUENCES_ID, st_sequences_schema());

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
            let data_key = row.to_data_key();
            st_sequences.rows.insert(RowId(data_key), row);
            // Increment row count for st_sequences
            DB_METRICS
                .rdb_num_table_rows
                .with_label_values(&database_address, &ST_SEQUENCES_ID.into(), ST_SEQUENCES_NAME)
                .inc();
        }

        Ok(())
    }

    pub(crate) fn build_sequence_state(&mut self, sequence_state: &mut SequencesState) -> Result<()> {
        let st_sequences = self.tables.get(&ST_SEQUENCES_ID).unwrap();
        for row in st_sequences.scan_rows() {
            let sequence = StSequenceRow::try_from(row)?;
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

    pub(crate) fn build_indexes(&mut self) -> Result<()> {
        let st_indexes = self.tables.get(&ST_INDEXES_ID).unwrap();
        let rows = st_indexes.scan_rows().cloned().collect::<Vec<_>>();
        for row in rows {
            let index_row = StIndexRow::try_from(&row)?;
            let table = self.get_table(&index_row.table_id).unwrap();
            let mut index = BTreeIndex::new(
                index_row.index_id,
                index_row.table_id,
                index_row.columns.clone(),
                index_row.index_name.into(),
                index_row.is_unique,
            );
            index.build_from_rows(table.scan_rows())?;
            table.indexes.insert(index_row.columns, index);
        }
        Ok(())
    }

    /// After replaying all old transactions, tables which have rows will
    /// have been created in memory, but tables with no rows will not have
    /// been created. This function ensures that they are created.
    pub(crate) fn build_missing_tables(&mut self) -> Result<()> {
        let st_tables = self.tables.get(&ST_TABLES_ID).unwrap();
        let rows = st_tables.scan_rows().cloned().collect::<Vec<_>>();
        for row in rows {
            let table_row = StTableRow::try_from(&row)?;
            let table_id = table_row.table_id;
            if self.get_table(&table_id).is_none() {
                let schema = self
                    .schema_for_table(&ExecutionContext::default(), table_id)?
                    .into_owned();
                self.tables.insert(table_id, Table::new(schema));
            }
        }
        Ok(())
    }

    pub(crate) fn table_rows(
        &mut self,
        table_id: TableId,
        schema: TableSchema,
    ) -> &mut indexmap::IndexMap<RowId, ProductValue> {
        &mut self.tables.entry(table_id).or_insert_with(|| Table::new(schema)).rows
    }
}

pub struct CommittedIndexIter<'a> {
    pub(crate) ctx: &'a ExecutionContext<'a>,
    pub(crate) table_id: TableId,
    pub(crate) tx_state: Option<&'a TxState>,
    pub(crate) committed_state: &'a CommittedState,
    pub(crate) committed_rows: BTreeIndexRangeIter<'a>,
    pub(crate) num_committed_rows_fetched: u64,
}

impl Drop for CommittedIndexIter<'_> {
    fn drop(&mut self) {
        let table_name = self
            .committed_state
            .get_schema(&self.table_id)
            .map(|table| table.table_name.as_str())
            .unwrap_or_default();

        DB_METRICS
            .rdb_num_index_seeks
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_name(),
                &self.table_id.0,
                table_name,
            )
            .inc();

        // Increment number of index keys scanned
        DB_METRICS
            .rdb_num_keys_scanned
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_name(),
                &self.table_id.0,
                table_name,
            )
            .inc_by(self.committed_rows.keys_scanned());

        // Increment number of rows fetched
        DB_METRICS
            .rdb_num_rows_fetched
            .with_label_values(
                &self.ctx.workload(),
                &self.ctx.database(),
                self.ctx.reducer_name(),
                &self.table_id.0,
                table_name,
            )
            .inc_by(self.num_committed_rows_fetched);
    }
}

impl<'a> Iterator for CommittedIndexIter<'a> {
    type Item = DataRef<'a>;

    #[tracing::instrument(skip_all)]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(row_id) = self.committed_rows.find(|row_id| match self.tx_state {
            Some(tx_state) => !tx_state
                .delete_tables
                .get(&self.table_id)
                .map_or(false, |table| table.contains(row_id)),
            None => true,
        }) {
            self.num_committed_rows_fetched += 1;
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
pub(crate) fn get_committed_row<'a>(state: &'a CommittedState, table_id: &TableId, row_id: &'a RowId) -> DataRef<'a> {
    DataRef::new(row_id, state.tables.get(table_id).unwrap().get_row(row_id).unwrap())
}
