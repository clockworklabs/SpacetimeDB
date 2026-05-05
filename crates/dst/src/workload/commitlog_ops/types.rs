//! Serializable interaction model for relational-db + commitlog DST.

use crate::{
    client::SessionId,
    config::CommitlogFaultProfile,
    schema::SimRow,
    workload::table_ops::{TableWorkloadInteraction, TableWorkloadOutcome},
};

/// One interaction in the commitlog-oriented mixed workload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitlogInteraction {
    /// Reused base workload interaction from `table_ops`.
    Table(TableWorkloadInteraction),
    /// Create a dynamic user table for a logical slot.
    CreateDynamicTable { conn: SessionId, slot: u32 },
    /// Drop a previously created dynamic user table.
    DropDynamicTable { conn: SessionId, slot: u32 },
    /// Migrate dynamic table schema for a slot.
    MigrateDynamicTable { conn: SessionId, slot: u32 },
    /// Close and restart the database from durable history.
    CloseReopen,
}

/// Successful run summary for commitlog target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitlogWorkloadOutcome {
    pub applied_steps: usize,
    pub durable_commit_count: usize,
    pub replay_table_count: usize,
    pub schema: SchemaSummary,
    pub interactions: InteractionSummary,
    pub table_ops: TableOperationSummary,
    pub transactions: TransactionSummary,
    pub runtime: RuntimeSummary,
    pub disk_faults: DiskFaultSummary,
    pub replay: DurableReplaySummary,
    pub table: TableWorkloadOutcome,
}

/// State observed after opening a fresh database from durable commitlog history.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DurableReplaySummary {
    pub durable_offset: Option<u64>,
    pub base_rows: Vec<Vec<SimRow>>,
    pub dynamic_table_count: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SchemaSummary {
    pub initial_tables: usize,
    pub initial_columns: usize,
    pub max_columns_per_table: usize,
    pub initial_indexes: usize,
    pub extra_indexes: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InteractionSummary {
    pub table: usize,
    pub create_dynamic_table: usize,
    pub drop_dynamic_table: usize,
    pub migrate_dynamic_table: usize,
    pub close_reopen_requested: usize,
    pub close_reopen_applied: usize,
    pub close_reopen_skipped: usize,
    pub skipped: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TableOperationSummary {
    /// Explicit write transaction starts.
    pub begin_tx: usize,
    /// Explicit write transaction commits.
    pub commit_tx: usize,
    /// Explicit write transaction rollbacks.
    pub rollback_tx: usize,
    /// Long read snapshot starts.
    pub begin_read_tx: usize,
    /// Long read snapshot releases.
    pub release_read_tx: usize,
    /// Expected failures when a second writer tries to begin.
    pub begin_tx_conflict: usize,
    /// Expected failures when a second writer tries to write.
    pub write_conflict_insert: usize,
    /// Fresh single-row inserts.
    pub insert: usize,
    /// Single-row deletes.
    pub delete: usize,
    /// Exact full-row reinserts that should be idempotent no-ops.
    pub exact_duplicate_insert: usize,
    /// Same primary id with different payload; should violate the unique key.
    pub unique_key_conflict_insert: usize,
    /// Deletes of absent rows that should report no mutation.
    pub delete_missing: usize,
    /// Multi-row inserts.
    pub batch_insert: usize,
    /// Multi-row deletes.
    pub batch_delete: usize,
    /// Delete followed by inserting the same row.
    pub reinsert: usize,
    /// Add-column schema changes against live base tables.
    pub add_column: usize,
    /// Add-index schema changes against live base tables.
    pub add_index: usize,
    /// Primary-id lookup oracle checks.
    pub point_lookup: usize,
    /// Column equality count oracle checks.
    pub predicate_count: usize,
    /// Indexed range scan oracle checks.
    pub range_scan: usize,
    /// Full scan oracle checks.
    pub full_scan: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TransactionSummary {
    pub explicit_begin: usize,
    pub explicit_commit: usize,
    pub explicit_rollback: usize,
    pub auto_commit: usize,
    pub read_tx: usize,
    pub durable_commit_count: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeSummary {
    pub known_runtime_tasks_scheduled: usize,
    pub durability_actors_started: usize,
    pub runtime_alive_tasks: Option<usize>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiskFaultSummary {
    pub profile: CommitlogFaultProfile,
    pub latency: usize,
    pub short_read: usize,
    pub short_write: usize,
    pub read_error: usize,
    pub write_error: usize,
    pub flush_error: usize,
    pub fsync_error: usize,
    pub open_error: usize,
    pub metadata_error: usize,
}
