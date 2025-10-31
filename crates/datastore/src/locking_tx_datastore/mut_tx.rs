use super::{
    committed_state::{CommitTableForInsertion, CommittedState},
    datastore::{Result, TxMetrics},
    delete_table::DeleteTable,
    sequence::{Sequence, SequencesState},
    state_view::{IterByColEqMutTx, IterByColRangeMutTx, IterMutTx, ScanIterByColRangeMutTx, StateView},
    tx::TxId,
    tx_state::{IndexIdMap, PendingSchemaChange, TxState, TxTableForInsertion},
    SharedMutexGuard, SharedWriteGuard,
};
use crate::system_tables::{
    system_tables, ConnectionIdViaU128, StConnectionCredentialsFields, StConnectionCredentialsRow, StViewColumnFields,
    StViewFields, StViewParamFields, StViewParamRow, ST_CONNECTION_CREDENTIALS_ID, ST_VIEW_COLUMN_ID, ST_VIEW_ID,
    ST_VIEW_PARAM_ID,
};
use crate::traits::{InsertFlags, RowTypeForTable, TxData, UpdateFlags};
use crate::{
    error::{IndexError, SequenceError, TableError},
    system_tables::{
        with_sys_table_buf, StClientFields, StClientRow, StColumnFields, StColumnRow, StConstraintFields,
        StConstraintRow, StFields as _, StIndexFields, StIndexRow, StRowLevelSecurityFields, StRowLevelSecurityRow,
        StScheduledFields, StScheduledRow, StSequenceFields, StSequenceRow, StTableFields, StTableRow, SystemTable,
        ST_CLIENT_ID, ST_COLUMN_ID, ST_CONSTRAINT_ID, ST_INDEX_ID, ST_ROW_LEVEL_SECURITY_ID, ST_SCHEDULED_ID,
        ST_SEQUENCE_ID, ST_TABLE_ID,
    },
};
use crate::{execution_context::ExecutionContext, system_tables::StViewColumnRow};
use crate::{execution_context::Workload, system_tables::StViewRow};
use core::ops::RangeBounds;
use core::{cell::RefCell, mem};
use core::{iter, ops::Bound};
use smallvec::SmallVec;
use spacetimedb_data_structures::map::{IntMap, IntSet};
use spacetimedb_durability::TxOffset;
use spacetimedb_execution::{dml::MutDatastore, Datastore, DeltaStore, Row};
use spacetimedb_lib::{db::raw_def::v9::RawSql, metrics::ExecutionMetrics};
use spacetimedb_lib::{
    db::{auth::StAccess, raw_def::SEQUENCE_ALLOCATION_STEP},
    ConnectionId, Identity,
};
use spacetimedb_primitives::{
    col_list, ColId, ColList, ColSet, ConstraintId, IndexId, ScheduleId, SequenceId, TableId, ViewId,
};
use spacetimedb_sats::{
    bsatn::{self, to_writer, DecodeError, Deserializer},
    de::{DeserializeSeed, WithBound},
    ser::Serialize,
    AlgebraicType, AlgebraicValue, ProductType, ProductValue, WithTypespace,
};
use spacetimedb_schema::{
    def::{ModuleDef, ViewColumnDef, ViewDef},
    schema::{ColumnSchema, ConstraintSchema, IndexSchema, RowLevelSecuritySchema, SequenceSchema, TableSchema},
};
use spacetimedb_table::{
    blob_store::BlobStore,
    indexes::{RowPointer, SquashedOffset},
    static_assert_size,
    table::{
        BlobNumBytes, DuplicateError, IndexScanRangeIter, InsertError, RowRef, Table, TableAndIndex,
        UniqueConstraintViolation,
    },
    table_index::TableIndex,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

type DecodeResult<T> = core::result::Result<T, DecodeError>;

/// Views track their read sets and update the [`CommittedState`] with them.
/// The [`CommittedState`] maintains these read sets in order to determine when to re-evaluate a view.
#[derive(Default)]
pub struct ReadSet {
    table_scans: IntSet<TableId>,
    index_keys: IntMap<TableId, IntMap<IndexId, AlgebraicValue>>,
}

impl ReadSet {
    /// Enumerate the tables that are scanned and tracked by this read set
    pub fn tables_scanned(&self) -> impl Iterator<Item = &TableId> + '_ {
        self.table_scans.iter()
    }

    /// Enumerate the single index keys that are tracked by this read set
    pub fn index_keys_scanned(&self) -> impl Iterator<Item = (&TableId, &IndexId, &AlgebraicValue)> + '_ {
        self.index_keys
            .iter()
            .flat_map(|(table_id, keys)| keys.iter().map(move |(index_id, key)| (table_id, index_id, key)))
    }

    /// Track a table scan in this read set
    fn insert_table_scan(&mut self, table_id: TableId) {
        self.table_scans.insert(table_id);
    }

    /// Track an index scan in this read set.
    /// If we only read a single index key we record the key.
    /// If we read a range, we treat it as though we scanned the entire table.
    fn insert_index_scan(
        &mut self,
        table_id: TableId,
        index_id: IndexId,
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) {
        match (lower, upper) {
            (Bound::Included(lower), Bound::Included(upper)) if lower == upper => {
                self.index_keys.entry(table_id).or_default().insert(index_id, lower);
            }
            _ => {
                self.table_scans.insert(table_id);
            }
        }
    }
}

pub type ViewReadSets = IntMap<ViewId, ReadSet>;

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
    pub(super) read_sets: ViewReadSets,
    // TODO(cloutiertyler): The below were made `pub` for the datastore split. We should
    // make these private again.
    pub timer: Instant,
    pub ctx: ExecutionContext,
    pub metrics: ExecutionMetrics,
}

static_assert_size!(MutTxId, 432);

impl MutTxId {
    /// Record that a view performs a table scan in this transaction's read set
    pub fn record_table_scan(&mut self, view_id: Option<ViewId>, table_id: TableId) {
        if let Some(view_id) = view_id {
            self.read_sets.entry(view_id).or_default().insert_table_scan(table_id)
        }
    }

    /// Record that a view performs an index scan in this transaction's read set
    pub fn record_index_scan(
        &mut self,
        view_id: Option<ViewId>,
        table_id: TableId,
        index_id: IndexId,
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) {
        if let Some(view_id) = view_id {
            self.read_sets
                .entry(view_id)
                .or_default()
                .insert_index_scan(table_id, index_id, lower, upper)
        }
    }
}

impl Datastore for MutTxId {
    type TableIter<'a>
        = IterMutTx<'a>
    where
        Self: 'a;

    type IndexIter<'a>
        = IndexScanRanged<'a>
    where
        Self: 'a;

    fn row_count(&self, table_id: TableId) -> u64 {
        self.table_row_count(table_id).unwrap_or_default()
    }

    fn table_scan<'a>(&'a self, table_id: TableId) -> anyhow::Result<Self::TableIter<'a>> {
        Ok(self.iter(table_id)?)
    }

    fn index_scan<'a>(
        &'a self,
        table_id: TableId,
        index_id: IndexId,
        range: &impl RangeBounds<AlgebraicValue>,
    ) -> anyhow::Result<Self::IndexIter<'a>> {
        // Extract the table id, and commit/tx indices.
        let (_, commit_index, tx_index) = self
            .get_table_and_index(index_id)
            .ok_or_else(|| IndexError::NotFound(index_id))?;

        // Get an index seek iterator for the tx and committed state.
        let tx_iter = tx_index.map(|i| i.seek_range(range));
        let commit_iter = commit_index.seek_range(range);

        let dt = self.tx_state.get_delete_table(table_id);
        let iter = combine_range_index_iters(dt, tx_iter, commit_iter);

        Ok(iter)
    }
}

/// Note, deltas are evaluated using read-only transactions, not mutable ones.
/// Nevertheless this contract is still required for query evaluation.
impl DeltaStore for MutTxId {
    fn num_inserts(&self, _: TableId) -> usize {
        0
    }

    fn num_deletes(&self, _: TableId) -> usize {
        0
    }

    fn inserts_for_table(&self, _: TableId) -> Option<std::slice::Iter<'_, ProductValue>> {
        None
    }

    fn deletes_for_table(&self, _: TableId) -> Option<std::slice::Iter<'_, ProductValue>> {
        None
    }

    /// Subscriptions are currently evaluated using read-only transactions.
    /// Hence this will never be called on a mutable transaction.
    fn index_scan_range_for_delta(
        &self,
        _: TableId,
        _: IndexId,
        _: spacetimedb_lib::query::Delta,
        _: impl RangeBounds<AlgebraicValue>,
    ) -> impl Iterator<Item = Row<'_>> {
        std::iter::empty()
    }

    /// Subscriptions are currently evaluated using read-only transactions.
    /// Hence this will never be called on a mutable transaction.
    fn index_scan_point_for_delta(
        &self,
        _: TableId,
        _: IndexId,
        _: spacetimedb_lib::query::Delta,
        _: &AlgebraicValue,
    ) -> impl Iterator<Item = Row<'_>> {
        std::iter::empty()
    }
}

impl MutDatastore for MutTxId {
    fn insert_product_value(&mut self, table_id: TableId, row: &ProductValue) -> anyhow::Result<bool> {
        Ok(match self.insert_via_serialize_bsatn(table_id, row)?.1 {
            RowRefInsertion::Inserted(_) => true,
            RowRefInsertion::Existed(_) => false,
        })
    }

    fn delete_product_value(&mut self, table_id: TableId, row: &ProductValue) -> anyhow::Result<bool> {
        Ok(self.delete_by_row_value(table_id, row)?)
    }
}

impl MutTxId {
    /// Push a pending schema change.
    fn push_schema_change(&mut self, change: PendingSchemaChange) {
        self.tx_state.pending_schema_changes.push(change);
    }

    /// Get the list of current pending schema changes, for testing.
    #[cfg(any(test, feature = "test"))]
    pub fn pending_schema_changes(&self) -> &[PendingSchemaChange] {
        &self.tx_state.pending_schema_changes
    }

