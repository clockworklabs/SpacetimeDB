use serde::{Deserialize, Serialize};

use crate::{
    schema::{SchemaPlan, SimRow},
    seed::DstRng,
};

use super::{generation::ScenarioPlanner, properties::TableProperty, scenarios::TableScenarioId};

/// Scenario hook for shared table-oriented workloads.
///
/// A scenario supplies the initial schema, scenario-specific commit-time
/// properties, and any final invariant over the collected outcome.
pub(crate) trait TableScenario: Clone {
    fn generate_schema(&self, rng: &mut DstRng) -> SchemaPlan;
    fn validate_outcome(&self, schema: &SchemaPlan, outcome: &TableWorkloadOutcome) -> anyhow::Result<()>;
    fn commit_properties(&self) -> Vec<TableWorkloadInteraction>;
    fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: usize);
}

/// Materialized shared table-workload case reused by multiple targets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TableWorkloadCase {
    /// Seed used to derive schema and workload decisions.
    pub seed: crate::seed::DstSeed,
    /// Shared workload scenario identifier.
    pub(crate) scenario: TableScenarioId,
    /// Number of simulated client connections in the run.
    pub(crate) num_connections: usize,
    /// Initial schema installed into target before replaying interactions.
    pub(crate) schema: SchemaPlan,
    /// Materialized interaction trace for replay and shrinking.
    pub interactions: Vec<TableWorkloadInteraction>,
}

/// One generated workload step.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TableWorkloadInteraction {
    BeginTx { conn: usize },
    CommitTx { conn: usize },
    RollbackTx { conn: usize },
    Insert { conn: usize, table: usize, row: SimRow },
    Delete { conn: usize, table: usize, row: SimRow },
    Check(TableProperty),
}

/// Final state gathered from a table-workload engine after execution ends.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TableWorkloadOutcome {
    /// Row count for each table in schema order.
    pub final_row_counts: Vec<u64>,
    /// Full committed rows for each table in schema order.
    pub final_rows: Vec<Vec<SimRow>>,
}

/// First failing interaction observed while executing a generated workload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TableWorkloadExecutionFailure {
    /// Zero-based position of the failing interaction.
    pub step_index: usize,
    /// Target-provided error message.
    pub reason: String,
    /// Interaction that triggered the failure.
    pub(crate) interaction: TableWorkloadInteraction,
}

/// Minimal engine interface implemented by concrete table-oriented targets.
pub(crate) trait TableWorkloadEngine {
    fn execute(&mut self, interaction: &TableWorkloadInteraction) -> Result<(), String>;
    fn collect_outcome(&mut self) -> anyhow::Result<TableWorkloadOutcome>;
    fn finish(&mut self);
}

/// Per-connection write transaction bookkeeping shared by locking targets.
pub(crate) struct ConnectionWriteState<Tx> {
    /// Open mutable transaction handle for each simulated connection.
    pub tx_by_connection: Vec<Option<Tx>>,
    /// Connection that currently owns the single-writer lock, if any.
    pub active_writer: Option<usize>,
}

impl<Tx> ConnectionWriteState<Tx> {
    pub fn new(connection_count: usize) -> Self {
        Self {
            tx_by_connection: (0..connection_count).map(|_| None).collect(),
            active_writer: None,
        }
    }

    pub fn ensure_known_connection(&self, conn: usize) -> Result<(), String> {
        self.tx_by_connection
            .get(conn)
            .map(|_| ())
            .ok_or_else(|| format!("connection {conn} out of range"))
    }

    pub fn ensure_writer_owner(&self, conn: usize, action: &str) -> Result<(), String> {
        self.ensure_known_connection(conn)?;
        match self.active_writer {
            Some(owner) if owner == conn => Ok(()),
            Some(owner) => Err(format!(
                "connection {conn} cannot {action} while connection {owner} owns lock"
            )),
            None => Err(format!("connection {conn} has no transaction to {action}")),
        }
    }
}
