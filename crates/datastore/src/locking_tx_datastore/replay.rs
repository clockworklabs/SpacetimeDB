use super::committed_state::CommittedState;
use super::datastore::Result;
use crate::db_metrics::DB_METRICS;
use crate::error::{IndexError, TableError};
use crate::locking_tx_datastore::datastore::ReplayError;
use crate::locking_tx_datastore::state_view::iter_st_column_for_table;
use crate::locking_tx_datastore::state_view::StateView;
use crate::system_tables::is_built_in_meta_row;
use crate::system_tables::{StColumnRow, StTableFields, StTableRow, ST_COLUMN_ID, ST_TABLE_ID};
use anyhow::{anyhow, Context};
use core::ops::{Deref, DerefMut};
use parking_lot::{RwLock, RwLockReadGuard};
use spacetimedb_commitlog::payload::txdata;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_lib::Identity;
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_sats::algebraic_value::de::ValueDeserializer;
use spacetimedb_sats::buffer::BufReader;
use spacetimedb_sats::{AlgebraicValue, Deserialize, ProductValue};
use spacetimedb_schema::schema::{ColumnSchema, TableSchema};
use spacetimedb_schema::table_name::TableName;
use spacetimedb_table::indexes::RowPointer;
use spacetimedb_table::table::{InsertError, RowRef};
use std::cell::RefCell;
use std::sync::Arc;

/// A [`spacetimedb_commitlog::Decoder`] suitable for replaying a transaction
/// history into the database state.
pub struct Replay<F> {
    pub(super) database_identity: Identity,
    pub(super) committed_state: Arc<RwLock<CommittedState>>,
    pub(super) progress: RefCell<F>,
    pub(super) error_behavior: ErrorBehavior,
}

impl<F> Replay<F> {
    fn using_visitor<T>(&self, f: impl FnOnce(&mut ReplayVisitor<'_, F>) -> T) -> T {
        let mut committed_state = self.committed_state.write();
        let state = &mut *committed_state;
        let committed_state = ReplayCommittedState { state };
        let mut visitor = ReplayVisitor {
            database_identity: &self.database_identity,
            committed_state,
            progress: &mut *self.progress.borrow_mut(),
            dropped_table_names: IntMap::default(),
            error_behavior: self.error_behavior,
        };
        f(&mut visitor)
    }

    pub fn next_tx_offset(&self) -> u64 {
        self.committed_state.read_arc().next_tx_offset
    }

    pub fn committed_state(&self) -> RwLockReadGuard<'_, CommittedState> {
        self.committed_state.read()
    }
}

impl<F: FnMut(u64)> spacetimedb_commitlog::Decoder for &mut Replay<F> {
    type Record = txdata::Txdata<ProductValue>;
    type Error = txdata::DecoderError<ReplayError>;

    fn decode_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        tx_offset: u64,
        reader: &mut R,
    ) -> std::result::Result<Self::Record, Self::Error> {
        self.using_visitor(|visitor| txdata::decode_record_fn(visitor, version, tx_offset, reader))
    }

    fn consume_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        tx_offset: u64,
        reader: &mut R,
    ) -> std::result::Result<(), Self::Error> {
        self.using_visitor(|visitor| txdata::consume_record_fn(visitor, version, tx_offset, reader))
    }

    fn skip_record<'a, R: BufReader<'a>>(
        &self,
        version: u8,
        _tx_offset: u64,
        reader: &mut R,
    ) -> std::result::Result<(), Self::Error> {
        self.using_visitor(|visitor| txdata::skip_record_fn(visitor, version, reader))
    }
}

// n.b. (Tyler) We actually **do not** want to check constraints at replay
// time because not only is it a pain, but actually **subtly wrong** the
// way we have it implemented. It's wrong because the actual constraints of
// the database may change as different transactions are added to the
// schema and you would actually have to change your indexes and
// constraints as you replayed the log. This we are not currently doing
// (we're building all the non-bootstrapped indexes at the end after
// replaying), and thus aren't implementing constraint checking correctly
// as it stands.
//
// However, the above is all rendered moot anyway because we don't need to
// check constraints while replaying if we just assume that they were all
// checked prior to the transaction committing in the first place.
//
// Note also that operation/mutation ordering **does not** matter for
// operations inside a transaction of the message log assuming we only ever
// insert **OR** delete a unique row in one transaction. If we ever insert
// **AND** delete then order **does** matter. The issue caused by checking
// constraints for each operation while replaying does not imply that order
// matters. Ordering of operations would **only** matter if you wanted to
// view the state of the database as of a partially applied transaction. We
// never actually want to do this, because after a transaction has been
// committed, it is assumed that all operations happen instantaneously and
// atomically at the timestamp of the transaction. The only time that we
// actually want to view the state of a database while a transaction is
// partially applied is while the transaction is running **before** it
// commits. Thus, we only care about operation ordering while the
// transaction is running, but we do not care about it at all in the
// context of the commit log.
//
// Not caring about the order in the log, however, requires that we **do
// not** check index constraints during replay of transaction operations.
// We **could** check them in between transactions if we wanted to update
// the indexes and constraints as they changed during replay, but that is
// unnecessary.