    /// Deletes all the rows in table with `table_id`
    /// where the column with `col_pos` equals `value`.
    fn delete_col_eq(&mut self, table_id: TableId, col_pos: ColId, value: &AlgebraicValue) -> Result<()> {
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

    /// Create a backing table for a view and update the system tables.
    ///
    /// Requires:
    /// - Everything [`Self::create_table`] requires.
    ///
    /// Ensures:
    /// - Everything [`Self::create_table`] ensures.
    /// - The returned [`ViewId`] is unique and not [`ViewId::SENTINEL`].
    /// - All view metadata maintained by the datastore is created atomically
    pub fn create_view(&mut self, module_def: &ModuleDef, view_def: &ViewDef) -> Result<(ViewId, TableId)> {
        let table_schema = TableSchema::from_view_def(module_def, view_def);
        let table_id = self.create_table(table_schema)?;

        let ViewDef {
            name,
            is_anonymous,
            is_public,
            params,
            return_columns,
            ..
        } = view_def;

        let view_id = self.insert_into_st_view(name.clone().into(), table_id, *is_public, *is_anonymous)?;
        self.insert_into_st_view_param(view_id, params)?;
        self.insert_into_st_view_column(view_id, return_columns)?;
        Ok((view_id, table_id))
    }

    /// Drop the backing table of a view and update the system tables.
    pub fn drop_view(&mut self, view_id: ViewId) -> Result<()> {
        // Drop the view's metadata
        self.drop_st_view(view_id)?;
        self.drop_st_view_param(view_id)?;
        self.drop_st_view_column(view_id)?;

        // Drop the view's backing table if materialized
        if let StViewRow {
            table_id: Some(table_id),
            ..
        } = self.lookup_st_view(view_id)?
        {
            return self.drop_table(table_id);
        };

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
        let matching_system_table_schema = system_tables().iter().find(|s| **s == table_schema).cloned();
        if table_schema.table_id != TableId::SENTINEL && matching_system_table_schema.is_none() {
            return Err(anyhow::anyhow!("`table_id` must be `TableId::SENTINEL` in `{table_schema:#?}`").into());
            // checks for children are performed in the relevant `create_...` functions.
        }

        let table_name = table_schema.table_name.clone();
        log::trace!("TABLE CREATING: {table_name}");

        // Insert the table row into `st_tables`
        // NOTE: Because `st_tables` has a unique index on `table_name`, this will
        // fail if the table already exists.
        let row = StTableRow {
            table_id: table_schema.table_id,
            table_name: table_name.clone(),
            table_type: table_schema.table_type,
            table_access: table_schema.table_access,
            table_primary_key: table_schema.primary_key.map(Into::into),
        };
        let table_id = self
            .insert_via_serialize_bsatn(ST_TABLE_ID, &row)?
            .1
            .collapse()
            .read_col(StTableFields::TableId)?;

        table_schema.update_table_id(table_id);

        // Generate the full definition of the table, with the generated indexes, constraints, sequences...

        // Insert the columns into `st_column`.
        self.insert_st_column(table_schema.columns())?;

        let schedule = table_schema.schedule.clone();
        let mut schema_internal = table_schema;
        // Extract all indexes, constraints, and sequences from the schema.
        // We will add them back later with correct ids.
        let (indices, sequences, constraints) = schema_internal.take_adjacent_schemas();

        // Create the in memory representation of the table
        // NOTE: This should be done before creating the indexes
        // NOTE: This `TableSchema` will be updated when we call `create_...` below.
        //       This allows us to create the indexes, constraints, and sequences with the correct `index_id`, ...
        self.create_table_internal(schema_internal.into());

        // Insert the scheduled table entry into `st_scheduled`
        if let Some(schedule) = schedule {
            let row = StScheduledRow {
                table_id: schedule.table_id,
                schedule_id: schedule.schedule_id,
                schedule_name: schedule.schedule_name,
                reducer_name: schedule.function_name,
                at_column: schedule.at_column,
            };
            let id = self
                .insert_via_serialize_bsatn(ST_SCHEDULED_ID, &row)?
                .1
                .collapse()
                .read_col::<ScheduleId>(StScheduledFields::ScheduleId)?;
            let ((table, ..), _) = self.get_or_create_insert_table_mut(table_id)?;
            table.with_mut_schema(|s| s.schedule.as_mut().unwrap().schedule_id = id);
        }

        // Create the indexes for the table.
        for index in indices {
            let col_set = ColSet::from(index.index_algorithm.columns());
            let is_unique = constraints.iter().any(|c| c.data.unique_columns() == Some(&col_set));
            self.create_index(index, is_unique)?;
        }

        // Insert constraints into `st_constraints`.
        for constraint in constraints {
            self.create_constraint(constraint)?;
        }

        // Insert sequences into `st_sequences`.
        for seq in sequences {
            self.create_sequence(seq)?;
        }

        log::trace!("TABLE CREATED: {table_name}, table_id: {table_id}");

        Ok(table_id)
    }

    /// Insert `columns` into `st_column`.
    fn insert_st_column(&mut self, columns: &[ColumnSchema]) -> Result<()> {
        columns.iter().try_for_each(|col| {
            let row: StColumnRow = col.clone().into();
            self.insert_via_serialize_bsatn(ST_COLUMN_ID, &row)?;
            Ok(())
        })
    }

    fn lookup_st_view(&self, view_id: ViewId) -> Result<StViewRow> {
        let row = self
            .iter_by_col_eq(ST_VIEW_ID, StViewFields::ViewId, &view_id.into())?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_view, view_id.into()))?;

        StViewRow::try_from(row)
    }

    /// Insert a row into `st_view`, auto-increments and returns the [`ViewId`].
    fn insert_into_st_view(
        &mut self,
        view_name: Box<str>,
        table_id: TableId,
        is_public: bool,
        is_anonymous: bool,
    ) -> Result<ViewId> {
        Ok(self
            .insert_via_serialize_bsatn(
                ST_VIEW_ID,
                &StViewRow {
                    view_id: ViewId::SENTINEL,
                    view_name,
                    table_id: Some(table_id),
                    is_public,
                    is_anonymous,
                },
            )?
            .1
            .collapse()
            .read_col(StViewFields::ViewId)?)
    }

    /// For each parameter of a view, insert a row into `st_view_param`.
    /// This does not include the context parameter.
    fn insert_into_st_view_param(&mut self, view_id: ViewId, params: &ProductType) -> Result<()> {
        for (i, field) in params.elements.iter().enumerate() {
            self.insert_via_serialize_bsatn(
                ST_VIEW_PARAM_ID,
                &StViewParamRow {
                    view_id,
                    param_pos: i.into(),
                    param_name: field
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("param_{i}").into_boxed_str()),
                    param_type: field.algebraic_type.clone().into(),
                },
            )?;
        }
        Ok(())
    }

    /// For each column or field returned in a view, insert a row into `st_view_column`.
    fn insert_into_st_view_column(&mut self, view_id: ViewId, columns: &[ViewColumnDef]) -> Result<()> {
        for def in columns {
            self.insert_via_serialize_bsatn(
                ST_VIEW_COLUMN_ID,
                &StViewColumnRow {
                    view_id,
                    col_pos: def.col_id,
                    col_name: def.name.clone().into(),
                    col_type: def.ty.clone().into(),
                },
            )?;
        }
        Ok(())
    }

    fn create_table_internal(&mut self, schema: Arc<TableSchema>) {
        // Construct the in memory tables.
        let table_id = schema.table_id;
        let commit_table = Table::new(schema, SquashedOffset::COMMITTED_STATE);
        let tx_table = commit_table.clone_structure(SquashedOffset::TX_STATE);

        // Add them to the committed and tx states.
        self.committed_state_write_lock.tables.insert(table_id, commit_table);
        self.tx_state.insert_tables.insert(table_id, tx_table);

        // Record that the committed state table is pending.
        self.push_schema_change(PendingSchemaChange::TableAdded(table_id));
    }

    fn get_row_type(&self, table_id: TableId) -> Option<&ProductType> {
        self.committed_state_write_lock
            .get_table(table_id)
            .map(|table| table.get_row_type())
    }

    pub fn row_type_for_table(&self, table_id: TableId) -> Result<RowTypeForTable<'_>> {
        // Fetch the `ProductType` from the in memory table if it exists.
        // The `ProductType` is invalidated if the schema of the table changes.
        if let Some(row_type) = self.get_row_type(table_id) {
            return Ok(RowTypeForTable::Ref(row_type));
        }

        // TODO(centril): if the table exists, this is now dead code,
        // as we will immediately insert a table into the committed state upon creation.
        // So simplify this and merge with `get_row_type`.
        //
        // Look up the columns for the table in question.
        // NOTE: This is quite an expensive operation, although we only need
        // to do this in situations where there is not currently an in memory
        // representation of a table. This would happen in situations where
        // we have created the table in the database, but have not yet
        // represented in memory or inserted any rows into it.
        Ok(RowTypeForTable::Arc(self.schema_for_table(table_id)?))
    }

    /// Drops all the columns of `table_id` in `st_column`.
    fn drop_st_column(&mut self, table_id: TableId) -> Result<()> {
        self.delete_col_eq(ST_COLUMN_ID, StColumnFields::TableId.col_id(), &table_id.into())
    }

    /// Drops the row in `st_table` for this `table_id`
    fn drop_st_table(&mut self, table_id: TableId) -> Result<()> {
        self.delete_col_eq(ST_TABLE_ID, StTableFields::TableId.col_id(), &table_id.into())
    }

    /// Drops the row in `st_view` for this `view_id`
    fn drop_st_view(&mut self, view_id: ViewId) -> Result<()> {
        self.delete_col_eq(ST_VIEW_ID, StViewFields::ViewId.col_id(), &view_id.into())
    }

    /// Drops the rows in `st_view_param` for this `view_id`
    fn drop_st_view_param(&mut self, view_id: ViewId) -> Result<()> {
        self.delete_col_eq(ST_VIEW_PARAM_ID, StViewParamFields::ViewId.col_id(), &view_id.into())
    }

    /// Drops the rows in `st_view_column` for this `view_id`
    fn drop_st_view_column(&mut self, view_id: ViewId) -> Result<()> {
        self.delete_col_eq(ST_VIEW_COLUMN_ID, StViewColumnFields::ViewId.col_id(), &view_id.into())
    }

    pub fn drop_table(&mut self, table_id: TableId) -> Result<()> {
        self.clear_table(table_id)?;

        let schema = &*self.schema_for_table(table_id)?;

        for row in &schema.indexes {
            self.drop_index(row.index_id)?;
        }

        for row in &schema.sequences {
            self.drop_sequence(row.sequence_id)?;
        }

        for row in &schema.constraints {
            self.drop_constraint(row.constraint_id)?;
        }

        // Drop the table and their columns
        self.drop_st_table(table_id)?;
        self.drop_st_column(table_id)?;

        if let Some(schedule) = &schema.schedule {
            self.delete_col_eq(
                ST_SCHEDULED_ID,
                StScheduledFields::ScheduleId.col_id(),
                &schedule.schedule_id.into(),
            )?;
        }

        // Delete the table from memory, both in the tx an committed states.
        self.tx_state.insert_tables.remove(&table_id);
        // No need to keep the delete tables.
        // By seeing `PendingSchemaChange::TableRemoved`, `merge` knows that all rows were deleted.
        self.tx_state.delete_tables.remove(&table_id);
        let commit_table = self
            .committed_state_write_lock
            .tables
            .remove(&table_id)
            .expect("there should be a schema in the committed state if we reach here");
        self.push_schema_change(PendingSchemaChange::TableRemoved(table_id, commit_table));

        Ok(())
    }

    // TODO(centril): remove this. It doesn't seem to be used by anything.
    pub fn rename_table(&mut self, table_id: TableId, new_name: &str) -> Result<()> {
        // Update the table's name in st_tables.
        self.update_st_table_row(table_id, |st| st.table_name = new_name.into())
    }

    fn update_st_table_row<R>(&mut self, table_id: TableId, updater: impl FnOnce(&mut StTableRow) -> R) -> Result<R> {
        // Fetch the row.
        let st_table_ref = self
            .iter_by_col_eq(ST_TABLE_ID, StTableFields::TableId, &table_id.into())?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_table, table_id.into()))?;
        let mut row = StTableRow::try_from(st_table_ref)?;
        let ptr = st_table_ref.pointer();

        // Delete the row, run updates, and insert again.
        self.delete(ST_TABLE_ID, ptr)?;
        let ret = updater(&mut row);
        self.insert_via_serialize_bsatn(ST_TABLE_ID, &row)?;

        Ok(ret)
    }

    pub fn view_id_from_name(&self, view_name: &str) -> Result<Option<ViewId>> {
        let view_name = &view_name.into();
        let row = self
            .iter_by_col_eq(ST_VIEW_ID, StViewFields::ViewName, view_name)?
            .next();
        Ok(row.map(|row| row.read_col(StViewFields::ViewId).unwrap()))
    }

    pub fn table_id_from_name(&self, table_name: &str) -> Result<Option<TableId>> {
        let table_name = &table_name.into();
        let row = self
            .iter_by_col_eq(ST_TABLE_ID, StTableFields::TableName, table_name)?
            .next();
        Ok(row.map(|row| row.read_col(StTableFields::TableId).unwrap()))
    }

    pub fn table_name_from_id(&self, table_id: TableId) -> Result<Option<Box<str>>> {
        self.iter_by_col_eq(ST_TABLE_ID, StTableFields::TableId, &table_id.into())
            .map(|mut iter| iter.next().map(|row| row.read_col(StTableFields::TableName).unwrap()))
    }

    /// Retrieves or creates the insert tx table for `table_id`.
    #[allow(clippy::type_complexity)]
    fn get_or_create_insert_table_mut(
        &mut self,
        table_id: TableId,
    ) -> Result<(
        TxTableForInsertion<'_>,
        (&mut Table, &mut dyn BlobStore, &mut IndexIdMap),
    )> {
        let (commit_table, commit_bs, idx_map, _) =
            self.committed_state_write_lock.get_table_and_blob_store_mut(table_id)?;
        // Get the insert table, so we can write the row into it.
        let tx = self
            .tx_state
            .get_table_and_blob_store_or_create_from(table_id, commit_table);

        let commit = (commit_table, commit_bs, idx_map);

        Ok((tx, commit))
    }
}

