//! Serializable interaction model for relational-db + commitlog DST.

use serde::{Deserialize, Serialize};

use crate::{schema::SchemaPlan, seed::DstSeed, workload::table_ops::TableWorkloadInteraction};

/// One interaction in the commitlog-oriented mixed workload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
}

/// Materialized case for deterministic replay and shrinking.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitlogWorkloadCase {
    pub seed: DstSeed,
    pub scenario: crate::workload::table_ops::TableScenarioId,
    pub num_connections: usize,
    pub schema: SchemaPlan,
    pub interactions: Vec<CommitlogInteraction>,
}

/// Successful run summary for commitlog target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitlogWorkloadOutcome {
    pub applied_steps: usize,
    pub durable_commit_count: usize,
    pub replay_table_count: usize,
}

/// Failure info for commitlog target execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitlogWorkloadFailure {
    pub step_index: usize,
    pub reason: String,
    pub interaction: Option<CommitlogInteraction>,
}
