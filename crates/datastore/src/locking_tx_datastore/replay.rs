use super::committed_state::CommittedState;
use super::datastore::Result;
use crate::db_metrics::DB_METRICS;
use crate::error::{DatastoreError, ViewError};
use crate::locking_tx_datastore::datastore::Locking;
use crate::locking_tx_datastore::replay::ErrorBehavior::FailFast;
use crate::locking_tx_datastore::state_view::iter_st_column_for_table;
use crate::system_tables::{
    table_id_is_reserved, StConstraintData, StConstraintRow, StIndexRow, StSequenceFields, StViewRow, ST_CONSTRAINT_ID,
    ST_INDEX_ID, ST_SEQUENCE_ID, ST_VIEW_ARG_ID, ST_VIEW_ID, ST_VIEW_SUB_ID,
};
use crate::{
    error::{IndexError, TableError},
    locking_tx_datastore::state_view::StateView,
    system_tables::{
        is_built_in_meta_row, StColumnRow, StFields as _, StTableFields, StTableRow, ST_COLUMN_ID, ST_TABLE_ID,
    },
};
use anyhow::{anyhow, Context};
use core::cell::RefCell;
use parking_lot::RwLockWriteGuard;
use prometheus::core::{AtomicF64, GenericGauge};
use prometheus::IntGauge;
use spacetimedb_commitlog::payload::{txdata, Txdata};
use spacetimedb_data_structures::map::{HashMap, HashSet, IntMap, IntSet};
use spacetimedb_lib::Identity;
use spacetimedb_primitives::{ColId, ColSet, SequenceId, TableId};
use spacetimedb_sats::buffer::BufReader;
use spacetimedb_sats::memory_usage::MemoryUsage;
use spacetimedb_sats::{algebraic_value::de::ValueDeserializer, Deserialize as _, ProductValue};
use spacetimedb_sats::{bsatn, AlgebraicValue};
use spacetimedb_schema::def::IndexAlgorithm;
use spacetimedb_schema::schema::{ColumnSchema, TableSchema};
use spacetimedb_table::{
    indexes::RowPointer,
    table::{InsertError, RowRef},
};
use std::{ops::RangeBounds, sync::Arc};
use thiserror::Error;

pub fn apply_history(
    datastore: &Locking,
    database_identity: Identity,
    history: impl spacetimedb_durability::History<TxData = Txdata<ProductValue>>,
    counters: ApplyHistoryCounters,
) -> Result<()> {
    log::info!("[{database_identity}] DATABASE: applying transaction history...");

    // TODO: Revisit once we actually replay history suffixes, ie. starting
    // from an offset larger than the history's min offset.
    // TODO: We may want to require that a `tokio::runtime::Handle` is
    // always supplied when constructing a `RelationalDB`. This would allow
    // to spawn a timer task here which just prints the progress periodically
    // in case the history is finite but very long.
    let (_, max_tx_offset) = history.tx_range_hint();
    let mut last_logged_percentage = 0;
    let progress = |tx_offset: u64| {
        if let Some(max_tx_offset) = max_tx_offset {
            let percentage = f64::floor((tx_offset as f64 / max_tx_offset as f64) * 100.0) as i32;
            if percentage > last_logged_percentage && percentage % 10 == 0 {
                log::info!("[{database_identity}] Loaded {percentage}% ({tx_offset}/{max_tx_offset})");
                last_logged_percentage = percentage;
            }
        // Print _something_ even if we don't know what's still ahead.
        } else if tx_offset.is_multiple_of(10_000) {
            log::info!("[{database_identity}] Loading transaction {tx_offset}");
        }
    };

    let time_before = std::time::Instant::now();

    let mut replay = datastore.replay(
        progress,
        // We don't want to instantiate an incorrect state;
        // if the commitlog contains an inconsistency we'd rather get a hard error than showing customers incorrect data.
        FailFast,
    );
    let mut replay = replay.borrow_mut();
    let start_tx_offset = replay.next_tx_offset();
    history
        .fold_transactions_from(start_tx_offset, &mut replay)
        .map_err(anyhow::Error::from)?;
    let time_elapsed = time_before.elapsed();
    counters.replay_commitlog_time_seconds.set(time_elapsed.as_secs_f64());
    let end_tx_offset = replay.next_tx_offset();
    counters
        .replay_commitlog_num_commits
        .set((end_tx_offset - start_tx_offset) as _);
    log::info!("[{database_identity}] DATABASE: applied transaction history");

    replay
        .committed_state
        .borrow_mut()
        .rebuild_state_after_replay(datastore)?;
    log::info!("[{database_identity}] DATABASE: rebuilt state after replay");

    Ok(())
}