impl MutTxId {
    /// Set the table access of `table_id` to `access`.
    pub(crate) fn alter_table_access(&mut self, table_id: TableId, access: StAccess) -> Result<()> {
        // Write to the table in the tx state.
        let ((tx_table, ..), (commit_table, ..)) = self.get_or_create_insert_table_mut(table_id)?;
        tx_table.with_mut_schema_and_clone(commit_table, |s| s.table_access = access);

        // Update system tables.
        let old_access = self.update_st_table_row(table_id, |st| mem::replace(&mut st.table_access, access))?;

        // Remember the pending change so we can undo if necessary.
        self.push_schema_change(PendingSchemaChange::TableAlterAccess(table_id, old_access));

        Ok(())
    }

    /// Change the row type of the table identified by `table_id`.
    ///
    /// In practice, this should not error,
    /// as the update machinery should disallow any incompatible change.
    /// However, for redundancy and internal soundness of the datastore,
    /// the compatibility is also checked here.
    ///
    /// A compatible change is restricted to adding variants to the tail of a sum type
    /// while preserving the [`Layout`](spacetimedb_table::layout::Layout) of the row type.
    /// For timer/scheduler tables, the `id` an `at` columns must not change at all.
    /// All other changes, including reordering of columns, are incompatible.
    pub(crate) fn alter_table_row_type(
        &mut self,
        table_id: TableId,
        mut column_schemas: Vec<ColumnSchema>,
    ) -> Result<()> {
        // Write to the table in the tx state.
        let ((tx_table, ..), (commit_table, ..)) = self.get_or_create_insert_table_mut(table_id)?;

        // Ensure the columns have the right `table_id`.
        // NOTE(centril): This should already be done by the update machinery,
        // but do it redundantly here too, just in case.
        // This is not performance critical, so we don't care that there is overhead.
        column_schemas.iter_mut().for_each(|c| c.table_id = table_id);

        // Try to change the tables into what we want.
        let old_column_schemas = tx_table
            .change_columns_to(column_schemas.clone())
            .map_err(TableError::from)?;
        // SAFETY: `commit_table` should have a schema identical to that of `tx_table`
        // prior to changing it just now.
        unsafe { commit_table.set_layout_and_schema_to(tx_table) };

        // Update system tables.
        // We'll simply remove all rows in `st_columns` and then add the new ones.
        // The datastore takes care of not persisting any no-op delete/inserts to the commitlog.
        self.drop_st_column(table_id)?;
        self.insert_st_column(&column_schemas)?;

        // Remember the pending change so we can undo if necessary.
        self.push_schema_change(PendingSchemaChange::TableAlterRowType(table_id, old_column_schemas));

        Ok(())
    }

    /// Change the row type of the table identified by `table_id`.
    ///
    /// This is an incompatible change that requires a new table to be created.
    /// The existing rows are copied over to the new table,
    /// with `default_values` appended to each row.
    ///
    /// `column_schemas` must contain all the old columns in same order as before,
    /// followed by the new columns to be added.
    /// All new columns must have a default value in `default_values`.
    ///
    /// After calling this method, Table referred by `table_id` is dropped,
    /// and a new table is created with the same name but a new `table_id`.
    /// The new `table_id` is returned.
    pub(crate) fn add_columns_to_table(
        &mut self,
        table_id: TableId,
        column_schemas: Vec<ColumnSchema>,
        mut default_values: Vec<AlgebraicValue>,
    ) -> Result<TableId> {
        let original_table_schema: TableSchema = (*self.schema_for_table(table_id)?).clone();

        // Only keep the default values of new columns.
        let new_cols = column_schemas
            .len()
            .checked_sub(original_table_schema.columns.len())
            .ok_or_else(|| {
                anyhow::anyhow!("new column schemas must be more than existing ones for table_id: {table_id}")
            })?;
        let older_defaults = default_values.len().checked_sub(new_cols).ok_or_else(|| {
            anyhow::anyhow!("not enough default values provided for new columns for table_id: {table_id}")
        })?;
        default_values.drain(..older_defaults);

        let ((tx_table, ..), _) = self.get_or_create_insert_table_mut(table_id)?;

        tx_table
            .validate_add_columns_schema(&column_schemas, &default_values)
            .map_err(TableError::from)?;

        // Copy all rows as `ProductValue` before dropping the table.
        // This approach isn't ideal, as allocating a large contiguous block of memory
        // can lead to memory overflow errors.
        // Ideally, we should use iterators to avoid this, but that's not sufficient on its own.
        // `CommittedState::merge_apply_inserts` also allocates a similar `Vec` to hold the data.
        // So, if we really find this problematic in practice, this should be fixed in both places.
        let mut table_rows: Vec<ProductValue> = iter(&self.tx_state, &self.committed_state_write_lock, table_id)?
            .map(|r| r.to_product_value())
            .collect();

        log::debug!(
            "ADDING TABLE COLUMN (incompatible layout): {}, table_id: {}",
            original_table_schema.table_name,
            table_id
        );

        // Store sequence values to restore them later with new table.
        // Using a map from name to value as the new sequence ids will be different.
        // and I am not sure if we should rely on the order of sequences in the table schema.
        let seq_values: HashMap<Box<str>, i128> = original_table_schema
            .sequences
            .iter()
            .map(|s| {
                (
                    s.sequence_name.clone(),
                    self.sequence_state_lock
                        .get_sequence_mut(s.sequence_id)
                        .expect("sequence exists in original schema and should in sequence state.")
                        .get_value(),
                )
            })
            .collect();

        // Drop existing table first due to unique constraints on table name in `st_table`
        self.drop_table(table_id)?;

        // Update existing (dropped) table schema with provided columns, reset Ids.
        let mut updated_table_schema = original_table_schema;
        updated_table_schema.columns = column_schemas;
        updated_table_schema.reset();
        let new_table_id = self.create_table_and_update_seq(updated_table_schema, seq_values)?;

        // Populate rows with default values for new columns
        for product_value in table_rows.iter_mut() {
            let mut row_elements = product_value.elements.to_vec();
            row_elements.extend(default_values.iter().cloned());
            product_value.elements = row_elements.into();
        }

        let (new_table, tx_blob_store) = self
            .tx_state
            .get_table_and_blob_store(new_table_id)
            .ok_or(TableError::IdNotFoundState(new_table_id))?;

        for row in table_rows {
            new_table.insert(&self.committed_state_write_lock.page_pool, tx_blob_store, &row)?;
        }

        Ok(new_table_id)
    }

    fn create_table_and_update_seq(
        &mut self,
        table_schema: TableSchema,
        seq_values: HashMap<Box<str>, i128>,
    ) -> Result<TableId> {
        let table_id = self.create_table(table_schema)?;
        let table_schema = self.schema_for_table(table_id)?;

        for seq in table_schema.sequences.iter() {
            let new_seq = self
                .sequence_state_lock
                .get_sequence_mut(seq.sequence_id)
                .expect("sequence just created");
            let value = *seq_values
                .get(&seq.sequence_name)
                .ok_or_else(|| SequenceError::NotFound(seq.sequence_id))?;
            new_seq.update_value(value);
        }

        Ok(table_id)
    }

    /// Create an index.
    ///
    /// Requires:
    /// - `index.index_name` must not be used for any other database entity.
    /// - `index.index_id == IndexId::SENTINEL`
    /// - `index.table_id != TableId::SENTINEL`
    /// - `is_unique` must be `true` if and only if a unique constraint will exist on
    ///   `ColSet::from(&index.index_algorithm.columns())` after this transaction is committed.
    ///
    /// Ensures:
    /// - The index metadata is inserted into the system tables (and other data structures reflecting them).
    /// - The returned ID is unique and is not `IndexId::SENTINEL`.
    pub fn create_index(&mut self, mut index_schema: IndexSchema, is_unique: bool) -> Result<IndexId> {
        let table_id = index_schema.table_id;
        if table_id == TableId::SENTINEL {
            return Err(anyhow::anyhow!("`table_id` must not be `TableId::SENTINEL` in `{index_schema:#?}`").into());
        }

        log::trace!(
            "INDEX CREATING: {} for table: {} and algorithm: {:?}",
            index_schema.index_name,
            table_id,
            index_schema.index_algorithm
        );
        if self.table_name(table_id).is_none() {
            return Err(TableError::IdNotFoundState(table_id).into());
        }

        // Insert the index row into `st_indexes` and write back the `IndexId`.
        // NOTE: Because `st_indexes` has a unique index on `index_name`,
        // this will fail if the index already exists.
        let row: StIndexRow = index_schema.clone().into();
        let index_id = self
            .insert_via_serialize_bsatn(ST_INDEX_ID, &row)?
            .1
            .collapse()
            .read_col(StIndexFields::IndexId)?;
        index_schema.index_id = index_id;

        // Add the index to the transaction's insert table.
        let ((table, blob_store, delete_table), (commit_table, commit_blob_store, idx_map)) =
            self.get_or_create_insert_table_mut(table_id)?;

        // Create and build the indices.
        let map_violation = |violation, index: &TableIndex, table: &Table, bs: &dyn BlobStore| {
            let violation = table
                .get_row_ref(bs, violation)
                .expect("row came from scanning the table")
                .project(&index.indexed_columns)
                .expect("`cols` should consist of valid columns for this table");

            let schema = table.get_schema();
            let violation = UniqueConstraintViolation::build_with_index_schema(schema, index, &index_schema, violation);
            IndexError::from(violation).into()
        };
        // Builds the index and ensures that `table`'s row won't cause a unique constraint violation
        // due to the existing rows having the same value for some column(s).
        let build_from_rows = |index: &mut TableIndex, table: &Table, bs: &dyn BlobStore| -> Result<()> {
            let rows = table.scan_rows(bs);
            // SAFETY: (1) `tx_index` / `commit_index` was derived from `table` / `commit_table`
            // which in turn was derived from `commit_table`.
            let violation = unsafe { index.build_from_rows(rows) };
            violation.map_err(|v| map_violation(v, index, table, bs))
        };
        // Build the tx index.
        let mut tx_index = table.new_index(&index_schema.index_algorithm, is_unique)?;
        build_from_rows(&mut tx_index, table, blob_store)?;
        // Build the commit index.
        let mut commit_index = tx_index.clone_structure();
        build_from_rows(&mut commit_index, commit_table, commit_blob_store)?;
        // Make sure the two indices can be merged.
        let is_deleted = |ptr: &RowPointer| delete_table.contains(*ptr);
        commit_index
            .can_merge(&tx_index, is_deleted)
            .map_err(|v| map_violation(v, &commit_index, commit_table, commit_blob_store))?;

        log::trace!(
            "INDEX CREATED: {} for table: {} and algorithm: {:?}",
            index_id,
            table_id,
            index_schema.index_algorithm
        );

        // Associate `index_id -> table_id` for fast lookup.
        idx_map.insert(index_id, table_id);
        // SAFETY: same as (1).
        unsafe { table.add_index(index_id, tx_index) };
        let pointer_map = unsafe { commit_table.add_index(index_id, commit_index) };
        // Update the table's schema.
        table.with_mut_schema_and_clone(commit_table, |s| s.indexes.push(index_schema.clone()));
        // Note the index in pending schema changes.
        self.push_schema_change(PendingSchemaChange::IndexAdded(table_id, index_id, pointer_map));

        Ok(index_id)
    }

