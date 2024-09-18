use super::{
    committed_state::{CommittedIndexIter, CommittedState},
    datastore::{record_metrics, Result},
    sequence::{Sequence, SequencesState},
    state_view::{IndexSeekIterMutTxId, Iter, IterByColRange, ScanIterByColRange, StateView},
    tx::TxId,
    tx_state::{IndexIdMap, TxState},
    SharedMutexGuard, SharedWriteGuard,
};
use crate::db::datastore::{
    system_tables::{
        table_name_is_system, StColumnFields, StColumnRow, StConstraintFields, StConstraintRow, StFields as _,
        StIndexFields, StIndexRow, StScheduledRow, StSequenceFields, StSequenceRow, StTableFields, StTableRow,
        SystemTable, ST_COLUMN_ID, ST_CONSTRAINT_ID, ST_INDEX_ID, ST_SCHEDULED_ID, ST_SEQUENCE_ID, ST_TABLE_ID,
    },
    traits::{RowTypeForTable, TxData},
};
use crate::{
    error::{DBError, IndexError, SequenceError, TableError},
    execution_context::ExecutionContext,
};
use core::ops::RangeBounds;
use core::{iter, ops::Bound};
use smallvec::SmallVec;
use spacetimedb_lib::{
    address::Address,
    bsatn::Deserializer,
    db::{
        auth::StAccess,
        error::SchemaErrors,
        raw_def::{RawConstraintDefV8, RawIndexDefV8, RawSequenceDefV8, RawTableDefV8, SEQUENCE_ALLOCATION_STEP},
    },
    de::DeserializeSeed,
};
use spacetimedb_primitives::{ColId, ColList, ConstraintId, Constraints, IndexId, SequenceId, TableId};
use spacetimedb_sats::{
    bsatn::{self, DecodeError},
    de::WithBound,
    AlgebraicType, AlgebraicValue, ProductType, ProductValue, WithTypespace,
};
use spacetimedb_schema::schema::{ConstraintSchema, IndexSchema, SequenceSchema, TableSchema};
use spacetimedb_table::{
    blob_store::{BlobStore, HashMapBlobStore},
    indexes::{RowPointer, SquashedOffset},
    table::{InsertError, RowRef, Table},
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
}