/// What to do when encountering an error during commitlog replay due to an invalid TX.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ErrorBehavior {
    /// Return an error and refuse to continue.
    ///
    /// This is the behavior in production, as we don't want to reconstruct an incorrect state.
    FailFast,
    /// Log a warning and continue replay.
    ///
    /// This behavior is used when inspecting broken commitlogs during debugging.
    Warn,
}

struct ReplayVisitor<'a, F> {
    database_identity: &'a Identity,
    committed_state: ReplayCommittedState<'a>,
    progress: &'a mut F,
    // Since deletes are handled before truncation / drop, sometimes the schema
    // info is gone. We save the name on the first delete of that table so metrics
    // can still show a name.
    dropped_table_names: IntMap<TableId, TableName>,
    error_behavior: ErrorBehavior,
}

impl<F> ReplayVisitor<'_, F> {
    /// Process `err` according to `self.error_behavior`,
    /// either warning about it or returning it.
    ///
    /// If this method returns an `Err`, the caller should bubble up that error with `?`.
    fn process_error(&self, err: ReplayError) -> std::result::Result<(), ReplayError> {
        match self.error_behavior {
            ErrorBehavior::FailFast => Err(err),
            ErrorBehavior::Warn => {
                log::warn!("{err:?}");
                Ok(())
            }
        }
    }
}

impl<F: FnMut(u64)> spacetimedb_commitlog::payload::txdata::Visitor for ReplayVisitor<'_, F> {
    type Error = ReplayError;
    // NOTE: Technically, this could be `()` if and when we can extract the
    // row data without going through `ProductValue` (PV).
    // To accommodate auxiliary traversals (e.g. for analytics), we may want to
    // provide a separate visitor yielding PVs.
    type Row = ProductValue;

    fn skip_row<'a, R: BufReader<'a>>(
        &mut self,
        table_id: TableId,
        reader: &mut R,
    ) -> std::result::Result<(), Self::Error> {
        let schema = self.committed_state.schema_for_table(table_id)?;
        ProductValue::decode(schema.get_row_type(), reader)?;
        Ok(())
    }

    fn visit_insert<'a, R: BufReader<'a>>(
        &mut self,
        table_id: TableId,
        reader: &mut R,
    ) -> std::result::Result<Self::Row, Self::Error> {
        let schema = self.committed_state.schema_for_table(table_id)?;
        let row = ProductValue::decode(schema.get_row_type(), reader)?;

        if let Err(e) = self
            .committed_state
            .replay_insert(table_id, &schema, &row)
            .with_context(|| {
                format!(
                    "Error inserting row {:?} during transaction {:?} playback",
                    row, self.committed_state.next_tx_offset
                )
            })
        {
            self.process_error(e.into())?;
        }
        // NOTE: the `rdb_num_table_rows` metric is used by the query optimizer,
        // and therefore has performance implications and must not be disabled.
        DB_METRICS
            .rdb_num_table_rows
            .with_label_values(self.database_identity, &table_id.into(), &schema.table_name)
            .inc();

        Ok(row)
    }

    fn visit_delete<'a, R: BufReader<'a>>(
        &mut self,
        table_id: TableId,
        reader: &mut R,
    ) -> std::result::Result<Self::Row, Self::Error> {
        let schema = self.committed_state.schema_for_table(table_id)?;
        // TODO: avoid clone
        let table_name = schema.table_name.clone();
        let row = ProductValue::decode(schema.get_row_type(), reader)?;

        // If this is a delete from the `st_table` system table, save the name
        if table_id == ST_TABLE_ID {
            let ab = AlgebraicValue::Product(row.clone());
            let st_table_row = StTableRow::deserialize(ValueDeserializer::from_ref(&ab)).unwrap();
            self.dropped_table_names
                .insert(st_table_row.table_id, st_table_row.table_name);
        }

        if let Err(e) = self
            .committed_state
            .replay_delete_by_rel(table_id, &row)
            .with_context(|| {
                format!(
                    "Error deleting row {:?} from table {:?} during transaction {:?} playback",
                    row, table_name, self.committed_state.next_tx_offset
                )
            })
        {
            self.process_error(e.into())?;
        }
        // NOTE: the `rdb_num_table_rows` metric is used by the query optimizer,
        // and therefore has performance implications and must not be disabled.
        DB_METRICS
            .rdb_num_table_rows
            .with_label_values(self.database_identity, &table_id.into(), &table_name)
            .dec();

        Ok(row)
    }

    fn visit_truncate(&mut self, table_id: TableId) -> std::result::Result<(), Self::Error> {
        let table_name = match self.committed_state.schema_for_table(table_id) {
            // TODO: avoid clone
            Ok(schema) => schema.table_name.clone(),

            Err(_) => match self.dropped_table_names.remove(&table_id) {
                Some(name) => name,
                _ => {
                    return self
                        .process_error(anyhow!("Error looking up name for truncated table {table_id:?}").into());
                }
            },
        };

        if let Err(e) = self.committed_state.replay_truncate(table_id).with_context(|| {
            format!(
                "Error truncating table {:?} during transaction {:?} playback",
                table_id, self.committed_state.next_tx_offset
            )
        }) {
            self.process_error(e.into())?;
        }

        // NOTE: the `rdb_num_table_rows` metric is used by the query optimizer,
        // and therefore has performance implications and must not be disabled.
        DB_METRICS
            .rdb_num_table_rows
            .with_label_values(self.database_identity, &table_id.into(), &table_name)
            .set(0);

        Ok(())
    }

    fn visit_tx_start(&mut self, offset: u64) -> std::result::Result<(), Self::Error> {
        // The first transaction in a history must have the same offset as the
        // committed state.
        //
        // (Technically, the history should guarantee that all subsequent
        // transaction offsets are contiguous, but we don't currently have a
        // good way to only check the first transaction.)
        //
        // Note that this is not a panic, because the starting offset can be
        // chosen at runtime.
        if offset != self.committed_state.next_tx_offset {
            return Err(ReplayError::InvalidOffset {
                expected: self.committed_state.next_tx_offset,
                encountered: offset,
            });
        }
        (self.progress)(offset);

        Ok(())
    }

    fn visit_tx_end(&mut self) -> std::result::Result<(), Self::Error> {
        self.committed_state.replay_end_tx().map_err(Into::into)
    }
}