    pub fn drop_index(&mut self, index_id: IndexId) -> Result<()> {
        log::trace!("INDEX DROPPING: {index_id}");
        // Find the index in `st_indexes`.
        let st_index_ref = self
            .iter_by_col_eq(ST_INDEX_ID, StIndexFields::IndexId, &index_id.into())?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_index, index_id.into()))?;
        let st_index_row = StIndexRow::try_from(st_index_ref)?;
        let st_index_ptr = st_index_ref.pointer();
        let table_id = st_index_row.table_id;

        // Remove the index from st_indexes.
        self.delete(ST_INDEX_ID, st_index_ptr)?;

        // Remove the index in the transaction's insert table and the commit table.
        let ((tx_table, tx_bs, _), (commit_table, commit_bs, idx_map)) =
            self.get_or_create_insert_table_mut(table_id)?;
        tx_table.delete_index(tx_bs, index_id, None);
        let commit_index = commit_table
            .delete_index(commit_bs, index_id, None)
            .expect("there should be a schema in the committed state if we reach here");

        // Remove index from schema.
        let index_schema = commit_table
            .with_mut_schema_and_clone(tx_table, |s| s.remove_index(index_id))
            .expect("there should be an index with `index_id`");

        // Remove the `index_id -> (table_id, col_list)` association.
        idx_map.remove(&index_id);
        // Note the index in pending schema changes.
        self.push_schema_change(PendingSchemaChange::IndexRemoved(
            table_id,
            index_id,
            commit_index,
            index_schema,
        ));

        log::trace!("INDEX DROPPED: {index_id}");
        Ok(())
    }

    pub fn index_id_from_name(&self, index_name: &str) -> Result<Option<IndexId>> {
        let name = &index_name.into();
        let row = self.iter_by_col_eq(ST_INDEX_ID, StIndexFields::IndexName, name)?.next();
        Ok(row.map(|row| row.read_col(StIndexFields::IndexId).unwrap()))
    }

    /// Returns an iterator yielding rows by performing a range index scan
    /// on the range-scan-compatible index identified by `index_id`.
    ///
    /// The `prefix` is equated to the first `prefix_elems` values of the index key
    /// and then `prefix_elem`th value is bounded to the left by by `rstart`
    /// and to the right by `rend`.
    pub fn index_scan_range<'a>(
        &'a self,
        index_id: IndexId,
        prefix: &[u8],
        prefix_elems: ColId,
        rstart: &[u8],
        rend: &[u8],
    ) -> Result<(
        TableId,
        Bound<AlgebraicValue>,
        Bound<AlgebraicValue>,
        IndexScanRanged<'a>,
    )> {
        // Extract the table id, and commit/tx indices.
        let (table_id, commit_index, tx_index) = self
            .get_table_and_index(index_id)
            .ok_or_else(|| IndexError::NotFound(index_id))?;
        // Extract the index type.
        let index_ty = &commit_index.index().key_type;

        // TODO(centril): Once we have more index types than range-compatible ones,
        // we'll need to enforce that `index_id` refers to a range-compatible index.

        // We have the index key type, so we can decode everything.
        let bounds =
            Self::range_scan_decode_bounds(index_ty, prefix, prefix_elems, rstart, rend).map_err(IndexError::Decode)?;

        // Get an index seek iterator for the tx and committed state.
        let tx_iter = tx_index.map(|i| i.seek_range(&bounds));
        let commit_iter = commit_index.seek_range(&bounds);

        let (lower, upper) = bounds;

        let dt = self.tx_state.get_delete_table(table_id);
        let iter = combine_range_index_iters(dt, tx_iter, commit_iter);

        Ok((table_id, lower, upper, iter))
    }

    /// Translate `index_id` to the table id, and commit/tx indices.
    fn get_table_and_index(
        &self,
        index_id: IndexId,
    ) -> Option<(TableId, TableAndIndex<'_>, Option<TableAndIndex<'_>>)> {
        // Figure out what table the index belongs to.
        let table_id = self.committed_state_write_lock.get_table_for_index(index_id)?;

        // Find the index for the commit state.
        // If we cannot find it, there's a bug.
        let commit_index = self
            .committed_state_write_lock
            .get_index_by_id_with_table(table_id, index_id)?;

        // Find the index for the tx state, if any.
        let tx_index = self.tx_state.get_index_by_id_with_table(table_id, index_id);

        Some((table_id, commit_index, tx_index))
    }

    /// Decode the bounds for a ranged index scan for an index typed at `key_type`.
    fn range_scan_decode_bounds(
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
                let (prefix_types, rest_types) = key_types
                    .split_at_checked(prefix_elems.idx())
                    .ok_or_else(|| DecodeError::Other("index key type has too few fields compared to prefix".into()))?;

                // The `rstart` and `rend`s must be typed at `Bound<range_type>`.
                // Extract that type and determine the length of the suffix.
                let Some((range_type, suffix_types)) = rest_types.split_first() else {
                    return Err(DecodeError::Other(
                        "prefix length leaves no room for a range in ranged index scan".into(),
                    ));
                };
                let suffix_len = suffix_types.len();

                // We now have the types,
                // so proceed to decoding the prefix, and the start/end bounds.
                // Finally combine all of these to a single bound pair.
                let prefix = bsatn::decode(prefix_types, &mut prefix)?;
                let (start, end) = Self::range_scan_decode_start_end(&range_type.algebraic_type, rstart, rend)?;
                Ok(Self::range_scan_combine_prefix_and_bounds(
                    prefix, start, end, suffix_len,
                ))
            }
            // Single-column index case. We implicitly have a PT of len 1.
            _ if !prefix.is_empty() && prefix_elems.idx() != 0 => Err(DecodeError::Other(
                "a single-column index cannot be prefix scanned".into(),
            )),
            ty => Self::range_scan_decode_start_end(ty, rstart, rend),
        }
    }

    /// Decode `rstart` and `rend` as `Bound<ty>`.
    fn range_scan_decode_start_end(
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
    fn range_scan_combine_prefix_and_bounds(
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
            vals.extend(iter::repeat_n(fill, suffix_len));
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

    pub fn get_next_sequence_value(&mut self, seq_id: SequenceId) -> Result<i128> {
        get_next_sequence_value(
            &mut self.tx_state,
            &self.committed_state_write_lock,
            &mut self.sequence_state_lock,
            seq_id,
        )
    }
}

fn get_sequence_mut(seq_state: &mut SequencesState, seq_id: SequenceId) -> Result<&mut Sequence> {
    seq_state
        .get_sequence_mut(seq_id)
        .ok_or_else(|| SequenceError::NotFound(seq_id).into())
}

fn get_next_sequence_value(
    tx_state: &mut TxState,
    committed_state: &CommittedState,
    seq_state: &mut SequencesState,
    seq_id: SequenceId,
) -> Result<i128> {
    {
        let sequence = get_sequence_mut(seq_state, seq_id)?;

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
    let old_seq_row_ref = iter_by_col_eq(
        tx_state,
        committed_state,
        ST_SEQUENCE_ID,
        StSequenceFields::SequenceId,
        &seq_id.into(),
    )?
    .last()
    .unwrap();
    let old_seq_row_ptr = old_seq_row_ref.pointer();
    let seq_row = {
        let mut seq_row = StSequenceRow::try_from(old_seq_row_ref)?;

        let sequence = get_sequence_mut(seq_state, seq_id)?;
        let new_allocated = sequence.allocate_steps(SEQUENCE_ALLOCATION_STEP as usize);
        seq_row.allocated = new_allocated;
        seq_row
    };

    delete(tx_state, committed_state, ST_SEQUENCE_ID, old_seq_row_ptr)?;
    // `insert::<GENERATE = false>` rather than `GENERATE = true` because:
    // - We have already checked unique constraints during `create_sequence`.
    // - Similarly, we have already applied autoinc sequences.
    // - We do not want to apply autoinc sequences again,
    //   since the system table sequence `seq_st_table_table_id_primary_key_auto`
    //   has ID 0, and would otherwise trigger autoinc.
    with_sys_table_buf(|buf| {
        to_writer(buf, &seq_row).unwrap();
        insert::<false>(tx_state, committed_state, seq_state, ST_SEQUENCE_ID, buf)
    })?;

    get_sequence_mut(seq_state, seq_id)?
        .gen_next_value()
        .ok_or_else(|| SequenceError::UnableToAllocate(seq_id).into())
}

impl MutTxId {
    /// Create a sequence.
    /// Requires:
    /// - `seq.sequence_id == SequenceId::SENTINEL`
    /// - `seq.table_id != TableId::SENTINEL`
    /// - `seq.sequence_name` must not be used for any other database entity.
    ///
    /// Ensures:
    /// - The sequence metadata is inserted into the system tables (and other data structures reflecting them).
    /// - The returned ID is unique and not `SequenceId::SENTINEL`.
    pub fn create_sequence(&mut self, seq: SequenceSchema) -> Result<SequenceId> {
        if seq.table_id == TableId::SENTINEL {
            return Err(anyhow::anyhow!("`table_id` must not be `TableId::SENTINEL` in `{seq:#?}`").into());
        }

        let table_id = seq.table_id;
        let matching_system_table_schema = system_tables().iter().find(|s| s.table_id == table_id).cloned();

        if seq.sequence_id != SequenceId::SENTINEL && matching_system_table_schema.is_none() {
            return Err(anyhow::anyhow!("`sequence_id` must be `SequenceId::SENTINEL` in `{:#?}`", seq).into());
        }

        let sequence_id = seq.sequence_id;

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
            sequence_id,
            sequence_name: seq.sequence_name,
            table_id,
            col_pos: seq.col_pos,
            allocated: seq.start,
            increment: seq.increment,
            start: seq.start,
            min_value: seq.min_value,
            max_value: seq.max_value,
        };
        let row = self.insert_via_serialize_bsatn(ST_SEQUENCE_ID, &sequence_row)?;
        let seq_id = row.1.collapse().read_col(StSequenceFields::SequenceId)?;
        sequence_row.sequence_id = seq_id;

        let schema: SequenceSchema = sequence_row.into();
        let ((tx_table, ..), (commit_table, ..)) = self.get_or_create_insert_table_mut(table_id)?;
        // This won't clone-write when creating a table but likely to otherwise.
        tx_table.with_mut_schema_and_clone(commit_table, |s| s.update_sequence(schema.clone()));
        self.sequence_state_lock.insert(Sequence::new(schema, None));
        self.push_schema_change(PendingSchemaChange::SequenceAdded(table_id, seq_id));

        log::trace!("SEQUENCE CREATED: id = {seq_id}");

        Ok(seq_id)
    }

    pub fn drop_sequence(&mut self, sequence_id: SequenceId) -> Result<()> {
        // Ensure the sequence exists.
        let st_sequence_ref = self
            .iter_by_col_eq(ST_SEQUENCE_ID, StSequenceFields::SequenceId, &sequence_id.into())?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_sequence, sequence_id.into()))?;
        let table_id = st_sequence_ref.read_col(StSequenceFields::TableId)?;

        // Delete from system tables.
        self.delete(ST_SEQUENCE_ID, st_sequence_ref.pointer())?;

        // Drop the sequence from in-memory tables.
        let sequence = self
            .sequence_state_lock
            .remove(sequence_id)
            .expect("there should be a sequence in the committed state if we reach here");
        let ((tx_table, ..), (commit_table, ..)) = self.get_or_create_insert_table_mut(table_id)?;
        // This likely will do a clone-write as over time?
        // The schema might have found other referents.
        let schema = commit_table
            .with_mut_schema_and_clone(tx_table, |s| s.remove_sequence(sequence_id))
            .expect("there should be a schema in the committed state if we reach here");
        self.push_schema_change(PendingSchemaChange::SequenceRemoved(table_id, sequence, schema));

        Ok(())
    }

    pub fn sequence_id_from_name(&self, seq_name: &str) -> Result<Option<SequenceId>> {
        let name = &<Box<str>>::from(seq_name).into();
        self.iter_by_col_eq(ST_SEQUENCE_ID, StSequenceFields::SequenceName, name)
            .map(|mut iter| {
                iter.next()
                    .map(|row| row.read_col(StSequenceFields::SequenceId).unwrap())
            })
    }

    /// Create a constraint.
    ///
    /// Requires:
    /// - `constraint.constraint_name` must not be used for any other database entity.
    /// - `constraint.constraint_id == ConstraintId::SENTINEL`
    /// - `constraint.table_id != TableId::SENTINEL`
    /// - `is_unique` must be `true` if and only if a unique constraint will exist on
    ///   `ColSet::from(&constraint.constraint_algorithm.columns())` after this transaction is committed.
    ///
    /// Ensures:
    /// - The constraint metadata is inserted into the system tables (and other data structures reflecting them).
    /// - The returned ID is unique and is not `constraintId::SENTINEL`.
    fn create_constraint(&mut self, mut constraint: ConstraintSchema) -> Result<ConstraintId> {
        if constraint.table_id == TableId::SENTINEL {
            return Err(anyhow::anyhow!("`table_id` must not be `TableId::SENTINEL` in `{constraint:#?}`").into());
        }

        let table_id = constraint.table_id;

        log::trace!(
            "CONSTRAINT CREATING: {} for table: {} and data: {:?}",
            constraint.constraint_name,
            table_id,
            constraint.data
        );

        // Insert the constraint row into `st_constraint`.
        // NOTE: Because `st_constraint` has a unique index on constraint_name,
        // this will fail if the table already exists.
        let constraint_row = StConstraintRow {
            table_id,
            constraint_id: constraint.constraint_id,
            constraint_name: constraint.constraint_name.clone(),
            constraint_data: constraint.data.clone().into(),
        };

        let constraint_row = self.insert_via_serialize_bsatn(ST_CONSTRAINT_ID, &constraint_row)?;
        let constraint_id = constraint_row.1.collapse().read_col(StConstraintFields::ConstraintId)?;
        if let RowRefInsertion::Existed(_) = constraint_row.1 {
            log::trace!("CONSTRAINT ALREADY EXISTS: {constraint_id}");
            return Ok(constraint_id);
        }

        let ((tx_table, ..), (commit_table, ..)) = self.get_or_create_insert_table_mut(table_id)?;
        constraint.constraint_id = constraint_id;
        // This won't clone-write when creating a table but likely to otherwise.
        tx_table.with_mut_schema_and_clone(commit_table, |s| s.update_constraint(constraint.clone()));
        self.push_schema_change(PendingSchemaChange::ConstraintAdded(table_id, constraint_id));

        log::trace!("CONSTRAINT CREATED: {constraint_id}");
        Ok(constraint_id)
    }

    pub fn drop_constraint(&mut self, constraint_id: ConstraintId) -> Result<()> {
        // Delete row in `st_constraint`.
        let st_constraint_ref = self
            .iter_by_col_eq(
                ST_CONSTRAINT_ID,
                StConstraintFields::ConstraintId,
                &constraint_id.into(),
            )?
            .next()
            .ok_or_else(|| TableError::IdNotFound(SystemTable::st_constraint, constraint_id.into()))?;
        let table_id = st_constraint_ref.read_col(StConstraintFields::TableId)?;
        self.delete(ST_CONSTRAINT_ID, st_constraint_ref.pointer())?;

        // Remove constraint in transaction's insert table.
        let ((tx_table, ..), (commit_table, ..)) = self.get_or_create_insert_table_mut(table_id)?;
        // This likely will do a clone-write as over time?
        // The schema might have found other referents.
        let schema = commit_table
            .with_mut_schema_and_clone(tx_table, |s| s.remove_constraint(constraint_id))
            .expect("there should be a schema in the committed state if we reach here");
        self.push_schema_change(PendingSchemaChange::ConstraintRemoved(table_id, schema));
        // TODO(1.0): we should also re-initialize `table` without a unique constraint.
        // unless some other unique constraint on the same columns exists.
        // NOTE(centril): is this already handled by dropping the corresponding index?
        // Probably not in the case where an index
        // with the same name goes from being unique to not unique.

        Ok(())
    }

    pub fn constraint_id_from_name(&self, constraint_name: &str) -> Result<Option<ConstraintId>> {
        self.iter_by_col_eq(
            ST_CONSTRAINT_ID,
            StConstraintFields::ConstraintName,
            &<Box<str>>::from(constraint_name).into(),
        )
        .map(|mut iter| {
            iter.next()
                .map(|row| row.read_col(StConstraintFields::ConstraintId).unwrap())
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
        if row_level_security_schema.table_id == TableId::SENTINEL {
            return Err(anyhow::anyhow!(
                "`table_id` must not be `TableId::SENTINEL` in `{row_level_security_schema:#?}`"
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

        let row = self.insert_via_serialize_bsatn(ST_ROW_LEVEL_SECURITY_ID, &row)?;
        let row_level_security_sql = row.1.collapse().read_col(StRowLevelSecurityFields::Sql)?;
        let existed = matches!(row.1, RowRefInsertion::Existed(_));

        // Add the row level security to the transaction's insert table.
        self.get_or_create_insert_table_mut(row_level_security_schema.table_id)?;

        if existed {
            log::trace!("ROW LEVEL SECURITY ALREADY EXISTS: {row_level_security_sql}");
        } else {
            log::trace!("ROW LEVEL SECURITY CREATED: {row_level_security_sql}");
        }

        Ok(row_level_security_sql)
    }

    pub fn row_level_security_for_table_id(&self, table_id: TableId) -> Result<Vec<RowLevelSecuritySchema>> {
        Ok(self
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
    }

    pub fn drop_row_level_security(&mut self, sql: RawSql) -> Result<()> {
        let st_rls_ref = self
            .iter_by_col_eq(
                ST_ROW_LEVEL_SECURITY_ID,
                StRowLevelSecurityFields::Sql,
                &sql.clone().into(),
            )?
            .next()
            .ok_or(TableError::RawSqlNotFound(SystemTable::st_row_level_security, sql))?;
        self.delete(ST_ROW_LEVEL_SECURITY_ID, st_rls_ref.pointer())?;

        Ok(())
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

    /// Commits this transaction in memory, applying its changes to the committed state.
    /// This doesn't handle the persistence layer at all.
    ///
    /// Returns:
    /// - [`TxData`], the set of inserts and deletes performed by this transaction.
    /// - [`TxMetrics`], various measurements of the work performed by this transaction.
    /// - `String`, the name of the reducer which ran during this transaction.
    pub(super) fn commit(mut self) -> (TxOffset, TxData, TxMetrics, String) {
        let tx_offset = self.committed_state_write_lock.next_tx_offset;
        let tx_data = self
            .committed_state_write_lock
            .merge(self.tx_state, self.read_sets, &self.ctx);

        // Compute and keep enough info that we can
        // record metrics after the transaction has ended
        // and after the lock has been dropped.
        // Recording metrics when holding the lock is too expensive.
        let tx_metrics = TxMetrics::new(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            self.metrics,
            true,
            Some(&tx_data),
            &self.committed_state_write_lock,
        );
        let reducer = self.ctx.into_reducer_name();

        // If the transaction didn't consume an offset (i.e. it was empty),
        // report the previous offset.
        //
        // Note that technically the tx could have run against an empty database,
        // in which case we'd wrongly return zero (a non-existent transaction).
        // This doesn not happen in practice, however, as [RelationalDB::set_initialized]
        // creates a transaction.
        let tx_offset = if tx_offset == self.committed_state_write_lock.next_tx_offset {
            tx_offset.saturating_sub(1)
        } else {
            tx_offset
        };

        (tx_offset, tx_data, tx_metrics, reducer)
    }

    /// Commits this transaction, applying its changes to the committed state.
    /// The lock on the committed state is converted into a read lock,
    /// and returned as a new read-only transaction.
    ///
    /// Returns:
    /// - [`TxData`], the set of inserts and deletes performed by this transaction.
    /// - [`TxMetrics`], various measurements of the work performed by this transaction.
    /// - [`TxId`], a read-only transaction with a shared lock on the committed state.
    pub fn commit_downgrade(mut self, workload: Workload) -> (TxData, TxMetrics, TxId) {
        let tx_data = self
            .committed_state_write_lock
            .merge(self.tx_state, self.read_sets, &self.ctx);

        // Compute and keep enough info that we can
        // record metrics after the transaction has ended
        // and after the lock has been dropped.
        // Recording metrics when holding the lock is too expensive.
        let tx_metrics = TxMetrics::new(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            self.metrics,
            true,
            Some(&tx_data),
            &self.committed_state_write_lock,
        );

        // Update the workload type of the execution context
        self.ctx.workload = workload.workload_type();
        let tx = TxId {
            committed_state_shared_lock: SharedWriteGuard::downgrade(self.committed_state_write_lock),
            lock_wait_time: Duration::ZERO,
            timer: Instant::now(),
            ctx: self.ctx,
            metrics: ExecutionMetrics::default(),
        };
        (tx_data, tx_metrics, tx)
    }

    /// Rolls back this transaction, discarding its changes.
    ///
    /// Returns:
    /// - [`TxMetrics`], various measurements of the work performed by this transaction.
    /// - `String`, the name of the reducer which ran during this transaction.
    pub fn rollback(mut self) -> (TxOffset, TxMetrics, String) {
        let offset = self
            .committed_state_write_lock
            .rollback(&mut self.sequence_state_lock, self.tx_state);

        // Compute and keep enough info that we can
        // record metrics after the transaction has ended
        // and after the lock has been dropped.
        // Recording metrics when holding the lock is too expensive.
        let tx_metrics = TxMetrics::new(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            self.metrics,
            true,
            None,
            &self.committed_state_write_lock,
        );
        let reducer = self.ctx.into_reducer_name();
        (offset, tx_metrics, reducer)
    }

    /// Roll back this transaction, discarding its changes.
    /// The lock on the committed state is converted into a read lock,
    /// and returned as a new read-only transaction.
    ///
    /// Returns:
    /// - [`TxMetrics`], various measurements of the work performed by this transaction.
    /// - [`TxId`], a read-only transaction with a shared lock on the committed state.
    pub fn rollback_downgrade(mut self, workload: Workload) -> (TxMetrics, TxId) {
        self.committed_state_write_lock
            .rollback(&mut self.sequence_state_lock, self.tx_state);

        // Compute and keep enough info that we can
        // record metrics after the transaction has ended
        // and after the lock has been dropped.
        // Recording metrics when holding the lock is too expensive.
        let tx_metrics = TxMetrics::new(
            &self.ctx,
            self.timer,
            self.lock_wait_time,
            self.metrics,
            true,
            None,
            &self.committed_state_write_lock,
        );

        // Update the workload type of the execution context
        self.ctx.workload = workload.workload_type();
        let tx = TxId {
            committed_state_shared_lock: SharedWriteGuard::downgrade(self.committed_state_write_lock),
            lock_wait_time: Duration::ZERO,
            timer: Instant::now(),
            ctx: self.ctx,
            metrics: ExecutionMetrics::default(),
        };

        (tx_metrics, tx)
    }
}

/// Either a row just inserted to a table or a row that already existed in some table.
#[derive(Clone, Copy)]
pub enum RowRefInsertion<'a> {
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

/// The iterator returned by [`MutTxId::index_scan_range`].
pub struct IndexScanRanged<'a> {
    inner: IndexScanRangedInner<'a>,
}

enum IndexScanRangedInner<'a> {
    CommitOnly(IndexScanRangeIter<'a>),
    CommitOnlyWithDeletes(FilterDeleted<'a, IndexScanRangeIter<'a>>),
    Both(iter::Chain<IndexScanRangeIter<'a>, IndexScanRangeIter<'a>>),
    BothWithDeletes(iter::Chain<IndexScanRangeIter<'a>, FilterDeleted<'a, IndexScanRangeIter<'a>>>),
}

pub(super) struct FilterDeleted<'a, I> {
    pub(super) iter: I,
    pub(super) deletes: &'a DeleteTable,
}

impl<'a> Iterator for IndexScanRanged<'a> {
    type Item = RowRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            IndexScanRangedInner::CommitOnly(it) => it.next(),
            IndexScanRangedInner::CommitOnlyWithDeletes(it) => it.next(),
            IndexScanRangedInner::Both(it) => it.next(),
            IndexScanRangedInner::BothWithDeletes(it) => it.next(),
        }
    }
}

impl<'a, I: Iterator<Item = RowRef<'a>>> Iterator for FilterDeleted<'a, I> {
    type Item = RowRef<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.find(|row| !self.deletes.contains(row.pointer()))
    }
}

impl MutTxId {
    pub fn insert_st_client(
        &mut self,
        identity: Identity,
        connection_id: ConnectionId,
        jwt_payload: &str,
    ) -> Result<()> {
        let row = &StClientRow {
            identity: identity.into(),
            connection_id: connection_id.into(),
        };
        self.insert_via_serialize_bsatn(ST_CLIENT_ID, row)
            .map(|_| ())
            .inspect_err(|e| {
                log::error!(
                    "[{identity}]: insert_st_client: failed to insert client ({identity}, {connection_id}), error: {e}"
                );
            })?;
        self.insert_st_client_credentials(connection_id, jwt_payload)
    }

    fn insert_st_client_credentials(&mut self, connection_id: ConnectionId, jwt_payload: &str) -> Result<()> {
        let row = &StConnectionCredentialsRow {
            connection_id: connection_id.into(),
            jwt_payload: jwt_payload.to_owned(),
        };
        self.insert_via_serialize_bsatn(ST_CONNECTION_CREDENTIALS_ID, row)
            .map(|_| ())
            .inspect_err(|e| {
                log::error!("[{connection_id}]: insert_st_client_credentials: failed to insert client credentials for connection id ({connection_id}), error: {e}");
            })
    }

    fn delete_st_client_credentials(&mut self, database_identity: Identity, connection_id: ConnectionId) -> Result<()> {
        if let Err(e) = self.delete_col_eq(
            ST_CONNECTION_CREDENTIALS_ID,
            StConnectionCredentialsFields::ConnectionId.col_id(),
            &ConnectionIdViaU128::from(connection_id).into(),
        ) {
            // This is possible on restart if the database was previously running a version
            // before this system table was added.
            log::error!("[{database_identity}]: delete_st_client_credentials: attempting to delete credentials for missing connection id ({connection_id}), error: {e}");
        }
        Ok(())
    }

    pub fn delete_st_client(
        &mut self,
        identity: Identity,
        connection_id: ConnectionId,
        database_identity: Identity,
    ) -> Result<()> {
        let row = &StClientRow {
            identity: identity.into(),
            connection_id: connection_id.into(),
        };
        if let Some(ptr) = self
            .iter_by_col_eq(
                ST_CLIENT_ID,
                // TODO(perf, minor, centril): consider a `const_col_list([x, ..])`
                // so we know this is not computed at runtime.
                col_list![StClientFields::Identity, StClientFields::ConnectionId],
                &AlgebraicValue::product(row),
            )?
            .next()
            .map(|row| row.pointer())
        {
            self.delete(ST_CLIENT_ID, ptr).map(drop)?
        } else {
            log::error!("[{database_identity}]: delete_st_client: attempting to delete client ({identity}, {connection_id}), but no st_client row for that client is resident");
        }
        self.delete_st_client_credentials(database_identity, connection_id)
    }

    /// Look up a client row by identity and connection ID in the `st_clients` system table.
    pub fn st_client_row(&self, identity: Identity, connection_id: ConnectionId) -> Option<RowPointer> {
        let row = StClientRow {
            identity: identity.into(),
            connection_id: connection_id.into(),
        };

        self.iter_by_col_eq(
            ST_CLIENT_ID,
            col_list![StClientFields::Identity, StClientFields::ConnectionId],
            &AlgebraicValue::product(row),
        )
        .expect("failed to read from st_client system table")
        .next()
        .map(|row| row.pointer())
    }

    pub fn insert_via_serialize_bsatn<'a, T: Serialize>(
        &'a mut self,
        table_id: TableId,
        row: &T,
    ) -> Result<(ColList, RowRefInsertion<'a>, InsertFlags)> {
        thread_local! {
            static BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        }
        BUF.with_borrow_mut(|buf| {
            buf.clear();
            to_writer(buf, row).unwrap();
            self.insert::<true>(table_id, buf)
        })
    }

    /// Insert a row, encoded in BSATN, into a table.
    ///
    /// Zero placeholders, i.e., sequence triggers,
    /// in auto-inc columns in the new row will be replaced with generated values
    /// if and only if `GENERATE` is true.
    /// This method is called with `GENERATE` false when updating the `st_sequence` system table.
    ///
    /// Requires:
    /// - `table_id` must refer to a valid table for the database at `database_identity`.
    /// - `row` must be a valid row for the table at `table_id`.
    ///
    /// Returns:
    /// - a list of columns which have been replaced with generated values.
    /// - a ref to the inserted row.
    /// - any insert flags.
    pub(super) fn insert<const GENERATE: bool>(
        &mut self,
        table_id: TableId,
        row: &[u8],
    ) -> Result<(ColList, RowRefInsertion<'_>, InsertFlags)> {
        insert::<GENERATE>(
            &mut self.tx_state,
            &self.committed_state_write_lock,
            &mut self.sequence_state_lock,
            table_id,
            row,
        )
    }
}

