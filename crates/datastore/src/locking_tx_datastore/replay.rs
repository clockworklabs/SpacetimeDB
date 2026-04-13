use super::committed_state::CommittedState;
use crate::db_metrics::DB_METRICS;
use crate::locking_tx_datastore::datastore::ErrorBehavior;
use crate::locking_tx_datastore::datastore::ReplayError;
use crate::locking_tx_datastore::state_view::StateView;
use crate::system_tables::{StTableRow, ST_TABLE_ID};
use anyhow::{anyhow, Context};
use core::ops::{Deref, DerefMut};
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_lib::Identity;
use spacetimedb_primitives::TableId;
use spacetimedb_sats::algebraic_value::de::ValueDeserializer;
use spacetimedb_sats::buffer::BufReader;
use spacetimedb_sats::{AlgebraicValue, Deserialize, ProductValue};
use spacetimedb_schema::table_name::TableName;

pub(super) struct ReplayVisitor<'a, F> {
    pub(super) database_identity: &'a Identity,
    pub(super) committed_state: ReplayCommittedState<'a>,
    pub(super) progress: &'a mut F,
    // Since deletes are handled before truncation / drop, sometimes the schema
    // info is gone. We save the name on the first delete of that table so metrics
    // can still show a name.
    pub(super) dropped_table_names: IntMap<TableId, TableName>,
    pub(super) error_behavior: ErrorBehavior,
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
pub(super) struct ReplayCommittedState<'cs> {
    /// The committed state being contructed.
    pub(super) state: &'cs mut CommittedState,
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