/// A `CommittedState` under construction during replay.
struct ReplayCommittedState<'cs> {
    /// The committed state being contructed.
    state: &'cs mut CommittedState,
}

impl Deref for ReplayCommittedState<'_> {
    type Target = CommittedState;

    fn deref(&self) -> &Self::Target {
        self.state
    }
}

impl DerefMut for ReplayCommittedState<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.state
    }
}

impl ReplayCommittedState<'_> {
    pub(super) fn replay_insert(
        &mut self,
        table_id: TableId,
        schema: &Arc<TableSchema>,
        row: &ProductValue,
    ) -> Result<()> {
        // Event table rows in the commitlog are preserved for future replay features
        // but don't rebuild state — event tables have no committed state.
        if schema.is_event {
            return Ok(());
        }

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
        let target_table_id = CommittedState::read_table_id(st_column_row);
        let target_col_id = ColId::deserialize(ValueDeserializer::from_ref(&st_column_row.elements[1]))
            .expect("second field in `st_column` should decode to a `ColId`");

        let outdated_st_column_rows = iter_st_column_for_table(self.state, &target_table_id.into())?
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
        let table_name = self.find_st_table_row(table_id)?.table_name;

        // We're replaying and we don't have unique constraints yet.
        // Due to replay handling all inserts first and deletes after,
        // when processing `st_column` insert/deletes,
        // we may end up with two definitions for the same `col_pos`.
        // Of those two, we're interested in the one we just inserted
        // and not the other one, as it is being replaced.
        // `Self::ignore_previous_version_of_column` has marked the old version as ignored,
        // so filter only the non-ignored columns.
        let mut columns = iter_st_column_for_table(self.state, &table_id.into())?
            .filter(|row_ref| !self.replay_columns_to_ignore.contains(&row_ref.pointer()))
            .map(|row_ref| {
                let row = StColumnRow::try_from(row_ref)?;
                let mut column_schema = ColumnSchema::from(row);
                let alias = self
                    .find_st_column_accessor_row(table_name.as_ref(), &column_schema.col_name)?
                    .map(|row| row.accessor_name);
                column_schema.alias = alias;
                Ok(column_schema)
            })
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

    fn replay_end_tx(&mut self) -> Result<()> {
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
}

#[cfg(test)]
mod tests {
    use crate::{
        locking_tx_datastore::datastore::tests::{all_rows, begin_mut_tx, commit, setup_event_table, u32_str_u32},
        traits::{MutTx as _, MutTxDatastore as _},
    };
    use spacetimedb_lib::error::ResultTest;

    use super::*;

    #[test]
    fn test_event_table_replay_ignores_inserts() -> ResultTest<()> {
        let (datastore, tx, table_id) = setup_event_table()?;
        // Commit the table-creation tx so the schema exists.
        commit(&datastore, tx)?;

        // Get the schema for this event table.
        let tx = begin_mut_tx(&datastore);
        let schema = datastore.schema_for_table_mut_tx(&tx, table_id)?;
        let _ = datastore.rollback_mut_tx(tx);

        // Directly call replay_insert on committed state.
        let row = u32_str_u32(1, "Carol", 40);
        {
            let state = &mut *datastore.committed_state.write();
            let mut committed_state = ReplayCommittedState { state };
            committed_state.replay_insert(table_id, &schema, &row)?;
        }

        // After replay, the event table should still have no committed rows.
        let tx = begin_mut_tx(&datastore);
        assert_eq!(
            all_rows(&datastore, &tx, table_id).len(),
            0,
            "replay_insert should be a no-op for event tables"
        );
        Ok(())
    }
}
