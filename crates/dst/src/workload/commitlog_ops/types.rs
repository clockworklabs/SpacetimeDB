//! Serializable interaction model for relational-db + commitlog DST.

use crate::workload::table_ops::{TableWorkloadInteraction, TableWorkloadOutcome};

/// One interaction in the commitlog-oriented mixed workload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitlogInteraction {
    /// Reused base workload interaction from `table_ops`.
    Table(TableWorkloadInteraction),
    /// Create a dynamic user table for a logical slot.
    CreateDynamicTable { conn: usize, slot: u32 },
    /// Drop a previously created dynamic user table.
    DropDynamicTable { conn: usize, slot: u32 },
    /// Migrate dynamic table schema for a slot.
    MigrateDynamicTable { conn: usize, slot: u32 },
    /// Ask the mock commitlog file layer to run a sync attempt.
    ChaosSync,
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
    pub table: TableWorkloadOutcome,
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
    pub chaos_sync: usize,
    pub close_reopen_requested: usize,
    pub close_reopen_applied: usize,
    pub close_reopen_skipped: usize,
    pub skipped: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TableOperationSummary {
    pub begin_tx: usize,
    pub commit_tx: usize,
    pub rollback_tx: usize,
    pub insert: usize,
    pub delete: usize,
    pub duplicate_insert: usize,
    pub delete_missing: usize,
    pub batch_insert: usize,
    pub batch_delete: usize,
    pub reinsert: usize,
    pub point_lookup: usize,
    pub predicate_count: usize,
    pub range_scan: usize,
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
    pub known_tokio_tasks_scheduled: usize,
    pub durability_actors_started: usize,
    pub runtime_alive_tasks: Option<usize>,
}
