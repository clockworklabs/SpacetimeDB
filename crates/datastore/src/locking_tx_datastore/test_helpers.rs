//! Test-only helpers shared between the datastore's internal tests and
//! downstream-crate tests (e.g. `spacetimedb-core`'s `update.rs`).
//!
//! These are gated by `#[cfg(any(test, feature = "test"))]` and re-exported
//! from `locking_tx_datastore::mod` so they are reachable from other crates
//! that enable the `test` feature.

use super::datastore::Locking;
use super::state_view::StateView as _;
use super::tx_state::PendingSchemaChange;
use super::MutTxId;
use crate::system_tables::{StEventTableFields, ST_EVENT_TABLE_ID};
use crate::traits::MutTxDatastore as _;
use spacetimedb_primitives::TableId;

/// Asserts that the live schema's `is_event` flag for `table_id` equals `expected`.
pub fn assert_is_event_state(tx: &MutTxId, table_id: TableId, expected: bool) {
    let actual = tx
        .get_schema(table_id)
        .map(|s| s.is_event)
        .expect("schema should exist");
    assert_eq!(actual, expected, "expected table {table_id:?} is_event={expected}");
}

/// Returns whether `st_event_table` contains a row referencing `table_id`.
pub fn st_event_table_has_row(datastore: &Locking, tx: &MutTxId, table_id: TableId) -> bool {
    datastore
        .iter_by_col_eq_mut_tx(tx, ST_EVENT_TABLE_ID, StEventTableFields::TableId, &table_id.into())
        .expect("st_event_table lookup should succeed")
        .next()
        .is_some()
}

/// Asserts that `tx.pending_schema_changes()` contains exactly one
/// `TableAlterEventFlag` change for `table_id` recording the old value
/// (i.e. the value just before we altered to `state`).
pub fn check_table_event_flag_altered(tx: &MutTxId, table_id: TableId, state: bool) {
    assert_eq!(
        tx.pending_schema_changes(),
        [PendingSchemaChange::TableAlterEventFlag(table_id, !state)]
    );
}