/// Insert a row, encoded in BSATN, into a table.
///
/// Zero placeholders, i.e., sequence triggers,
/// in auto-inc columns in the new row will be replaced with generated values
/// if and only if `GENERATE` is true.
/// This method is called with `GENERATE` false when updating the `st_sequence` system table.
///
/// Requires:
/// - `table_id` must refer to a valid table for the database at `database_identity`.
/// - `row` must be a valid row for the table at `table_id`.
///
/// Returns:
/// - list of columns which have been replaced with generated values.
/// - a pointer to the inserted row.
/// - the number of bytes added to the tx blob store.
/// - The "tx table for insertion" for further processing.
/// - The "commit table for insertion" for further processing.
fn insert_physically_maybe_generate<'a, const GENERATE: bool>(
    tx_state: &'a mut TxState,
    committed_state: &'a CommittedState,
    seq_state: &mut SequencesState,
    table_id: TableId,
    row: &[u8],
) -> Result<(
    RowPointer,
    ColList,
    BlobNumBytes,
    TxTableForInsertion<'a>,
    CommitTableForInsertion<'a>,
)> {
    // Get commit table and friends.
    let commit_parts = committed_state.get_table_and_blob_store(table_id)?;
    let (commit_table, ..) = commit_parts;

    // Get the insert table, so we can write the row into it.
    let (tx_table, tx_blob_store, _) = tx_state.get_table_and_blob_store_or_create_from(table_id, commit_table);

    // 1. Insert the physical row.
    let page_pool = &committed_state.page_pool;
    let (tx_row_ref, blob_bytes) = tx_table.insert_physically_bsatn(page_pool, tx_blob_store, row)?;
    let tx_row_ptr = tx_row_ref.pointer();
    // 2. Optionally: Detect, generate, write sequence values.
    let (tx_parts, gen_cols) = if GENERATE {
        // When `GENERATE` is enabled, we're instructed to deal with sequence value generation.
        // Collect all the columns with sequences that need generation.
        let (cols_to_gen, seqs_to_use) = unsafe { tx_table.sequence_triggers_for(tx_blob_store, tx_row_ptr) };

        // Generate a value for every column in the row that needs it.
        let mut seq_vals: SmallVec<[i128; 1]> = <_>::default();
        for sequence_id in seqs_to_use {
            seq_vals.push(get_next_sequence_value(
                tx_state,
                committed_state,
                seq_state,
                sequence_id,
            )?);
        }

        // Write the generated values to the physical row at `tx_row_ptr`.
        // We assume here that column with a sequence is of a sequence-compatible type.
        // SAFETY: After `get_table_and_blob_store_or_create_from` there's a insert and delete table.
        let (tx_table, tx_blob_store, delete_table) = unsafe { tx_state.assume_present_get_mut_table(table_id) };
        for (col_id, seq_val) in cols_to_gen.iter().zip(seq_vals) {
            // SAFETY:
            // - `self.is_row_present(row)` holds as we haven't deleted the row.
            // - `col_id` is a valid column, and has a sequence, so it must have a primitive type.
            unsafe { tx_table.write_gen_val_to_col(col_id, tx_row_ptr, seq_val) };
        }

        ((tx_table, tx_blob_store, delete_table), cols_to_gen)
    } else {
        // When `GENERATE` is not enabled, avoid sequence generation.
        // This branch is hit when inside sequence generation itself, to avoid infinite recursion.
        // SAFETY: After `get_table_and_blob_store_or_create_from` there's a insert and delete table.
        let tx_parts = unsafe { tx_state.assume_present_get_mut_table(table_id) };
        (tx_parts, ColList::empty())
    };

    Ok((tx_row_ptr, gen_cols, blob_bytes, tx_parts, commit_parts))
}

