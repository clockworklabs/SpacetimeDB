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
    pub table: TableWorkloadOutcome,
}
