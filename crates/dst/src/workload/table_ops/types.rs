use crate::{
    core::TargetEngine,
    schema::{SchemaPlan, SimRow},
    seed::DstRng,
};

use super::generation::ScenarioPlanner;

/// Scenario hook for shared table-oriented workloads.
///
/// A scenario supplies the initial schema, scenario-specific commit-time
/// properties, and any final invariant over the collected outcome.
pub(crate) trait TableScenario: Clone {
    fn generate_schema(&self, rng: &mut DstRng) -> SchemaPlan;
    fn validate_outcome(&self, schema: &SchemaPlan, outcome: &TableWorkloadOutcome) -> anyhow::Result<()>;
    fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: usize);
}

/// One generated workload step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableWorkloadInteraction {
    BeginTx { conn: usize },
    CommitTx { conn: usize },
    RollbackTx { conn: usize },
    Insert { conn: usize, table: usize, row: SimRow },
    Delete { conn: usize, table: usize, row: SimRow },
}

/// Final state gathered from a table-workload engine after execution ends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableWorkloadOutcome {
    /// Row count for each table in schema order.
    pub final_row_counts: Vec<u64>,
    /// Full committed rows for each table in schema order.
    pub final_rows: Vec<Vec<SimRow>>,
}

/// Minimal engine interface implemented by concrete table-oriented targets.
pub(crate) trait TableWorkloadEngine {
    fn execute(&mut self, interaction: &TableWorkloadInteraction) -> Result<(), String>;
    fn collect_outcome(&mut self) -> anyhow::Result<TableWorkloadOutcome>;
    fn finish(&mut self);
}

impl<T> TargetEngine<TableWorkloadInteraction> for T
where
    T: TableWorkloadEngine,
{
    type Outcome = TableWorkloadOutcome;
    type Error = String;

    fn execute_interaction(&mut self, interaction: &TableWorkloadInteraction) -> Result<(), Self::Error> {
        self.execute(interaction)
    }

    fn finish(&mut self) {
        TableWorkloadEngine::finish(self);
    }

    fn collect_outcome(&mut self) -> anyhow::Result<Self::Outcome> {
        TableWorkloadEngine::collect_outcome(self)
    }
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