/// Insert a row, encoded in BSATN, into a table.
///
/// Zero placeholders, i.e., sequence triggers,
/// in auto-inc columns in the new row will be replaced with generated values
/// if and only if `GENERATE` is true.
/// This method is called with `GENERATE` false when updating the `st_sequence` system table.
///
/// Requires:
/// - `table_id` must refer to a valid table for the database at `database_identity`.
/// - `row` must be a valid row for the table at `table_id`.
///
/// Returns:
/// - a list of columns which have been replaced with generated values.
/// - a ref to the inserted row.
/// - any insert flags.
pub(super) fn insert<'a, const GENERATE: bool>(
    tx_state: &'a mut TxState,
    committed_state: &'a CommittedState,
    seq_state: &mut SequencesState,
    table_id: TableId,
    row: &[u8],
) -> Result<(ColList, RowRefInsertion<'a>, InsertFlags)> {
    let (
        tx_row_ptr,
        gen_cols,
        blob_bytes,
        (tx_table, tx_blob_store, delete_table),
        (commit_table, commit_blob_store, _),
    ) = insert_physically_maybe_generate::<GENERATE>(tx_state, committed_state, seq_state, table_id, row)?;

    let insert_flags = InsertFlags {
        is_scheduler_table: tx_table.is_scheduler(),
    };
    let ok = |row_ref| Ok((gen_cols, row_ref, insert_flags));

    // `CHECK_SAME_ROW = true`, as there might be an identical row already in the tx state.
    // SAFETY: `tx_table.is_row_present(row)` holds as we still haven't deleted the row,
    // in particular, the `write_gen_val_to_col` call does not remove the row.
    let res = unsafe { tx_table.confirm_insertion::<true>(tx_blob_store, tx_row_ptr, blob_bytes) };

    match res {
        Ok((tx_row_hash, tx_row_ptr)) => {
            // The `tx_row_ref` was not previously present in insert tables,
            // but may still be a set-semantic conflict
            // or may violate a unique constraint with a row in the committed state.
            // We'll check the set-semantic aspect in (1) and the constraint in (2).

            // (1) Rule out a set-semantic conflict with the committed state.
            // SAFETY:
            // - `commit_table` and `tx_table` use the same schema
            //   because `tx_table` is derived from `commit_table`.
            // - `tx_row_ptr` is correct per post-condition of `tx_table.confirm_insertion(...)`.
            if let (_, Some(commit_ptr)) =
                unsafe { Table::find_same_row(commit_table, tx_table, tx_blob_store, tx_row_ptr, tx_row_hash) }
            {
                // (insert_undelete)
                // -----------------------------------------------------
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
                    .delete(tx_blob_store, tx_row_ptr, |_| ())
                    .expect("Failed to delete a row we just inserted");

                // It's possible that `row` appears in the committed state,
                // but is marked as deleted.
                // In this case, undelete it, so it remains in the committed state.
                delete_table.remove(commit_ptr);

                // No new row was inserted, but return `committed_ptr`.
                // SAFETY: `find_same_row` told us that `ptr` refers to a valid row in `commit_table`.
                let row_ref = unsafe { commit_table.get_row_ref_unchecked(commit_blob_store, commit_ptr) };
                return ok(RowRefInsertion::Existed(row_ref));
            }

            // Pacify the borrow checker.
            // SAFETY: `tx_row_ptr` is still correct for `tx_table` per (PC.INS.1).
            // as there haven't been any interleaving `&mut` calls that could invalidate the pointer.
            let tx_row_ref = unsafe { tx_table.get_row_ref_unchecked(tx_blob_store, tx_row_ptr) };

            // (2) The `tx_row_ref` did not violate a unique constraint *within* the `tx_table`,
            // but it could do so wrt., `commit_table`,
            // assuming the conflicting row hasn't been deleted since.
            // Ensure that it doesn't, or roll back the insertion.
            let is_deleted = |commit_ptr| delete_table.contains(commit_ptr);
            // SAFETY: `commit_table.row_layout() == tx_row_ref.row_layout()` holds
            // as the `tx_table` is derived from `commit_table`.
            let res = unsafe { commit_table.check_unique_constraints(tx_row_ref, |ixs| ixs, is_deleted) };
            if let Err(e) = res {
                // There was a constraint violation, so undo the insertion.
                tx_table.delete(tx_blob_store, tx_row_ptr, |_| {});
                return Err(IndexError::from(e).into());
            }

            // SAFETY: `tx_row_ptr` is still correct for `tx_table` per (PC.INS.1).
            // as there haven't been any interleaving `&mut` calls that could invalidate the pointer.
            let row_ref = unsafe { tx_table.get_row_ref_unchecked(tx_blob_store, tx_row_ptr) };
            ok(RowRefInsertion::Inserted(row_ref))
        }
        // `row` previously present in insert tables; do nothing but return `ptr`.
        Err(InsertError::Duplicate(DuplicateError(ptr))) => {
            // SAFETY: `tx_table` told us that `ptr` refers to a valid row in it.
            let row_ref = unsafe { tx_table.get_row_ref_unchecked(tx_blob_store, ptr) };
            ok(RowRefInsertion::Existed(row_ref))
        }
        // Unwrap these error into `TableError::{IndexError, Bflatn}`:
        Err(InsertError::IndexError(e)) => Err(IndexError::from(e).into()),
        Err(InsertError::Bflatn(e)) => Err(TableError::Bflatn(e).into()),
    }
}

