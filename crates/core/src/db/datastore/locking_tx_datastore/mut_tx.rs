use super::btree_index::BTreeIndex;
use super::committed_state::{CommittedIndexIter, CommittedState};
use super::sequence::SequencesState;
use super::state_view::{IndexSeekIterMutTxId, ScanIterByColRange, StateView};
use super::table::Table;
use super::tx_state::{RowState, TxState};
use super::{DataRef, Iter, IterByColRange, RowId, SharedMutexGuard, SharedWriteGuard};
use crate::db::datastore::locking_tx_datastore::sequence::Sequence;
use crate::db::datastore::system_tables::{
    table_name_is_system, StColumnFields, StColumnRow, StConstraintFields, StConstraintRow, StIndexFields, StIndexRow,
    StSequenceFields, StSequenceRow, StTableFields, StTableRow, SystemTable, ST_COLUMNS_ID, ST_CONSTRAINTS_ID,
    ST_INDEXES_ID, ST_SEQUENCES_ID, ST_TABLES_ID,
};
use crate::db::datastore::traits::TxData;
use crate::db::datastore::Result;
use crate::error::{DBError, IndexError, SequenceError, TableError};
use crate::execution_context::ExecutionContext;
use core::ops::{Deref, RangeBounds};
use spacetimedb_lib::Address;
use spacetimedb_primitives::*;
use spacetimedb_sats::data_key::{DataKey, ToDataKey};
use spacetimedb_sats::db::def::*;
use spacetimedb_sats::db::error::SchemaErrors;
use spacetimedb_sats::{AlgebraicValue, ProductType, ProductValue};
use spacetimedb_table::table::UniqueConstraintViolation;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Represents a Mutable transaction. Holds locks for its duration
///
/// The initialization of this struct is sensitive because improper
/// handling can lead to deadlocks. Therefore, it is strongly recommended to use
/// `Locking::begin_mut_tx()` for instantiation to ensure safe acquisition of locks.
pub struct MutTxId {
    pub(crate) tx_state: TxState,
    pub(crate) committed_state_write_lock: SharedWriteGuard<CommittedState>,
    pub(crate) sequence_state_lock: SharedMutexGuard<SequencesState>,
    pub(crate) memory_lock: SharedMutexGuard<BTreeMap<DataKey, Arc<Vec<u8>>>>,
    pub(crate) lock_wait_time: Duration,
    pub(crate) timer: Instant,
}

impl StateView for MutTxId {
    fn get_schema(&self, table_id: &TableId) -> Option<&TableSchema> {
        if let Some(schema) = self
            .tx_state
            .insert_tables
            .get(table_id)
            .map(|table| table.get_schema())
        {
            return Some(schema);
        }
        self.committed_state_write_lock
            .tables
            .get(table_id)
            .map(|table| table.get_schema())
    }

    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: &TableId) -> Result<Iter> {
        if let Some(table_name) = self.table_exists(table_id) {
            return Ok(Iter::new(
                ctx,
                *table_id,
                table_name,
                Some(&self.tx_state),
                &self.committed_state_write_lock,
            ));
        }
        Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into())
    }

    fn table_exists(&self, table_id: &TableId) -> Option<&str> {
        if let Some(table) = self.tx_state.insert_tables.get(table_id) {
            Some(&table.schema.table_name)
        } else if let Some(table) = self.committed_state_write_lock.tables.get(table_id) {
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
    ) -> Result<IterByColRange<R>> {
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
                ctx,
                table_id: *table_id,
                tx_state: &self.tx_state,
                inserted_rows,
                committed_rows: self.committed_state_write_lock.index_seek(table_id, &cols, &range),
                committed_state: &self.committed_state_write_lock,
                num_committed_rows_fetched: 0,
            }))
        } else {
            // Either the current transaction has not modified this table, or the table is not
            // indexed.
            match self.committed_state_write_lock.index_seek(table_id, &cols, &range) {
                Some(committed_rows) => Ok(IterByColRange::CommittedIndex(CommittedIndexIter {
                    ctx,
                    table_id: *table_id,
                    tx_state: Some(&self.tx_state),
                    committed_state: &self.committed_state_write_lock,
                    committed_rows,
                    num_committed_rows_fetched: 0,
                })),
                None => Ok(IterByColRange::Scan(ScanIterByColRange {
                    range,
                    cols,
                    scan_iter: self.iter(ctx, table_id)?,
                })),
            }
        }
    }
}