impl MutTxId {
    fn drop_col_eq(
        &mut self,
        table_id: TableId,
        col_pos: ColId,
        value: &AlgebraicValue,
        database_address: Address,
    ) -> Result<()> {
        let ctx = ExecutionContext::internal(database_address);
        let rows = self.iter_by_col_eq(&ctx, table_id, col_pos, value)?;
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

    fn validate_table(table_schema: &RawTableDefV8) -> Result<()> {
        if table_name_is_system(&table_schema.table_name) {
            return Err(TableError::System(table_schema.table_name.clone()).into());
        }

        #[allow(deprecated)]
        TableSchema::from_def(0.into(), table_schema.clone())
            .validated()
            .map_err(|err| DBError::Schema(SchemaErrors(err)))?;

        Ok(())
    }

    pub fn create_table(&mut self, table_schema: RawTableDefV8, database_address: Address) -> Result<TableId> {
        log::trace!("TABLE CREATING: {}", table_schema.table_name);

        Self::validate_table(&table_schema)?;

        // Insert the table row into `st_tables`
        // NOTE: Because `st_tables` has a unique index on `table_name`, this will
        // fail if the table already exists.
        let row = StTableRow {
            table_id: 0.into(), // autoinc
            table_name: table_schema.table_name.clone(),
            table_type: table_schema.table_type,
            table_access: table_schema.table_access,
        };
        let table_id = self
            .insert(ST_TABLE_ID, &mut row.into(), database_address)?
            .1
            .collapse()
            .read_col(StTableFields::TableId)?;

        // Generate the full definition of the table, with the generated indexes, constraints, sequences...
        #[allow(deprecated)]
        let table_schema = TableSchema::from_def(table_id, table_schema);

        // Insert the columns into `st_columns`
        for col in table_schema.columns() {
            let row = StColumnRow {
                table_id,
                col_pos: col.col_pos,
                col_name: col.col_name.clone(),
                col_type: col.col_type.clone(),
            };
            self.insert(ST_COLUMN_ID, &mut row.into(), database_address)?;
        }

        // Create the in memory representation of the table
        // NOTE: This should be done before creating the indexes
        let mut schema_internal = table_schema.clone();
        // Remove the adjacent object that has an unset `id = 0`, they will be created below with the correct `id`
        schema_internal.clear_adjacent_schemas();

        self.create_table_internal(table_id, schema_internal.into());

        // Insert the scheduled table entry into `st_scheduled`
        if let Some(reducer_name) = table_schema.scheduled {
            let row = StScheduledRow { table_id, reducer_name };
            self.insert(ST_SCHEDULED_ID, &mut row.into(), database_address)?;
        }

        // Insert constraints into `st_constraints`
        let ctx = ExecutionContext::internal(database_address);
        for constraint in table_schema.constraints {
            self.create_constraint(&ctx, constraint.table_id, constraint.into())?;
        }

        // Insert sequences into `st_sequences`
        for seq in table_schema.sequences {
            self.create_sequence(seq.table_id, seq.into(), database_address)?;
        }

        // Create the indexes for the table
        for index in table_schema.indexes {
            self.create_index_no_constraint(&ctx, table_id, index.into())?;
        }

        log::trace!("TABLE CREATED: {}, table_id: {table_id}", table_schema.table_name);

        Ok(table_id)
    }

    fn create_table_internal(&mut self, table_id: TableId, schema: Arc<TableSchema>) {
        self.tx_state
            .insert_tables
            .insert(table_id, Table::new(schema, SquashedOffset::TX_STATE));
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

    pub fn row_type_for_table(&self, table_id: TableId, database_address: Address) -> Result<RowTypeForTable<'_>> {
        // Fetch the `ProductType` from the in memory table if it exists.
        // The `ProductType` is invalidated if the schema of the table changes.
        if let Some(row_type) = self.get_row_type(table_id) {
            return Ok(RowTypeForTable::Ref(row_type));
        }

        let ctx = ExecutionContext::internal(database_address);

        // Look up the columns for the table in question.
        // NOTE: This is quite an expensive operation, although we only need
        // to do this in situations where there is not currently an in memory
        // representation of a table. This would happen in situations where
        // we have created the table in the database, but have not yet
        // represented in memory or inserted any rows into it.
        Ok(RowTypeForTable::Arc(self.schema_for_table(&ctx, table_id)?))
    }

    pub fn drop_table(&mut self, table_id: TableId, database_address: Address) -> Result<()> {
        let ctx = &ExecutionContext::internal(database_address);
        let schema = &*self.schema_for_table(ctx, table_id)?;

        for row in &schema.indexes {
            self.drop_index(row.index_id, false, database_address)?;
        }

        for row in &schema.sequences {
            self.drop_sequence(row.sequence_id, database_address)?;
        }

        for row in &schema.constraints {
            self.drop_constraint(ctx, row.constraint_id)?;
        }

        // Drop the table and their columns
        self.drop_col_eq(
            ST_TABLE_ID,
            StTableFields::TableId.col_id(),
            &table_id.into(),
            database_address,
        )?;
        self.drop_col_eq(
            ST_COLUMN_ID,
            StColumnFields::TableId.col_id(),
            &table_id.into(),
            database_address,
        )?;

        // Delete the table and its rows and indexes from memory.
        // TODO: This needs to not remove it from the committed state, because it can still be rolled back.
        // We will have to store the deletion in the TxState and then apply it to the CommittedState in commit.

        // NOT use unwrap
        self.committed_state_write_lock.tables.remove(&table_id);
        Ok(())
    }

    pub fn rename_table(&mut self, table_id: TableId, new_name: &str, database_address: Address) -> Result<()> {
        let ctx = ExecutionContext::internal(database_address);
        // Update the table's name in st_tables.
        self.update_st_table_row(&ctx, database_address, table_id, |st| st.table_name = new_name.into())
    }

    fn update_st_table_row(
        &mut self,
        ctx: &ExecutionContext,
        database_address: Address,
        table_id: TableId,
        updater: impl FnOnce(&mut StTableRow<Box<str>>),
    ) -> Result<()> {
        // Fetch the row.
        let st_table_ref = self
            .iter_by_col_eq(ctx, ST_TABLE_ID, StTableFields::TableId, &table_id.into())?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let mut row = StTableRow::try_from(st_table_ref)?;
        let ptr = st_table_ref.pointer();

        // Delete the row, run updates, and insert again.
        self.delete(ST_TABLE_ID, ptr)?;
        updater(&mut row);
        self.insert(ST_TABLE_ID, &mut row.into(), database_address)?;

        Ok(())
    }

    pub fn table_id_from_name(&self, table_name: &str, database_address: Address) -> Result<Option<TableId>> {
        let ctx = ExecutionContext::internal(database_address);
        let table_name = &table_name.into();
        let row = self
            .iter_by_col_eq(&ctx, ST_TABLE_ID, StTableFields::TableName, table_name)?
            .next();
        Ok(row.map(|row| row.read_col(StTableFields::TableId).unwrap()))
    }

    pub fn table_name_from_id<'a>(&'a self, ctx: &'a ExecutionContext, table_id: TableId) -> Result<Option<Box<str>>> {
        self.iter_by_col_eq(ctx, ST_TABLE_ID, StTableFields::TableId, &table_id.into())
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
    pub(crate) fn alter_table_access(
        &mut self,
        database_address: Address,
        table_id: TableId,
        access: StAccess,
    ) -> Result<()> {
        // Write to the table in the tx state.
        let (table, ..) = self.get_or_create_insert_table_mut(table_id)?;
        table.with_mut_schema(|s| s.table_access = access);

        // Update system tables.
        let ctx = ExecutionContext::internal(database_address);
        self.update_st_table_row(&ctx, database_address, table_id, |st| st.table_access = access)?;
        Ok(())
    }

    fn create_index_no_constraint(
        &mut self,
        ctx: &ExecutionContext,
        table_id: TableId,
        index: RawIndexDefV8,
    ) -> Result<IndexId> {
        let columns = index.columns.clone();
        log::trace!(
            "INDEX CREATING: {} for table: {} and col(s): {:?}",
            index.index_name,
            table_id,
            columns
        );
        if self.table_name(table_id).is_none() {
            return Err(TableError::IdNotFoundState(table_id).into());
        }

        // Insert the index row into st_indexes
        // NOTE: Because st_indexes has a unique index on index_name, this will
        // fail if the index already exists.
        let is_unique = index.is_unique;
        let row = StIndexRow {
            index_id: 0.into(), // Autogen'd
            table_id,
            index_type: index.index_type,
            index_name: index.index_name.clone(),
            columns: columns.clone(),
            is_unique,
        };
        let index_id = self
            .insert(ST_INDEX_ID, &mut row.into(), ctx.database())?
            .1
            .collapse()
            .read_col(StIndexFields::IndexId)?;

        // Construct the index schema.
        #[allow(deprecated)]
        let mut index = IndexSchema::from_def(table_id, index);
        index.index_id = index_id;

        // Add the index to the transaction's insert table.
        let (table, blob_store, idx_map, commit_table, commit_blob_store) =
            self.get_or_create_insert_table_mut(table_id)?;
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

    pub fn create_index(
        &mut self,
        table_id: TableId,
        index: RawIndexDefV8,
        database_address: Address,
    ) -> Result<IndexId> {
        let columns = index.columns.clone();
        let is_unique = index.is_unique;
        let ctx = ExecutionContext::internal(database_address);
        let index_id = self.create_index_no_constraint(&ctx, table_id, index)?;

        // Add the constraint.
        let constraint = self.gen_constraint_def_for_index(&ctx, table_id, columns, is_unique)?;
        self.create_constraint(&ctx, table_id, constraint)?;

        Ok(index_id)
    }

    fn gen_constraint_def_for_index(
        &self,
        ctx: &ExecutionContext,
        table_id: TableId,
        columns: ColList,
        is_unique: bool,
    ) -> Result<RawConstraintDefV8> {
        let schema = self.schema_for_table(ctx, table_id)?;
        let constraints = Constraints::from_is_unique(is_unique);
        Ok(RawConstraintDefV8::for_column(
            &schema.table_name,
            &schema.generate_cols_name(&columns),
            constraints,
            columns,
        ))
    }

    pub fn drop_index(&mut self, index_id: IndexId, drop_constraint: bool, database_address: Address) -> Result<()> {
        log::trace!("INDEX DROPPING: {}", index_id);
        let ctx = ExecutionContext::internal(database_address);

        // Find the index in `st_indexes`.
        let st_index_ref = self
            .iter_by_col_eq(&ctx, ST_INDEX_ID, StIndexFields::IndexId, &index_id.into())?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_index, index_id.into()))?;
        let st_index_row = StIndexRow::try_from(st_index_ref)?;
        let st_index_ptr = st_index_ref.pointer();
        let table_id = st_index_row.table_id;

        if drop_constraint {
            // Find the constraint related to this index and remove it.
            let constraint =
                self.gen_constraint_def_for_index(&ctx, table_id, st_index_row.columns, st_index_row.is_unique)?;
            let constraint_id = self
                .constraint_id_from_name(&ctx, &constraint.constraint_name)?
                .unwrap();
            self.drop_constraint(&ctx, constraint_id)?;
        }

        // Remove the index from st_indexes.
        self.delete(ST_INDEX_ID, st_index_ptr)?;

        // Remove the index in the transaction's insert table.
        // By altering the insert table, this gets moved over to the committed state on merge.
        let (table, _, idx_map, ..) = self.get_or_create_insert_table_mut(table_id)?;
        if let Some(col) = table
            .indexes
            .iter()
            .find(|(_, idx)| idx.index_id == index_id)
            .map(|(cols, _)| cols.clone())
        {
            // This likely will do a clone-write as over time?
            // The schema might have found other referents.
            table.with_mut_schema(|s| s.indexes.retain(|x| x.columns != col));
            table.indexes.remove(&col);
        }
        // Remove the `index_id -> (table_id, col_list)` association.
        idx_map.remove(&index_id);
        self.tx_state.index_id_map_removals.push(index_id);

        log::trace!("INDEX DROPPED: {}", index_id);
        Ok(())
    }