impl MutTxId {
    /// Update a row, encoded in BSATN, into a table.
    ///
    /// Zero placeholders, i.e., sequence triggers,
    /// in auto-inc columns in the new row will be replaced with generated values.
    ///
    /// The old row is found by projecting `row` to the columns of `index_id`.
    ///
    /// Requires:
    /// - `table_id` must refer to a valid table for the database at `database_identity`.
    /// - `index_id` must refer to a valid index in the table.
    /// - `row` must be a valid row for the table at `table_id`.
    ///
    /// Returns:
    /// - a list of columns which have been replaced with generated values.
    /// - a ref to the new row.
    /// - any update flags.
    pub(crate) fn update(
        &mut self,
        table_id: TableId,
        index_id: IndexId,
        row: &[u8],
    ) -> Result<(ColList, RowRefInsertion<'_>, UpdateFlags)> {
        // Insert the physical row into the tx insert table
        // and possibly generate sequence values.
        //
        // As we are provided the `row` encoded in BSATN,
        // and since we don't have a convenient way to BSATN to a set of columns,
        // we cannot really do an in-place update in the row-was-in-tx-state case.
        // So we will begin instead by inserting the row physically to the tx state and project that.
        let (
            tx_row_ptr,
            cols_to_gen,
            blob_bytes,
            (tx_table, tx_blob_store, del_table),
            (commit_table, commit_blob_store, _),
        ) = insert_physically_maybe_generate::<true>(
            &mut self.tx_state,
            &self.committed_state_write_lock,
            &mut self.sequence_state_lock,
            table_id,
            row,
        )?;

        let update_flags = UpdateFlags {
            is_scheduler_table: tx_table.is_scheduler(),
        };
        let ok = |row_ref| Ok((cols_to_gen, row_ref, update_flags));

        // SAFETY: `tx_table.is_row_present(tx_row_ptr)` holds as we just inserted it.
        let tx_row_ref = unsafe { tx_table.get_row_ref_unchecked(tx_blob_store, tx_row_ptr) };

        let err = 'error: {
            // This macros can be thought of as a `throw $e` within `'error`.
            // TODO(centril): Get rid of this once we have stable `try` blocks or polonius.
            macro_rules! throw {
                ($e:expr) => {
                    break 'error $e.into()
                };
            }

            // Check that the index exists and is unique.
            // It's sufficient to check the committed state.
            let Some(commit_index) = commit_table.get_index_by_id(index_id) else {
                throw!(IndexError::NotFound(index_id));
            };
            if !commit_index.is_unique() {
                throw!(IndexError::NotUnique(index_id));
            }

            // Project the row to the index's type.
            // SAFETY: `tx_row_ref`'s table is derived from `commit_index`'s table,
            // so all `index.indexed_columns` will be in-bounds of the row layout.
            let index_key = unsafe { tx_row_ref.project_unchecked(&commit_index.indexed_columns) };

            // Try to find the old row first in the committed state using the `index_key`.
            let mut old_commit_del_ptr = None;
            let commit_old_ptr = commit_index.seek_point(&index_key).next().filter(|&ptr| {
                // Was committed row previously deleted in this TX?
                let deleted = del_table.contains(ptr);
                // If so, remember it in case it was identical to the new row.
                old_commit_del_ptr = deleted.then_some(ptr);
                !deleted
            });

            // Ensure that the new row does not violate other commit table unique constraints.
            let is_deleted = |commit_ptr| {
                commit_old_ptr.is_some_and(|old_ptr| old_ptr == commit_ptr) || del_table.contains(commit_ptr)
            };
            // SAFETY: `commit_table.row_layout() == new_row.row_layout()` holds
            // as the `tx_table` is derived from `commit_table`.
            if let Err(e) = unsafe {
                commit_table.check_unique_constraints(
                    tx_row_ref,
                    // Don't check this index since we'll do a 1-1 old/new replacement.
                    |ixs| ixs.filter(|(&id, _)| id != index_id),
                    is_deleted,
                )
            } {
                throw!(IndexError::from(e));
            }

            let tx_row_ptr = if let Some(old_ptr) = commit_old_ptr {
                // Row was found in the committed state!
                //
                // If the new row is the same as the old,
                // skip the update altogether to match the semantics of `Self::insert`.
                //
                // SAFETY:
                // 1. `tx_table` is derived from `commit_table` so they have the same layouts.
                // 2. `old_ptr` was found in an index of `commit_table`, so we know it is valid.
                // 3. we just inserted `tx_row_ptr` into `tx_table`, so we know it is valid.
                if unsafe { Table::eq_row_in_page(commit_table, old_ptr, tx_table, tx_row_ptr) } {
                    // SAFETY: `tx_table.is_row_present(tx_row_ptr)` holds, as noted in 3.
                    unsafe { tx_table.delete_internal_skip_pointer_map(tx_blob_store, tx_row_ptr) };
                    // SAFETY: `commit_table.is_row_present(old_ptr)` holds, as noted in 2.
                    let row_ref = unsafe { commit_table.get_row_ref_unchecked(commit_blob_store, old_ptr) };
                    return ok(RowRefInsertion::Existed(row_ref));
                }

                // Check constraints and confirm the insertion of the new row.
                //
                // `CHECK_SAME_ROW = false`,
                // as we know there's a row (`old_ptr`) in the committed state with,
                // for columns `C`, a unique value X.
                // For `row` to be identical to another row in the tx state,
                // it must have the value `X` for `C`,
                // but it cannot, as the committed state already has `X` for `C`.
                // So we don't need to check the tx state for a duplicate row.
                //
                // SAFETY: `tx_table.is_row_present(row)` holds as we still haven't deleted the row,
                // in particular, the `write_gen_val_to_col` call does not remove the row.
                // On error, `tx_row_ptr` has already been removed, so don't do it again.
                let (_, tx_row_ptr) =
                    unsafe { tx_table.confirm_insertion::<false>(tx_blob_store, tx_row_ptr, blob_bytes) }?;

                // Delete the old row.
                del_table.insert(old_ptr);
                tx_row_ptr
            } else if let Some(old_ptr) = tx_table
                .get_index_by_id(index_id)
                .and_then(|index| index.seek_point(&index_key).next())
            {
                // Row was found in the tx state!
                //
                // Check constraints and confirm the update of the new row.
                // This ensures that the old row is removed from the indices
                // before attempting to insert the new row into the indices.
                //
                // SAFETY: `tx_table.is_row_present(tx_row_ptr)` and `tx_table.is_row_present(old_ptr)` both hold
                // as we've deleted neither.
                // In particular, the `write_gen_val_to_col` call does not remove the row.
                let tx_row_ptr = unsafe { tx_table.confirm_update(tx_blob_store, tx_row_ptr, old_ptr, blob_bytes) }?;

                if let Some(old_commit_del_ptr) = old_commit_del_ptr {
                    // If we have an identical deleted row in the committed state,
                    // we need to undelete it, just like in `Self::insert`.
                    // The same note (`insert_undelete`) there re. MVCC applies here as well.
                    //
                    // SAFETY:
                    // 1. `tx_table` is derived from `commit_table` so they have the same layouts.
                    // 2. `old_commit_del_ptr` was found in an index of `commit_table`.
                    // 3. we just inserted `tx_row_ptr` into `tx_table`, so we know it is valid.
                    if unsafe { Table::eq_row_in_page(commit_table, old_commit_del_ptr, tx_table, tx_row_ptr) } {
                        // It is important that we `confirm_update` first,
                        // as we must ensure that undeleting the row causes no tx state conflict.
                        tx_table
                            .delete(tx_blob_store, tx_row_ptr, |_| ())
                            .expect("Failed to delete a row we just inserted");

                        // Undelete.
                        del_table.remove(old_commit_del_ptr);

                        // Return the undeleted committed state row.
                        // SAFETY: `commit_table.is_row_present(old_commit_del_ptr)` holds.
                        let row_ref =
                            unsafe { commit_table.get_row_ref_unchecked(commit_blob_store, old_commit_del_ptr) };
                        return ok(RowRefInsertion::Existed(row_ref));
                    }
                }

                tx_row_ptr
            } else {
                throw!(IndexError::KeyNotFound(index_id, index_key));
            };

            // SAFETY: `tx_table.is_row_present(tx_row_ptr)` holds
            // per post-condition of `confirm_insertion` and `confirm_update`
            // in the if/else branches respectively.
            let row_ref = unsafe { tx_table.get_row_ref_unchecked(tx_blob_store, tx_row_ptr) };
            return ok(RowRefInsertion::Inserted(row_ref));
        };