impl MutTxId {
    fn drop_col_eq(
        &mut self,
        table_id: TableId,
        col_pos: ColId,
        value: AlgebraicValue,
        database_address: Address,
    ) -> Result<()> {
        let ctx = ExecutionContext::internal(database_address);
        let rows = self.iter_by_col_eq(&ctx, &table_id, col_pos.into(), value)?;
        let ids_to_delete = rows.map(|row| RowId(*row.id())).collect::<Vec<_>>();
        if ids_to_delete.is_empty() {
            return Err(TableError::IdNotFound(SystemTable::st_columns, col_pos.0).into());
        }
        self.delete(&table_id, ids_to_delete);

        Ok(())
    }

    fn find_by_col_eq<'a, T>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: TableId,
        col_pos: ColId,
        value: AlgebraicValue,
    ) -> Result<Option<T>>
    where
        T: TryFrom<&'a ProductValue>,
        <T as TryFrom<&'a ProductValue>>::Error: Into<DBError>,
    {
        let mut rows = self.iter_by_col_eq(ctx, &table_id, col_pos.into(), value)?;
        rows.next()
            .map(|row| T::try_from(row.view()).map_err(Into::into))
            .transpose()
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn get_next_sequence_value(&mut self, seq_id: SequenceId, database_address: Address) -> Result<i128> {
        {
            let Some(sequence) = self.sequence_state_lock.get_sequence_mut(seq_id) else {
                return Err(SequenceError::NotFound(seq_id).into());
            };

            // If there are allocated sequence values, return the new value, if it is not bigger than
            // the upper range of `sequence.allocated`
            if let Some(value) = sequence.gen_next_value().filter(|v| v < &sequence.allocated()) {
                return Ok(value);
            }
        }
        // Allocate new sequence values
        // If we're out of allocations, then update the sequence row in st_sequences to allocate a fresh batch of sequences.
        let ctx = ExecutionContext::internal(database_address);
        let old_seq_row = self
            .iter_by_col_eq(
                &ctx,
                &ST_SEQUENCES_ID,
                StSequenceFields::SequenceId.into(),
                seq_id.into(),
            )?
            .last()
            .unwrap()
            .data;
        let (seq_row, old_seq_row_id) = {
            let old_seq_row_id = RowId(old_seq_row.to_data_key());
            let mut seq_row = StSequenceRow::try_from(old_seq_row)?.to_owned();

            let Some(sequence) = self.sequence_state_lock.get_sequence_mut(seq_id) else {
                return Err(SequenceError::NotFound(seq_id).into());
            };
            seq_row.allocated = sequence.nth_value(SEQUENCE_PREALLOCATION_AMOUNT as usize);
            sequence.set_allocation(seq_row.allocated);
            (seq_row, old_seq_row_id)
        };

        self.delete(&ST_SEQUENCES_ID, [old_seq_row_id]);
        self.insert(ST_SEQUENCES_ID, ProductValue::from(seq_row), database_address)?;

        let Some(sequence) = self.sequence_state_lock.get_sequence_mut(seq_id) else {
            return Err(SequenceError::NotFound(seq_id).into());
        };
        if let Some(value) = sequence.gen_next_value() {
            return Ok(value);
        }
        Err(SequenceError::UnableToAllocate(seq_id).into())
    }

    pub(crate) fn create_sequence(
        &mut self,
        table_id: TableId,
        seq: SequenceDef,
        database_address: Address,
    ) -> Result<SequenceId> {
        log::trace!(
            "SEQUENCE CREATING: {} for table: {} and col: {}",
            seq.sequence_name,
            table_id,
            seq.col_pos
        );

        // Insert the sequence row into st_sequences
        // NOTE: Because st_sequences has a unique index on sequence_name, this will
        // fail if the table already exists.
        let sequence_row = StSequenceRow {
            sequence_id: 0.into(), // autogen'd
            sequence_name: seq.sequence_name,
            table_id,
            col_pos: seq.col_pos,
            allocated: seq.allocated,
            increment: seq.increment,
            start: seq.start.unwrap_or(1),
            min_value: seq.min_value.unwrap_or(1),
            max_value: seq.max_value.unwrap_or(i128::MAX),
        };
        let row = sequence_row.into();
        let result = self.insert(ST_SEQUENCES_ID, row, database_address)?;
        // TODO(centril): `result` is already owned, so pass that in.
        let sequence_row = StSequenceRow::try_from(&result)?.to_owned();
        let sequence_id = sequence_row.sequence_id;

        let schema: SequenceSchema = sequence_row.into();
        let insert_table = self.get_insert_table_mut(schema.table_id)?;
        insert_table.schema.update_sequence(schema.clone());
        self.sequence_state_lock
            .insert(schema.sequence_id, Sequence::new(schema));

        log::trace!("SEQUENCE CREATED: id = {}", sequence_id);

        Ok(sequence_id)
    }

    fn get_insert_table_mut(&mut self, table_id: TableId) -> Result<&mut Table> {
        self.tx_state
            .get_insert_table_mut(&table_id)
            .ok_or_else(|| TableError::IdNotFoundState(table_id).into())
    }

    pub(crate) fn drop_sequence(&mut self, sequence_id: SequenceId, database_address: Address) -> Result<()> {
        let ctx = ExecutionContext::internal(database_address);

        let st: StSequenceRow<&str> = self
            .find_by_col_eq(
                &ctx,
                ST_SEQUENCES_ID,
                StSequenceFields::SequenceId.col_id(),
                sequence_id.into(),
            )?
            .unwrap();

        let table_id = st.table_id;

        self.drop_col_eq(
            ST_SEQUENCES_ID,
            StSequenceFields::SequenceId.col_id(),
            sequence_id.into(),
            database_address,
        )?;

        self.sequence_state_lock.remove(sequence_id);
        if let Some(insert_table) = self.tx_state.get_insert_table_mut(&table_id) {
            insert_table.schema.remove_sequence(sequence_id);
        }
        Ok(())
    }

    pub(crate) fn sequence_id_from_name(
        &self,
        seq_name: &str,
        database_address: Address,
    ) -> Result<Option<SequenceId>> {
        self.iter_by_col_eq(
            &ExecutionContext::internal(database_address),
            &ST_SEQUENCES_ID,
            StSequenceFields::SequenceName.into(),
            AlgebraicValue::String(seq_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next().map(|row| {
                let id = row.view().elements[0].as_u32().unwrap();
                (*id).into()
            })
        })
    }

    pub(crate) fn create_constraint(
        &mut self,
        table_id: TableId,
        constraint: ConstraintDef,
        database_address: Address,
    ) -> Result<ConstraintId> {
        log::trace!(
            "CONSTRAINT CREATING: {} for table: {} and cols: {:?}",
            constraint.constraint_name,
            table_id,
            constraint.columns
        );

        // Verify we have 1 column if need `auto_inc`
        if constraint.constraints.has_autoinc() && constraint.columns.len() != 1 {
            return Err(SequenceError::MultiColumnAutoInc(table_id, constraint.columns).into());
        };

        // Insert the constraint row into st_constraint
        // NOTE: Because st_constraint has a unique index on constraint_name, this will
        // fail if the table already exists.
        let constraint_row = StConstraintRow {
            constraint_id: 0.into(), // autogen'd
            columns: constraint.columns.clone(),
            constraint_name: constraint.constraint_name.clone(),
            constraints: constraint.constraints,
            table_id,
        };

        let row = ProductValue::from(constraint_row);
        let result = self.insert(ST_CONSTRAINTS_ID, row, database_address)?;
        let constraint_row = StConstraintRow::try_from(&result)?;
        let constraint_id = constraint_row.constraint_id;

        let mut constraint = ConstraintSchema::from_def(table_id, constraint);
        constraint.constraint_id = constraint_id;
        let insert_table = self.get_insert_table_mut(constraint.table_id)?;
        let constraint_name = constraint.constraint_name.clone();
        insert_table.schema.update_constraint(constraint);

        log::trace!("CONSTRAINT CREATED: {}", constraint_name);

        Ok(constraint_id)
    }

    pub(crate) fn drop_constraint(&mut self, constraint_id: ConstraintId, database_address: Address) -> Result<()> {
        let ctx = ExecutionContext::internal(database_address);

        let st: StConstraintRow<&str> = self
            .find_by_col_eq(
                &ctx,
                ST_CONSTRAINTS_ID,
                StConstraintFields::ConstraintId.col_id(),
                constraint_id.into(),
            )?
            .unwrap();

        let table_id = st.table_id;

        self.drop_col_eq(
            ST_CONSTRAINTS_ID,
            StConstraintFields::ConstraintId.col_id(),
            constraint_id.into(),
            database_address,
        )?;

        if let Some(insert_table) = self.tx_state.get_insert_table_mut(&table_id) {
            insert_table.schema.remove_constraint(constraint_id);
        }

        Ok(())
    }

    pub(crate) fn constraint_id_from_name(
        &self,
        constraint_name: &str,
        database_address: Address,
    ) -> Result<Option<ConstraintId>> {
        let ctx = ExecutionContext::internal(database_address);

        Ok(self
            .find_by_col_eq::<StConstraintRow<&str>>(
                &ctx,
                ST_CONSTRAINTS_ID,
                StConstraintFields::ConstraintName.col_id(),
                AlgebraicValue::String(constraint_name.to_owned()),
            )?
            .map(|x| x.constraint_id))
    }

    fn validate_table(table_schema: &TableDef) -> Result<()> {
        if table_name_is_system(&table_schema.table_name) {
            return Err(TableError::System(table_schema.table_name.clone()).into());
        }

        table_schema
            .clone()
            .into_schema(0.into())
            .validated()
            .map_err(|err| DBError::Schema(SchemaErrors(err)))?;

        Ok(())
    }

    // NOTE: It is essential to keep this function in sync with the
    // `Self::schema_for_table`, as it must reflect the same steps used
    // to create database objects when querying for information about the table.
    pub(crate) fn create_table(&mut self, table_schema: TableDef, database_address: Address) -> Result<TableId> {
        log::trace!("TABLE CREATING: {}", &table_schema.table_name);

        Self::validate_table(&table_schema)?;

        // Insert the table row into `st_tables`
        // NOTE: Because `st_tables` has a unique index on `table_name`, this will
        // fail if the table already exists.
        let row = StTableRow {
            table_id: ST_TABLES_ID,
            table_name: &*table_schema.table_name,
            table_type: table_schema.table_type,
            table_access: table_schema.table_access,
        };
        let row = self.insert(ST_TABLES_ID, row.into(), database_address)?;
        let row = StTableRow::try_from(&row)?;
        let table_id = row.table_id;

        // Generate the full definition of the table, with the generated indexes, constraints, sequences...
        let table_schema = table_schema.into_schema(table_id);

        // Insert the columns into `st_columns`
        for col in table_schema.columns() {
            let row = StColumnRow {
                table_id,
                col_pos: col.col_pos,
                col_name: col.col_name.clone(),
                col_type: col.col_type.clone(),
            };
            self.insert(ST_COLUMNS_ID, row.into(), database_address)?;
        }

        // Create the in memory representation of the table
        // NOTE: This should be done before creating the indexes
        let mut schema_internal = table_schema.clone();
        // Remove the adjacent object that has an unset `id = 0`, they will be created below with the correct `id`
        schema_internal.clear_adjacent_schemas();

        self.create_table_internal(table_id, schema_internal)?;

        // Insert constraints into `st_constraints`
        for constraint in table_schema.constraints {
            self.create_constraint(constraint.table_id, constraint.into(), database_address)?;
        }

        // Insert sequences into `st_sequences`
        for seq in table_schema.sequences {
            self.create_sequence(seq.table_id, seq.into(), database_address)?;
        }

        // Create the indexes for the table
        for index in table_schema.indexes {
            self.create_index(table_id, index.into(), database_address)?;
        }

        log::trace!("TABLE CREATED: {}, table_id:{table_id}", row.table_name);

        Ok(table_id)
    }

    fn create_table_internal(&mut self, table_id: TableId, schema: TableSchema) -> Result<()> {
        self.tx_state.insert_tables.insert(table_id, Table::new(schema));
        Ok(())
    }

    pub(crate) fn row_type_for_table(
        &self,
        table_id: TableId,
        database_address: Address,
    ) -> Result<Cow<'_, ProductType>> {
        // Fetch the `ProductType` from the in memory table if it exists.
        // The `ProductType` is invalidated if the schema of the table changes.
        if let Some(row_type) = self.get_row_type(&table_id) {
            return Ok(Cow::Borrowed(row_type));
        }

        // Look up the columns for the table in question.
        // NOTE: This is quite an expensive operation, although we only need
        // to do this in situations where there is not currently an in memory
        // representation of a table. This would happen in situations where
        // we have created the table in the database, but have not yet
        // represented in memory or inserted any rows into it.
        Ok(
            match self.schema_for_table(&ExecutionContext::internal(database_address), table_id)? {
                Cow::Borrowed(x) => Cow::Borrowed(x.get_row_type()),
                Cow::Owned(x) => Cow::Owned(x.into_row_type()),
            },
        )
    }

    pub(crate) fn drop_table(&mut self, table_id: TableId, database_address: Address) -> Result<()> {
        let schema = self
            .schema_for_table(&ExecutionContext::internal(database_address), table_id)?
            .into_owned();

        for row in schema.indexes {
            self.drop_index(row.index_id, database_address)?;
        }

        for row in schema.sequences {
            self.drop_sequence(row.sequence_id, database_address)?;
        }

        for row in schema.constraints {
            self.drop_constraint(row.constraint_id, database_address)?;
        }

        // Drop the table and their columns
        self.drop_col_eq(
            ST_TABLES_ID,
            StTableFields::TableId.col_id(),
            table_id.into(),
            database_address,
        )?;
        self.drop_col_eq(
            ST_COLUMNS_ID,
            StColumnFields::TableId.col_id(),
            table_id.into(),
            database_address,
        )?;

        // Delete the table and its rows and indexes from memory.
        // TODO: This needs to not remove it from the committed state, because it can still be rolled back.
        // We will have to store the deletion in the TxState and then apply it to the CommittedState in commit.

        // NOT use unwrap
        self.committed_state_write_lock.tables.remove(&table_id);
        Ok(())
    }

    pub(crate) fn rename_table(&mut self, table_id: TableId, new_name: &str, database_address: Address) -> Result<()> {
        let ctx = ExecutionContext::internal(database_address);

        let st: StTableRow<&str> = self
            .find_by_col_eq(&ctx, ST_TABLES_ID, StTableFields::TableId.col_id(), table_id.into())?
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;

        let mut pv = ProductValue::from(st);
        let row_ids = RowId(ProductValue::from(st).to_data_key());

        self.delete(&ST_TABLES_ID, [row_ids]);
        // Update the table's name in st_tables.
        pv.elements[StTableFields::TableName.col_idx()] = AlgebraicValue::String(new_name.into());
        self.insert(ST_TABLES_ID, pv, database_address)?;
        Ok(())
    }

    pub(crate) fn table_name_from_id<'a>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: TableId,
    ) -> Result<Option<&str>> {
        self.iter_by_col_eq(ctx, &ST_TABLES_ID, StTableFields::TableId.into(), table_id.into())
            .map(|mut iter| {
                iter.next()
                    .map(|row| row.view().elements[1].as_string().unwrap().deref())
            })
    }

    pub(crate) fn create_index(
        &mut self,
        table_id: TableId,
        index: IndexDef,
        database_address: Address,
    ) -> Result<IndexId> {
        log::trace!(
            "INDEX CREATING: {} for table: {} and col(s): {:?}",
            index.index_name,
            table_id,
            index.columns
        );
        if self.table_exists(&table_id).is_none() {
            return Err(TableError::IdNotFoundState(table_id).into());
        }

        // Insert the index row into st_indexes
        // NOTE: Because st_indexes has a unique index on index_name, this will
        // fail if the index already exists.
        let row = StIndexRow {
            index_id: 0.into(), // Autogen'd
            table_id,
            index_type: index.index_type,
            index_name: index.index_name.clone(),
            columns: index.columns.clone(),
            is_unique: index.is_unique,
        };
        let index_id = StIndexRow::try_from(&self.insert(ST_INDEXES_ID, row.into(), database_address)?)?.index_id;

        let mut index = IndexSchema::from_def(table_id, index);
        index.index_id = index_id;
        let index_name = index.index_name.clone();
        let columns = index.columns.clone();
        self.create_index_internal(index, database_address)?;

        log::trace!(
            "INDEX CREATED: {} for table: {} and col(s): {:?}",
            index_name,
            table_id,
            columns
        );
        Ok(index_id)
    }

    fn create_index_internal(&mut self, index: IndexSchema, database_address: Address) -> Result<()> {
        let index_id = index.index_id;

        let insert_table = if let Some(insert_table) = self.tx_state.get_insert_table_mut(&index.table_id) {
            insert_table
        } else {
            let schema = self
                .schema_for_table(&ExecutionContext::internal(database_address), index.table_id)?
                .into_owned();
            self.tx_state.insert_tables.insert(index.table_id, Table::new(schema));

            self.tx_state.get_insert_table_mut(&index.table_id).unwrap()
        };

        let mut insert_index = BTreeIndex::new(
            index_id,
            index.table_id,
            index.columns.clone(),
            index.index_name.to_string(),
            index.is_unique,
        );
        insert_index.build_from_rows(insert_table.scan_rows())?;

        // NOTE: Also add all the rows in the already committed table to the index.
        if let Some(committed_table) = self.committed_state_write_lock.get_table(&index.table_id) {
            insert_index.build_from_rows(committed_table.scan_rows())?;
        }

        insert_table.schema.indexes.push(IndexSchema {
            table_id: index.table_id,
            columns: index.columns.clone(),
            index_name: index.index_name.clone(),
            is_unique: index.is_unique,
            index_id,
            index_type: index.index_type,
        });

        insert_table.indexes.insert(index.columns, insert_index);
        Ok(())
    }

    pub(crate) fn drop_index(&mut self, index_id: IndexId, database_address: Address) -> Result<()> {
        log::trace!("INDEX DROPPING: {}", index_id);
        let ctx = ExecutionContext::internal(database_address);

        let st: StIndexRow<&str> = self
            .find_by_col_eq(&ctx, ST_INDEXES_ID, StIndexFields::IndexId.col_id(), index_id.into())?
            .unwrap();
        let table_id = st.table_id;

        // Remove the index from st_indexes.
        self.drop_col_eq(
            ST_INDEXES_ID,
            StIndexFields::IndexId.col_id(),
            index_id.into(),
            database_address,
        )?;

        let clear_indexes = |table: &mut Table| {
            let cols: Vec<_> = table
                .indexes
                .values()
                .filter(|i| i.index_id == index_id)
                .map(|i| i.cols.clone())
                .collect();

            for col in cols {
                table.schema.indexes.retain(|x| x.columns != col);
                table.indexes.remove(&col);
            }
        };

        for (_, table) in self.committed_state_write_lock.tables.iter_mut() {
            clear_indexes(table);
        }
        if let Some(insert_table) = self.tx_state.get_insert_table_mut(&table_id) {
            clear_indexes(insert_table);
        }

        log::trace!("INDEX DROPPED: {}", index_id);
        Ok(())
    }

    pub(crate) fn index_id_from_name(&self, index_name: &str, database_address: Address) -> Result<Option<IndexId>> {
        self.iter_by_col_eq(
            &ExecutionContext::internal(database_address),
            &ST_INDEXES_ID,
            StIndexFields::IndexName.into(),
            AlgebraicValue::String(index_name.to_owned()),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| IndexId(*row.view().elements[0].as_u32().unwrap()))
        })
    }

    fn contains_row(&mut self, table_id: &TableId, row_id: &RowId) -> RowState<'_> {
        match self.tx_state.get_row_op(table_id, row_id) {
            RowState::Committed(_) => unreachable!("a row cannot be committed in a tx state"),
            RowState::Insert(pv) => return RowState::Insert(pv),
            RowState::Delete => return RowState::Delete,
            RowState::Absent => (),
        }
        match self
            .committed_state_write_lock
            .tables
            .get(table_id)
            .and_then(|table| table.rows.get(row_id))
        {
            Some(pv) => RowState::Committed(pv),
            None => RowState::Absent,
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn insert(
        &mut self,
        table_id: TableId,
        mut row: ProductValue,
        database_address: Address,
    ) -> Result<ProductValue> {
        // TODO: Executing schema_for_table for every row insert is expensive.
        // However we ask for the schema in the [Table] struct instead.
        let schema = self.schema_for_table(&ExecutionContext::internal(database_address), table_id)?;
        let ctx = ExecutionContext::internal(database_address);

        let mut col_to_update = None;
        for seq in &schema.sequences {
            if !row.elements[usize::from(seq.col_pos)].is_numeric_zero() {
                continue;
            }
            for seq_row in self.iter_by_col_eq(
                &ctx,
                &ST_SEQUENCES_ID,
                StSequenceFields::TableId.into(),
                table_id.into(),
            )? {
                let seq_row = seq_row.view();
                let seq_row = StSequenceRow::try_from(seq_row)?;
                if seq_row.col_pos != seq.col_pos {
                    continue;
                }

                col_to_update = Some((seq.col_pos, seq_row.sequence_id));
                break;
            }
        }

        if let Some((col_id, sequence_id)) = col_to_update {
            let col_idx = col_id.idx();
            let col = &schema.columns()[col_idx];
            if !col.col_type.is_integer() {
                return Err(SequenceError::NotInteger {
                    col: format!("{}.{}", &schema.table_name, &col.col_name),
                    found: col.col_type.clone(),
                }
                .into());
            }
            // At this point, we know this will be essentially a cheap copy.
            let col_ty = col.col_type.clone();
            let seq_val = self.get_next_sequence_value(sequence_id, database_address)?;
            row.elements[col_idx] = AlgebraicValue::from_sequence_value(&col_ty, seq_val);
        }

        self.insert_row_internal(table_id, row.clone())?;
        Ok(row)
    }

    #[tracing::instrument(skip_all)]
    fn insert_row_internal(&mut self, table_id: TableId, row: ProductValue) -> Result<()> {
        let mut bytes = Vec::new();
        row.encode(&mut bytes);
        let data_key = DataKey::from_data(&bytes);
        let row_id = RowId(data_key);

        // If the table does exist in the tx state, we need to create it based on the table in the
        // committed state. If the table does not exist in the committed state, it doesn't exist
        // in the database.
        let insert_table = if let Some(table) = self.tx_state.get_insert_table(&table_id) {
            table
        } else {
            let Some(committed_table) = self.committed_state_write_lock.tables.get(&table_id) else {
                return Err(TableError::IdNotFoundState(table_id).into());
            };
            let table = Table {
                schema: committed_table.get_schema().clone(),
                indexes: committed_table
                    .indexes
                    .iter()
                    .map(|(cols, index)| {
                        (
                            cols.clone(),
                            BTreeIndex::new(
                                index.index_id,
                                index.table_id,
                                index.cols.clone(),
                                index.name.clone(),
                                index.is_unique,
                            ),
                        )
                    })
                    .collect(),
                rows: Default::default(),
            };
            self.tx_state.insert_tables.insert(table_id, table);
            self.tx_state.get_insert_table(&table_id).unwrap()
        };

        // Check unique constraints
        for index in insert_table.indexes.values() {
            if index.violates_unique_constraint(&row) {
                let value = row.project_not_empty(&index.cols).unwrap();
                return Err(Self::build_error_unique(index, insert_table, value).into());
            }
        }
        if let Some(table) = self.committed_state_write_lock.tables.get_mut(&table_id) {
            for index in table.indexes.values() {
                let value = index.get_fields(&row)?;
                let Some(violators) = index.get_rows_that_violate_unique_constraint(&value) else {
                    continue;
                };
                for row_id in violators {
                    if let Some(delete_table) = self.tx_state.delete_tables.get(&table_id) {
                        if !delete_table.contains(row_id) {
                            let value = row.project_not_empty(&index.cols).unwrap();
                            return Err(Self::build_error_unique(index, table, value).into());
                        }
                    } else {
                        let value = row.project_not_empty(&index.cols)?;
                        return Err(Self::build_error_unique(index, table, value).into());
                    }
                }
            }
        }

        // Now that we have checked all the constraints, we can perform the actual insertion.
        {
            // We have a few cases to consider, based on the history of this transaction, and
            // whether the row was already present or not at the start of this transaction.
            // 1. If the row was not originally present, and therefore also not deleted by
            //    this transaction, we will add it to `insert_tables`.
            // 2. If the row was originally present, but not deleted by this transaction,
            //    we should fail, as we would otherwise violate set semantics.
            // 3. If the row was originally present, and is currently going to be deleted
            //    by this transaction, we will remove it from `delete_tables`, and the
            //    cummulative effect will be to leave the row in place in the committed state.

            let delete_table = self.tx_state.get_or_create_delete_table(table_id);
            let row_was_previously_deleted = delete_table.remove(&row_id);

            // If the row was just deleted in this transaction and we are re-inserting it now,
            // we're done. Otherwise we have to add the row to the insert table, and into our memory.
            if row_was_previously_deleted {
                return Ok(());
            }

            let insert_table = self.tx_state.get_insert_table_mut(&table_id).unwrap();

            // TODO(cloutiertyler): should probably also check that all the columns are correct? Perf considerations.
            if insert_table.schema.columns().len() != row.elements.len() {
                return Err(TableError::RowInvalidType { table_id, row }.into());
            }

            insert_table.insert(row_id, row);

            match data_key {
                DataKey::Data(_) => (),
                DataKey::Hash(_) => {
                    self.memory_lock.insert(data_key, Arc::new(bytes));
                }
            };
        }

        Ok(())
    }

    fn build_error_unique(index: &BTreeIndex, table: &Table, value: AlgebraicValue) -> IndexError {
        IndexError::UniqueConstraintViolation(UniqueConstraintViolation {
            constraint_name: index.name.clone(),
            table_name: table.schema.table_name.clone(),
            cols: index
                .cols
                .iter()
                .map(|x| table.schema.columns()[usize::from(x)].col_name.clone())
                .collect(),
            value,
        })
    }

    pub(crate) fn get<'a>(&'a self, table_id: &TableId, row_id: &'a RowId) -> Result<Option<DataRef<'a>>> {
        if self.table_exists(table_id).is_none() {
            return Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into());
        }
        match self.tx_state.get_row_op(table_id, row_id) {
            RowState::Committed(_) => unreachable!("a row cannot be committed in a tx state"),
            RowState::Insert(row) => {
                return Ok(Some(DataRef::new(row_id, row)));
            }
            RowState::Delete => {
                return Ok(None);
            }
            RowState::Absent => {}
        }
        Ok(self
            .committed_state_write_lock
            .tables
            .get(table_id)
            .and_then(|table| table.get_row(row_id))
            .map(|row| DataRef::new(row_id, row)))
    }

    fn get_row_type(&self, table_id: &TableId) -> Option<&ProductType> {
        if let Some(row_type) = self
            .tx_state
            .insert_tables
            .get(table_id)
            .map(|table| table.get_row_type())
        {
            return Some(row_type);
        }
        self.committed_state_write_lock
            .tables
            .get(table_id)
            .map(|table| table.get_row_type())
    }

    pub(crate) fn delete(&mut self, table_id: &TableId, row_ids: impl IntoIterator<Item = RowId>) -> u32 {
        row_ids
            .into_iter()
            .map(|row_id| self.delete_row_internal(table_id, &row_id) as u32)
            .sum()
    }

    fn delete_row_internal(&mut self, table_id: &TableId, row_id: &RowId) -> bool {
        match self.contains_row(table_id, row_id) {
            RowState::Committed(_) => {
                // If the row is present because of a previously committed transaction,
                // we need to add it to the appropriate delete_table.
                self.tx_state.get_or_create_delete_table(*table_id).insert(*row_id);
                // True because we did delete the row.
                true
            }
            RowState::Insert(_) => {
                // If the row is present because of a an insertion in this transaction,
                // we need to remove it from the appropriate insert_table.
                let insert_table = self.tx_state.get_insert_table_mut(table_id).unwrap();
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

    pub(crate) fn delete_by_rel(
        &mut self,
        table_id: &TableId,
        relation: impl IntoIterator<Item = ProductValue>,
    ) -> u32 {
        self.delete(table_id, relation.into_iter().map(|pv| RowId(pv.to_data_key())))
    }

    pub(crate) fn commit(mut self) -> Result<Option<TxData>> {
        let memory: BTreeMap<DataKey, Arc<Vec<u8>>> = std::mem::take(&mut self.memory_lock);
        let tx_data = self.committed_state_write_lock.merge(self.tx_state, memory);
        Ok(Some(tx_data))
    }

    pub(crate) fn rollback(self) {
        // TODO: Check that no sequences exceed their allocation after the rollback.
    }
}