pub struct ApplyHistoryCounters {
    pub replay_commitlog_time_seconds: GenericGauge<AtomicF64>,
    pub replay_commitlog_num_commits: IntGauge,
}

#[derive(Debug, Error)]
pub enum ReplayError {
    #[error("Expected tx offset {expected}, encountered {encountered}")]
    InvalidOffset { expected: u64, encountered: u64 },
    #[error(transparent)]
    Decode(#[from] bsatn::DecodeError),
    #[error(transparent)]
    Db(#[from] DatastoreError),
    #[error(transparent)]
    Any(#[from] anyhow::Error),
}

pub struct ReplayGuard<'locking, F> {
    database_identity: Identity,
    committed_state: RwLockWriteGuard<'locking, CommittedState>,
    progress: F,
    error_behavior: ErrorBehavior,
}

impl<'locking, F> ReplayGuard<'locking, F> {
    pub fn new(
        database_identity: Identity,
        committed_state: RwLockWriteGuard<'locking, CommittedState>,
        progress: F,
        error_behavior: ErrorBehavior,
    ) -> Self {
        Self {
            database_identity,
            committed_state,
            progress,
            error_behavior,
        }
    }

    pub fn borrow_mut<'a>(&'a mut self) -> Replay<'a, F> {
        let committed_state = &mut *self.committed_state;
        let committed_state = RefCell::new(ReplayCommittedState::new(committed_state));
        let progress = RefCell::new(&mut self.progress);
        Replay {
            database_identity: self.database_identity,
            committed_state,
            progress,
            error_behavior: self.error_behavior,
        }
    }
}

/// A [`spacetimedb_commitlog::Decoder`] suitable for replaying a transaction
/// history into the database state.
pub struct Replay<'a, F> {
    database_identity: Identity,
    committed_state: RefCell<ReplayCommittedState<'a>>,
    progress: RefCell<&'a mut F>,
    error_behavior: ErrorBehavior,
}

impl<F> Replay<'_, F> {
    fn using_visitor<T>(&self, f: impl FnOnce(&mut ReplayVisitor<'_, '_, F>) -> T) -> T {
        let mut visitor = ReplayVisitor {
            database_identity: &self.database_identity,
            committed_state: &mut self.committed_state.borrow_mut(),
            progress: &mut **self.progress.borrow_mut(),
            dropped_table_names: IntMap::default(),
            error_behavior: self.error_behavior,
        };
        f(&mut visitor)
    }

    pub fn next_tx_offset(&self) -> u64 {
        self.committed_state.borrow().next_tx_offset()
    }
}

impl<F: FnMut(u64)> spacetimedb_commitlog::Decoder for &mut Replay<'_, F> {
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

struct ReplayVisitor<'a, 'cs, F> {
    database_identity: &'a Identity,
    committed_state: &'a mut ReplayCommittedState<'cs>,
    progress: &'a mut F,
    // Since deletes are handled before truncation / drop, sometimes the schema
    // info is gone. We save the name on the first delete of that table so metrics
    // can still show a name.
    dropped_table_names: IntMap<TableId, Box<str>>,
    error_behavior: ErrorBehavior,
}