    pub fn index_id_from_name(&self, index_name: &str, database_address: Address) -> Result<Option<IndexId>> {
        let ctx = ExecutionContext::internal(database_address);
        let name = &index_name.into();
        let row = self
            .iter_by_col_eq(&ctx, ST_INDEX_ID, StIndexFields::IndexName, name)?
            .next();
        Ok(row.map(|row| row.read_col(StIndexFields::IndexId).unwrap()))
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
    ) -> Result<(TableId, impl Iterator<Item = RowRef<'a>>)> {
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
        let tx_iter = self.tx_state.index_seek(table_id, col_list, &bounds).unwrap();
        let commit_iter = self.committed_state_write_lock.index_seek(table_id, col_list, &bounds);

        // Chain together the indexed rows in the tx and committed state,
        // but don't yield rows deleted in the tx state.
        enum Choice<A, B, C> {
            A(A),
            B(B),
            C(C),
        }
        impl<T, A: Iterator<Item = T>, B: Iterator<Item = T>, C: Iterator<Item = T>> Iterator for Choice<A, B, C> {
            type Item = T;
            fn next(&mut self) -> Option<Self::Item> {
                match self {
                    Self::A(i) => i.next(),
                    Self::B(i) => i.next(),
                    Self::C(i) => i.next(),
                }
            }
        }
        let iter = match commit_iter {
            None => Choice::A(tx_iter),
            Some(commit_iter) => match self.tx_state.delete_tables.get(&table_id) {
                None => Choice::B(tx_iter.chain(commit_iter)),
                Some(tx_dels) => {
                    Choice::C(tx_iter.chain(commit_iter.filter(move |row| !tx_dels.contains(&row.pointer()))))
                }
            },
        };
        Ok((table_id, iter))
    }