        // When we reach here, we had an error and we need to revert the insertion of `tx_row_ref`.
        // SAFETY: `tx_table.is_row_present(tx_row_ptr)` holds,
        // as we still haven't deleted the row physically.
        unsafe { tx_table.delete_internal_skip_pointer_map(tx_blob_store, tx_row_ptr) };
        Err(err)
    }

    pub(super) fn delete(&mut self, table_id: TableId, row_pointer: RowPointer) -> Result<bool> {
        delete(
            &mut self.tx_state,
            &self.committed_state_write_lock,
            table_id,
            row_pointer,
        )
    }

    // Clears the table for `table_id`, removing all rows.
    pub fn clear_table(&mut self, table_id: TableId) -> Result<usize> {
        // Get the commit table.
        let (commit_table, commit_bs, ..) = self.committed_state_write_lock.get_table_and_blob_store(table_id)?;

        // Get the insert table and delete all rows from it.
        let (tx_table, tx_blob_store, delete_table) = self
            .tx_state
            .get_table_and_blob_store_or_create_from(table_id, commit_table);
        let mut rows_removed = tx_table.clear(tx_blob_store);

        // Mark every row in the committed state as deleted.
        for row in commit_table.scan_rows(commit_bs) {
            delete_table.insert(row.pointer());
            rows_removed += 1;
        }

        Ok(rows_removed)
    }
}

pub(super) fn delete(
    tx_state: &mut TxState,
    committed_state: &CommittedState,
    table_id: TableId,
    row_pointer: RowPointer,
) -> Result<bool> {
    match row_pointer.squashed_offset() {
        // For newly-inserted rows,
        // just delete them from the insert tables
        // - there's no reason to have them in both the insert and delete tables.
        SquashedOffset::TX_STATE => {
            let (table, blob_store) = tx_state
                .get_table_and_blob_store(table_id)
                .ok_or(TableError::IdNotFoundState(table_id))?;
            Ok(table.delete(blob_store, row_pointer, |_| ()).is_some())
        }
        SquashedOffset::COMMITTED_STATE => {
            let commit_table = committed_state
                .get_table(table_id)
                .expect("there's a row in committed state so there should be a committed table");
            // NOTE: We trust the `row_pointer` refers to an extant row.
            // It could have been deleted already in this transaction,
            // in which case we don't want to return that we deleted it.
            let deleted = tx_state
                .get_delete_table_mut(table_id, commit_table)
                .insert(row_pointer);
            Ok(deleted)
        }
        _ => unreachable!("Invalid SquashedOffset for RowPointer: {:?}", row_pointer),
    }
}

impl MutTxId {
    pub(super) fn delete_by_row_value(&mut self, table_id: TableId, rel: &ProductValue) -> Result<bool> {
        // Get commit table and page pool.
        let page_pool = &self.committed_state_write_lock.page_pool;
        let (commit_table, ..) = self.committed_state_write_lock.get_table_and_blob_store(table_id)?;

        // Temporarily insert the row into the tx insert table.
        let (tx_table, tx_blob_store, _) = self
            .tx_state
            .get_table_and_blob_store_or_create_from(table_id, commit_table);

        // We only want to physically insert the row here to get a row pointer.
        // We'd like to avoid any set semantic and unique constraint checks.
        let (temp_row_ref, _) = tx_table.insert_physically_pv(page_pool, tx_blob_store, rel)?;
        let temp_ptr = temp_row_ref.pointer();

        // First, check if a matching row exists in the `commit_table`.
        // If it does, no need to check the `tx_table`.
        //
        // We start with `commit_table` as, in most cases,
        // we'll likely have a transaction that deletes a committed row
        // rather than deleting a row that was inserted in the same transaction.
        //
        // SAFETY:
        // - `commit_table` and `tx_table` use the same schema.
        // - `temp_ptr` is valid because we just inserted it.
        let (hash, to_delete) = unsafe { Table::find_same_row(commit_table, tx_table, tx_blob_store, temp_ptr, None) };
        let to_delete = to_delete
            // Not present in commit table? Check if present in the tx table.
            .or_else(|| {
                // SAFETY:
                // - `tx_table` and `tx_table` trivially use the same schema.
                // - `temp_ptr` is valid because we just inserted it.
                let (_, to_delete) = unsafe { Table::find_same_row(tx_table, tx_table, tx_blob_store, temp_ptr, hash) };
                to_delete
            });

        // Remove the temporary entry from the tx table.
        // Do this before actually deleting to drop the borrows on the table.
        // SAFETY: `temp_ptr` is valid because we just inserted it and haven't deleted it since.
        unsafe {
            tx_table.delete_internal_skip_pointer_map(tx_blob_store, temp_ptr);
        }

        // Delete the found row either by marking (commit table)
        // or by deleting directly (tx table).
        to_delete
            .map(|to_delete| self.delete(table_id, to_delete))
            .unwrap_or(Ok(false))
    }
}

impl StateView for MutTxId {
    type Iter<'a> = IterMutTx<'a>;
    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>> = IterByColRangeMutTx<'a, R>;
    type IterByColEq<'a, 'r>
        = IterByColEqMutTx<'a, 'r>
    where
        Self: 'a;

    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>> {
        // TODO(bikeshedding, docs): should this also check if the schema is in the system tables,
        // but the table hasn't been constructed yet?
        // If not, document why.

        // No need to check the tx state.
        // If the table is not in the committed state, it doesn't exist.
        self.committed_state_write_lock.get_schema(table_id)
    }

    fn table_row_count(&self, table_id: TableId) -> Option<u64> {
        table_row_count(&self.tx_state, &self.committed_state_write_lock, table_id)
    }

    fn iter(&self, table_id: TableId) -> Result<Self::Iter<'_>> {
        iter(&self.tx_state, &self.committed_state_write_lock, table_id)
    }

    fn iter_by_col_range<R: RangeBounds<AlgebraicValue>>(
        &self,
        table_id: TableId,
        cols: ColList,
        range: R,
    ) -> Result<Self::IterByColRange<'_, R>> {
        iter_by_col_range(&self.tx_state, &self.committed_state_write_lock, table_id, cols, range)
    }

    fn iter_by_col_eq<'r>(
        &self,
        table_id: TableId,
        cols: impl Into<ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'_, 'r>> {
        iter_by_col_eq(&self.tx_state, &self.committed_state_write_lock, table_id, cols, value)
    }
}

fn table_row_count(tx_state: &TxState, committed_state: &CommittedState, table_id: TableId) -> Option<u64> {
    let commit_count = committed_state.table_row_count(table_id);
    let (tx_ins_count, tx_del_count) = tx_state.table_row_count(table_id);
    let commit_count = commit_count.map(|cc| cc - tx_del_count);
    // Keep track of whether `table_id` exists.
    match (commit_count, tx_ins_count) {
        (Some(cc), Some(ic)) => Some(cc + ic),
        (Some(c), None) | (None, Some(c)) => Some(c),
        (None, None) => None,
    }
}

fn iter<'a>(tx_state: &'a TxState, committed_state: &'a CommittedState, table_id: TableId) -> Result<IterMutTx<'a>> {
    IterMutTx::new(table_id, tx_state, committed_state)
}

fn iter_by_col_range<'a, R: RangeBounds<AlgebraicValue>>(
    tx_state: &'a TxState,
    committed_state: &'a CommittedState,
    table_id: TableId,
    cols: ColList,
    range: R,
) -> Result<IterByColRangeMutTx<'a, R>> {
    // If there's an index, use that.
    // It's sufficient to check that the committed state has an index
    // as index schema changes are applied immediately.
    if let Some(commit_iter) = committed_state.index_seek(table_id, &cols, &range) {
        let tx_iter = tx_state.index_seek_by_cols(table_id, &cols, &range);
        let delete_table = tx_state.get_delete_table(table_id);
        let iter = combine_range_index_iters(delete_table, tx_iter, commit_iter);
        Ok(IterByColRangeMutTx::Index(iter))
    } else {
        unindexed_iter_by_col_range_warn(tx_state, committed_state, table_id, &cols);
        let iter = iter(tx_state, committed_state, table_id)?;

        Ok(IterByColRangeMutTx::Scan(ScanIterByColRangeMutTx::new(
            iter, cols, range,
        )))
    }
}

fn iter_by_col_eq<'a, 'r>(
    tx_state: &'a TxState,
    committed_state: &'a CommittedState,
    table_id: TableId,
    cols: impl Into<ColList>,
    value: &'r AlgebraicValue,
) -> Result<IterByColEqMutTx<'a, 'r>> {
    iter_by_col_range(tx_state, committed_state, table_id, cols.into(), value)
}

fn combine_range_index_iters<'a>(
    delete_table: Option<&'a DeleteTable>,
    tx_iter: Option<IndexScanRangeIter<'a>>,
    commit_iter: IndexScanRangeIter<'a>,
) -> IndexScanRanged<'a> {
    // Chain together the indexed rows in the tx and committed state,
    // but don't yield rows deleted in the tx state.
    use itertools::Either::*;
    use IndexScanRangedInner::*;
    let commit_iter = match delete_table {
        None => Left(commit_iter),
        Some(deletes) => Right(FilterDeleted {
            iter: commit_iter,
            deletes,
        }),
    };
    // This is effectively just `tx_iter.into_iter().flatten().chain(commit_iter)`,
    // but with all the branching and `Option`s flattened to just one layer.
    let iter = match (tx_iter, commit_iter) {
        (None, Left(commit_iter)) => CommitOnly(commit_iter),
        (None, Right(commit_iter)) => CommitOnlyWithDeletes(commit_iter),
        (Some(tx_iter), Left(commit_iter)) => Both(tx_iter.chain(commit_iter)),
        (Some(tx_iter), Right(commit_iter)) => BothWithDeletes(tx_iter.chain(commit_iter)),
    };
    IndexScanRanged { inner: iter }
}

#[cfg(not(feature = "unindexed_iter_by_col_range_warn"))]
fn unindexed_iter_by_col_range_warn(_: &TxState, _: &CommittedState, _: TableId, _: &ColList) {}

#[cfg(feature = "unindexed_iter_by_col_range_warn")]
fn unindexed_iter_by_col_range_warn(
    tx_state: &TxState,
    committed_state: &CommittedState,
    table_id: TableId,
    cols: &ColList,
) {
    match table_row_count(tx_state, committed_state, table_id) {
        // TODO(ux): log these warnings to the module logs rather than host logs.
        None => log::error!("iter_by_col_range on unindexed column, but couldn't fetch table `{table_id}`s row count",),
        Some(num_rows) => {
            const TOO_MANY_ROWS_FOR_SCAN: u64 = 1000;
            if num_rows >= TOO_MANY_ROWS_FOR_SCAN {
                let schema = committed_state.get_schema(table_id).unwrap();
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
}