impl<F> ReplayVisitor<'_, '_, F> {
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

impl<F: FnMut(u64)> spacetimedb_commitlog::payload::txdata::Visitor for ReplayVisitor<'_, '_, F> {
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
                    row,
                    self.committed_state.next_tx_offset()
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
                    row,
                    table_name,
                    self.committed_state.next_tx_offset()
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

            Err(_) => {
                if let Some(name) = self.dropped_table_names.remove(&table_id) {
                    name
                } else {
                    return self
                        .process_error(anyhow!("Error looking up name for truncated table {table_id:?}").into());
                }
            }
        };

        if let Err(e) = self.committed_state.replay_truncate(table_id).with_context(|| {
            format!(
                "Error truncating table {:?} during transaction {:?} playback",
                table_id,
                self.committed_state.next_tx_offset()
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
        let next_tx_offset = self.committed_state.next_tx_offset();
        if offset != next_tx_offset {
            return Err(ReplayError::InvalidOffset {
                expected: next_tx_offset,
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
    pub(super) state: &'cs mut CommittedState,

    /// Whether the table was dropped within the current transaction during replay.
    ///
    /// While processing a transaction which drops a table, we'll first see the `st_table` delete,
    /// then a series of deletes from the table itself.
    /// We track the table's ID here so we know to ignore the deletes.
    ///
    /// Cleared after the end of processing each transaction,
    /// as it should be impossible to ever see another reference to the table after that point.
    replay_table_dropped: IntSet<TableId>,

    /// Rows within `st_column` which should be ignored during replay
    /// due to having been superseded by a new row representing the same column.
    ///
    /// During replay, we visit all of the inserts table-by-table, followed by all of the deletes table-by-table.
    /// This means that, when multiple columns of a table change type within the same transaction,
    /// we see all of the newly-inserted `st_column` rows first, and then later, all of the deleted rows.
    /// We may even see inserts into the altered table before seeing the `st_column` deletes!
    ///
    /// In order to maintain a proper view of the schema of tables during replay,
    /// we must remember the old versions of the `st_column` rows when we insert the new ones,
    /// so that we can respect only the new versions.
    ///
    /// We insert into this set during [`Self::replay_insert`] of `st_column` rows
    /// and delete from it during [`Self::replay_delete`] of `st_column` rows.
    /// We assert this is empty at the end of each transaction.
    replay_columns_to_ignore: HashSet<RowPointer>,

    /// Set of tables whose `st_table` entries have been updated during the currently-replaying transaction,
    /// mapped to the current most-recent `st_table` row.
    ///
    /// When processing an insert to `st_table`, if the table already exists, we'll record it here.
    /// Then, when we see a corresponding delete, we know that the table has not been dropped,
    /// and so we won't delete the in-memory structure or insert its ID into [`Self::replay_table_dropped`].
    ///
    /// When looking up the `st_table` row for a table, if it has an entry here,
    /// that means there are two rows resident in `st_table` at this point in replay.
    /// We return the row recorded here rather than inspecting `st_table`.
    ///
    /// We remove from this set when we reach the matching delete,
    /// and assert this set is empty at the end of each transaction.
    ///
    /// [`RowPointer`]s from this set are passed to the `unsafe` [`Table::get_row_ref_unchecked`],
    /// so it's important to properly maintain only [`RowPointer`]s to valid, extant, non-deleted rows.
    replay_table_updated: IntMap<TableId, RowPointer>,
}

impl MemoryUsage for ReplayCommittedState<'_> {
    fn heap_usage(&self) -> usize {
        let Self {
            state: _, // We don't attribute usage to `CommittedState`.
            replay_table_dropped,
            replay_columns_to_ignore,
            replay_table_updated,
        } = self;
        replay_columns_to_ignore.heap_usage() + replay_table_dropped.heap_usage() + replay_table_updated.heap_usage()
    }
}

impl<'cs> ReplayCommittedState<'cs> {
    pub(super) fn new(state: &'cs mut CommittedState) -> Self {
        Self {
            state,
            replay_table_dropped: <_>::default(),
            replay_table_updated: <_>::default(),
            replay_columns_to_ignore: <_>::default(),
        }
    }

    /// The purpose of this is to rebuild the state of the datastore
    /// after having inserted all of rows from the message log.
    /// This is necessary because, for example, inserting a row into `st_table`
    /// is not equivalent to calling `create_table`.
    /// There may eventually be better way to do this, but this will have to do for now.
    fn rebuild_state_after_replay(&mut self, locking: &Locking) -> Result<()> {
        // Prior versions of `RelationalDb::migrate_system_tables` (defined in the `core` crate)
        // initialized newly-created system sequences to `allocation: 4097`,
        // while `committed_state::bootstrap_system_tables` sets `allocation: 4096`.
        // This affected the system table migration which added
        // `st_view_view_id_seq` and `st_view_arg_id_seq`.
        // As a result, when replaying these databases' commitlogs without a snapshot,
        // we will end up with two rows in `st_sequence` for each of these sequences,
        // resulting in a unique constraint violation in `CommittedState::build_indexes`.
        // We fix this by, for each system sequence, deleting all but the row with the highest allocation.
        self.fixup_delete_duplicate_system_sequence_rows();

        // `build_missing_tables` must be called before indexes.
        // Honestly this should maybe just be one big procedure.
        // See John Carmack's philosophy on this.
        self.reschema_tables()?;
        self.build_missing_tables()?;
        self.build_indexes()?;
        self.collect_ephemeral_tables()?;

        *locking.sequence_state.lock() = self.state.build_sequence_state()?;

        Ok(())
    }

    /// Delete all but the highest-allocation `st_sequence` row for each system sequence.
    ///
    /// Prior versions of `RelationalDb::migrate_system_tables` (defined in the `core` crate)
    /// initialized newly-created system sequences to `allocation: 4097`,
    /// while `committed_state::bootstrap_system_tables` sets `allocation: 4096`.
    /// This affected the system table migration which added
    /// `st_view_view_id_seq` and `st_view_arg_id_seq`.
    /// As a result, when replaying these databases' commitlogs without a snapshot,
    /// we will end up with two rows in `st_sequence` for each of these sequences,
    /// resulting in a unique constraint violation in `CommittedState::build_indexes`.
    /// We call this method in [`super::datastore::Locking::rebuild_state_after_replay`]
    /// to avoid that unique constraint violation.
    fn fixup_delete_duplicate_system_sequence_rows(&mut self) {
        struct StSequenceRowInfo {
            sequence_id: SequenceId,
            allocated: i128,
            row_pointer: RowPointer,
        }

        // Get all the `st_sequence` rows which refer to sequences on system tables,
        // including any duplicates caused by the bug described above.
        let sequence_rows = self
            .state
            .table_scan(ST_SEQUENCE_ID)
            .expect("`st_sequence` should exist")
            .filter_map(|row_ref| {
                // Read the table ID to which the sequence refers,
                // in order to determine if this is a system sequence or not.
                let table_id = row_ref
                    .read_col::<TableId>(StSequenceFields::TableId)
                    .expect("`st_sequence` row should conform to `st_sequence` schema");

                // If this sequence refers to a system table, it may need a fixup.
                // User tables' sequences will never need fixups.
                table_id_is_reserved(table_id).then(|| {
                    let allocated = row_ref
                        .read_col::<i128>(StSequenceFields::Allocated)
                        .expect("`st_sequence` row should conform to `st_sequence` schema");
                    let sequence_id = row_ref
                        .read_col::<SequenceId>(StSequenceFields::SequenceId)
                        .expect("`st_sequence` row should conform to `st_sequence` schema");
                    StSequenceRowInfo {
                        allocated,
                        sequence_id,
                        row_pointer: row_ref.pointer(),
                    }
                })
            })
            .collect::<Vec<_>>();

        let (st_sequence, blob_store, ..) = self
            .state
            .get_table_and_blob_store_mut(ST_SEQUENCE_ID)
            .expect("`st_sequence` should exist");

        // Track the row with the highest allocation for each sequence.
        let mut highest_allocations: HashMap<SequenceId, (i128, RowPointer)> = HashMap::default();

        for StSequenceRowInfo {
            sequence_id,
            allocated,
            row_pointer,
        } in sequence_rows
        {
            // For each `st_sequence` row which refers to a system table,
            // if we've already seen a row for the same sequence,
            // keep only the row with the higher allocation.
            if let Some((prev_allocated, prev_row_pointer)) =
                highest_allocations.insert(sequence_id, (allocated, row_pointer))
            {
                // We have a duplicate row. We want to keep whichever has the higher `allocated`,
                // and delete the other.
                let row_pointer_to_delete = if prev_allocated > allocated {
                    // The previous row has a higher allocation than the new row,
                    // so delete the new row and restore `previous` to `highest_allocations`.
                    highest_allocations.insert(sequence_id, (prev_allocated, prev_row_pointer));
                    row_pointer
                } else {
                    // The previous row does not have a higher allocation than the new,
                    // so delete the previous row and keep the new one.
                    prev_row_pointer
                };

                st_sequence.delete(blob_store, row_pointer_to_delete, |_| ())
                    .expect("Duplicated `st_sequence` row at `row_pointer_to_delete` should be present in `st_sequence` during fixup");
            }
        }
    }

    fn build_indexes(&mut self) -> Result<()> {
        let st_indexes = self.state.tables.get(&ST_INDEX_ID).unwrap();
        let rows = st_indexes
            .scan_rows(&self.state.blob_store)
            .map(StIndexRow::try_from)
            .collect::<Result<Vec<_>>>()?;

        let st_constraints = self.state.tables.get(&ST_CONSTRAINT_ID).unwrap();
        let unique_constraints: HashSet<(TableId, ColSet)> = st_constraints
            .scan_rows(&self.state.blob_store)
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
                .state
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

    fn collect_ephemeral_tables(&mut self) -> Result<()> {
        self.state.ephemeral_tables = self.ephemeral_tables()?.into_iter().collect();
        Ok(())
    }

    fn ephemeral_tables(&self) -> Result<Vec<TableId>> {
        let mut tables = vec![ST_VIEW_SUB_ID, ST_VIEW_ARG_ID];

        let Some(st_view) = self.state.tables.get(&ST_VIEW_ID) else {
            return Ok(tables);
        };
        let backing_tables = st_view
            .scan_rows(&self.state.blob_store)
            .map(|row_ref| {
                let view_row = StViewRow::try_from(row_ref)?;
                view_row
                    .table_id
                    .ok_or_else(|| DatastoreError::View(ViewError::TableNotFound(view_row.view_id)))
            })
            .collect::<Result<Vec<_>>>()?;

        tables.extend(backing_tables);

        Ok(tables)
    }

    /// After replaying all old transactions,
    /// inserts and deletes into the system tables
    /// might not be reflected in the schemas of the built tables.
    /// So we must re-schema every built table.
    fn reschema_tables(&mut self) -> Result<()> {
        // For already built tables, we need to reschema them to account for constraints et al.
        let mut schemas = Vec::with_capacity(self.state.tables.len());
        for table_id in self.state.tables.keys().copied() {
            schemas.push(self.schema_for_table_raw(table_id)?);
        }
        for (table, schema) in self.state.tables.values_mut().zip(schemas) {
            table.with_mut_schema(|s| *s = schema);
        }
        Ok(())
    }

    /// After replaying all old transactions, tables which have rows will
    /// have been created in memory, but tables with no rows will not have
    /// been created. This function ensures that they are created.
    fn build_missing_tables(&mut self) -> Result<()> {
        // Find all ids of tables that are in `st_tables` but haven't been built.
        let table_ids = self
            .state
            .get_table(ST_TABLE_ID)
            .unwrap()
            .scan_rows(&self.state.blob_store)
            .map(|r| r.read_col(StTableFields::TableId).unwrap())
            .filter(|table_id| self.state.get_table(*table_id).is_none())
            .collect::<Vec<_>>();

        // Construct their schemas and insert tables for them.
        for table_id in table_ids {
            let schema = self.schema_for_table(table_id)?;
            self.state.create_table(table_id, schema);
        }
        Ok(())
    }

    fn next_tx_offset(&self) -> u64 {
        self.state.next_tx_offset
    }

    fn replay_truncate(&mut self, table_id: TableId) -> Result<()> {
        // (1) Table dropped? Avoid an error and just ignore the row instead.
        if self.replay_table_dropped.contains(&table_id) {
            return Ok(());
        }

        // Get the table for mutation.
        let (table, blob_store, ..) = self.state.get_table_and_blob_store_mut(table_id)?;

        // We do not need to consider a truncation of `st_table` itself,
        // as if that happens, the database is bricked.

        table.clear(blob_store);

        Ok(())
    }

    fn replay_delete_by_rel(&mut self, table_id: TableId, row: &ProductValue) -> Result<()> {
        // (1) Table dropped? Avoid an error and just ignore the row instead.
        if self.replay_table_dropped.contains(&table_id) {
            return Ok(());
        }

        // Get the table for mutation.
        let (table, blob_store, _, page_pool) = self.state.get_table_and_blob_store_mut(table_id)?;

        // Delete the row.
        let row_ptr = table
            .delete_equal_row(page_pool, blob_store, row)
            .map_err(TableError::Bflatn)?
            .ok_or_else(|| anyhow!("Delete for non-existent row when replaying transaction"))?;

        if table_id == ST_TABLE_ID {
            let referenced_table_id = row
                .elements
                .get(StTableFields::TableId.col_idx())
                .expect("`st_table` row should conform to `st_table` schema")
                .as_u32()
                .expect("`st_table` row should conform to `st_table` schema");
            if self
                .replay_table_updated
                .remove(&TableId::from(*referenced_table_id))
                .is_some()
            {
                // This delete is part of an update to an `st_table` row,
                // i.e. earlier in this transaction we inserted a new version of the row.
                // That means it's not a dropped table.
            } else {
                // A row was removed from `st_table`, so a table was dropped.
                // Remove that table from the in-memory structures.
                let dropped_table_id = Self::read_table_id(row);
                // It's safe to ignore the case where we don't have an in-memory structure for the deleted table.
                // This can happen if a table is initially empty at the snapshot or its creation,
                // and never has any rows inserted into or deleted from it.
                self.state.tables.remove(&dropped_table_id);

                // Mark the table as dropped so that when
                // processing row deletions for that table later,
                // they are simply ignored in (1).
                self.replay_table_dropped.insert(dropped_table_id);
            }
        }

        if table_id == ST_COLUMN_ID {
            // We may have reached the corresponding delete to an insert in `st_column`
            // as the result of a column-type-altering migration.
            // Now that the outdated `st_column` row isn't present any more,
            // we can stop ignoring it.
            //
            // It's also possible that we're deleting this column as the result of a deleted table,
            // and that there wasn't any corresponding insert at all.
            // If that's the case, `row_ptr` won't be in `self.replay_columns_to_ignore`,
            // which is fine.
            self.replay_columns_to_ignore.remove(&row_ptr);
        }

        Ok(())
    }

    fn replay_insert(&mut self, table_id: TableId, schema: &Arc<TableSchema>, row: &ProductValue) -> Result<()> {
        let (table, blob_store, pool) = self.state.get_table_and_blob_store_or_create(table_id, schema);

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
            let (table, blob_store, _) = self.state.get_table_and_blob_store(ST_TABLE_ID)?;

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
        if let Ok((table, ..)) = self.state.get_table_and_blob_store_mut(row.table_id) {
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
        let target_table_id = Self::read_table_id(st_column_row);
        let target_col_id = ColId::deserialize(ValueDeserializer::from_ref(&st_column_row.elements[1]))
            .expect("second field in `st_column` should decode to a `ColId`");

        let outdated_st_column_rows = iter_st_column_for_table(self, &target_table_id.into())?
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
        // We're replaying and we don't have unique constraints yet.
        // Due to replay handling all inserts first and deletes after,
        // when processing `st_column` insert/deletes,
        // we may end up with two definitions for the same `col_pos`.
        // Of those two, we're interested in the one we just inserted
        // and not the other one, as it is being replaced.
        // `Self::ignore_previous_version_of_column` has marked the old version as ignored,
        // so filter only the non-ignored columns.
        let mut columns = iter_st_column_for_table(self, &table_id.into())?
            .filter(|row_ref| !self.replay_columns_to_ignore.contains(&row_ref.pointer()))
            .map(|row_ref| StColumnRow::try_from(row_ref).map(Into::into))
            .collect::<Result<Vec<_>>>()?;

        // Columns in `st_column` are not in general sorted by their `col_pos`,
        // though they will happen to be for tables which have never undergone migrations
        // because their initial insertion order matches their `col_pos` order.
        columns.sort_by_key(|col: &ColumnSchema| col.col_pos);

        // Update the columns and layout of the the in-memory table.
        if let Some(table) = self.state.tables.get_mut(&table_id) {
            table.change_columns_to(columns).map_err(TableError::from)?;
        }

        Ok(())
    }

    fn replay_end_tx(&mut self) -> Result<()> {
        self.state.next_tx_offset += 1;

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

    /// Assuming that a `TableId` is stored as the first field in `row`, read it.
    fn read_table_id(row: &ProductValue) -> TableId {
        TableId::deserialize(ValueDeserializer::from_ref(&row.elements[0]))
            .expect("first field in `st_column` should decode to a `TableId`")
    }
}

impl StateView for ReplayCommittedState<'_> {
    /// Find the `st_table` row for `table_id`,
    /// first inspecting [`Self::replay_table_updated`],
    /// then falling back to [`CommittedState::iter_by_col_eq`].
    fn find_st_table_row(&self, table_id: TableId) -> Result<StTableRow> {
        if let Some(row_ptr) = self.replay_table_updated.get(&table_id) {
            let (table, blob_store, _) = self.state.get_table_and_blob_store(table_id)?;
            // SAFETY: `row_ptr` is stored in `self.replay_table_updated`,
            // meaning it was inserted into `st_table` by `replay_insert`
            // and has not yet been deleted by `replay_delete_by_rel`.
            let row_ref = unsafe { table.get_row_ref_unchecked(blob_store, *row_ptr) };
            StTableRow::try_from(row_ref)
        } else {
            self.state.find_st_table_row(table_id)
        }
    }

    type Iter<'a>
        = <CommittedState as StateView>::Iter<'a>
    where
        Self: 'a;

    type IterByColRange<'a, R: RangeBounds<AlgebraicValue>>
        = <CommittedState as StateView>::IterByColRange<'a, R>
    where
        Self: 'a;

    type IterByColEq<'a, 'r>
        = <CommittedState as StateView>::IterByColEq<'a, 'r>
    where
        Self: 'a;

    fn get_schema(&self, table_id: TableId) -> Option<&Arc<TableSchema>> {
        self.state.get_schema(table_id)
    }

    fn table_row_count(&self, table_id: TableId) -> Option<u64> {
        self.state.table_row_count(table_id)
    }

    fn iter(&self, table_id: TableId) -> Result<Self::Iter<'_>> {
        self.state.iter(table_id)
    }

    fn iter_by_col_range<R: RangeBounds<AlgebraicValue>>(
        &self,
        table_id: TableId,
        cols: spacetimedb_primitives::ColList,
        range: R,
    ) -> Result<Self::IterByColRange<'_, R>> {
        self.state.iter_by_col_range(table_id, cols, range)
    }

    fn iter_by_col_eq<'a, 'r>(
        &'a self,
        table_id: TableId,
        cols: impl Into<spacetimedb_primitives::ColList>,
        value: &'r AlgebraicValue,
    ) -> Result<Self::IterByColEq<'a, 'r>> {
        self.state.iter_by_col_eq(table_id, cols, value)
    }
}