    /// Translate `index_id` to the table id, the column list and index key type.
    fn get_table_and_index_type(&self, index_id: IndexId) -> Option<(TableId, &ColList, &AlgebraicType)> {
        // The order of querying the committed vs. tx state for the translation is not important.
        // But it is vastly more likely that it is in the committed state,
        // so query that first to avoid two lookups.
        let (table_id, col_list) = self
            .committed_state_write_lock
            .index_id_map
            .get(&index_id)
            .or_else(|| self.tx_state.index_id_map.get(&index_id))?;
        // The tx state must have the index.
        // If the index was e.g., dropped from the tx state but exists physically in the committed state,
        // the index does not exist, semantically.
        let key_ty = self.tx_state.get_table_and_index_type(*table_id, col_list)?;
        Some((*table_id, col_list, key_ty))
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

    pub fn get_next_sequence_value(&mut self, seq_id: SequenceId, database_address: Address) -> Result<i128> {
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
        let ctx = ExecutionContext::internal(database_address);
        let old_seq_row_ref = self
            .iter_by_col_eq(&ctx, ST_SEQUENCE_ID, StSequenceFields::SequenceId, &seq_id.into())?
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

    pub fn create_sequence(
        &mut self,
        table_id: TableId,
        seq: RawSequenceDefV8,
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
        let mut sequence_row = StSequenceRow {
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
        let row = self.insert(ST_SEQUENCE_ID, &mut sequence_row.clone().into(), database_address)?;
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

    pub fn drop_sequence(&mut self, sequence_id: SequenceId, database_address: Address) -> Result<()> {
        let ctx = ExecutionContext::internal(database_address);

        let st_sequence_ref = self
            .iter_by_col_eq(&ctx, ST_SEQUENCE_ID, StSequenceFields::SequenceId, &sequence_id.into())?
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

    pub fn sequence_id_from_name(&self, seq_name: &str, database_address: Address) -> Result<Option<SequenceId>> {
        let ctx = ExecutionContext::internal(database_address);
        let name = &<Box<str>>::from(seq_name).into();
        self.iter_by_col_eq(&ctx, ST_SEQUENCE_ID, StSequenceFields::SequenceName, name)
            .map(|mut iter| {
                iter.next()
                    .map(|row| row.read_col(StSequenceFields::SequenceId).unwrap())
            })
    }

    fn create_constraint(
        &mut self,
        ctx: &ExecutionContext,
        table_id: TableId,
        constraint: RawConstraintDefV8,
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

        let constraint_row = self.insert(
            ST_CONSTRAINT_ID,
            &mut ProductValue::from(constraint_row),
            ctx.database(),
        )?;
        let constraint_id = constraint_row.1.collapse().read_col(StConstraintFields::ConstraintId)?;
        let existed = matches!(constraint_row.1, RowRefInsertion::Existed(_));
        // TODO: Can we return early here?

        let (table, ..) = self.get_or_create_insert_table_mut(table_id)?;
        #[allow(deprecated)]
        let mut constraint = ConstraintSchema::from_def(table_id, constraint);
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

    pub fn drop_constraint(&mut self, ctx: &ExecutionContext, constraint_id: ConstraintId) -> Result<()> {
        // Delete row in `st_constraint`.
        let st_constraint_ref = self
            .iter_by_col_eq(
                ctx,
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

        Ok(())
    }

    pub fn constraint_id_from_name(
        &self,
        ctx: &ExecutionContext,
        constraint_name: &str,
    ) -> Result<Option<ConstraintId>> {
        self.iter_by_col_eq(
            ctx,
            ST_CONSTRAINT_ID,
            StConstraintFields::ConstraintName,
            &<Box<str>>::from(constraint_name).into(),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| row.read_col(StConstraintFields::ConstraintId).unwrap())
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

    pub fn commit(self, ctx: &ExecutionContext) -> TxData {
        let Self {
            mut committed_state_write_lock,
            tx_state,
            ..
        } = self;
        let tx_data = committed_state_write_lock.merge(tx_state, ctx);
        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_metrics(
            ctx,
            self.timer,
            self.lock_wait_time,
            true,
            Some(&tx_data),
            Some(&committed_state_write_lock),
        );
        tx_data
    }

    pub fn commit_downgrade(self, ctx: &ExecutionContext) -> (TxData, TxId) {
        let Self {
            mut committed_state_write_lock,
            tx_state,
            ..
        } = self;
        let tx_data = committed_state_write_lock.merge(tx_state, ctx);
        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_metrics(
            ctx,
            self.timer,
            self.lock_wait_time,
            true,
            Some(&tx_data),
            Some(&committed_state_write_lock),
        );
        let tx = TxId {
            committed_state_shared_lock: SharedWriteGuard::downgrade(committed_state_write_lock),
            lock_wait_time: Duration::ZERO,
            timer: Instant::now(),
        };
        (tx_data, tx)
    }

    pub fn rollback(self, ctx: &ExecutionContext) {
        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_metrics(ctx, self.timer, self.lock_wait_time, false, None, None);
    }

    pub fn rollback_downgrade(self, ctx: &ExecutionContext) -> TxId {
        // Record metrics for the transaction at the very end,
        // right before we drop and release the lock.
        record_metrics(ctx, self.timer, self.lock_wait_time, false, None, None);
        TxId {
            committed_state_shared_lock: SharedWriteGuard::downgrade(self.committed_state_write_lock),
            lock_wait_time: Duration::ZERO,
            timer: Instant::now(),
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

impl MutTxId {
    pub(super) fn insert<'a>(
        &'a mut self,
        table_id: TableId,
        row: &mut ProductValue,
        database_address: Address,
    ) -> Result<(AlgebraicValue, RowRefInsertion<'a>)> {
        let generated = self.write_sequence_values(table_id, row, database_address)?;
        let row_ref = self.insert_row_internal(table_id, row)?;
        Ok((generated, row_ref))
    }

    /// Generate and write sequence values to `row`
    /// and return a projection of `row` with only the generated column values.
    fn write_sequence_values(
        &mut self,
        table_id: TableId,
        row: &mut ProductValue,
        database_address: Address,
    ) -> Result<AlgebraicValue> {
        let ctx = ExecutionContext::internal(database_address);

        // TODO: Executing schema_for_table for every row insert is expensive.
        // However we ask for the schema in the [Table] struct instead.
        let schema = self.schema_for_table(&ctx, table_id)?;

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
            let seq_val = self.get_next_sequence_value(sequence_id, database_address)?;
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

    fn iter<'a>(&'a self, ctx: &'a ExecutionContext, table_id: TableId) -> Result<Iter<'a>> {
        if let Some(table_name) = self.table_name(table_id) {
            return Ok(Iter::new(
                ctx,
                table_id,
                table_name,
                Some(&self.tx_state),
                &self.committed_state_write_lock,
            ));
        }
        Err(TableError::IdNotFound(SystemTable::st_table, table_id.0).into())
    }

    fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
        &'a self,
        ctx: &'a ExecutionContext,
        table_id: TableId,
        cols: ColList,
        range: R,
    ) -> Result<IterByColRange<'a, R>> {
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
                table_id,
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
                Some(committed_rows) => Ok(IterByColRange::CommittedIndex(CommittedIndexIter::new(
                    ctx,
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
                                let schema = self.schema_for_table(ctx, table_id).unwrap();
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
                        self.iter(ctx, table_id)?,
                        cols,
                        range,
                    )))
                }
            }
        }
    }
}
