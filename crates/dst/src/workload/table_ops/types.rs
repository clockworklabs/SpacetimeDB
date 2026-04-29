use std::ops::Bound;

use spacetimedb_sats::AlgebraicValue;

use crate::{
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
pub struct PlannedInteraction {
    pub op: TableOperation,
    pub expected: ExpectedResult,
}

pub type TableWorkloadInteraction = PlannedInteraction;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableOperation {
    BeginTx {
        conn: usize,
    },
    CommitTx {
        conn: usize,
    },
    RollbackTx {
        conn: usize,
    },
    Insert {
        conn: usize,
        table: usize,
        row: SimRow,
    },
    Delete {
        conn: usize,
        table: usize,
        row: SimRow,
    },
    DuplicateInsert {
        conn: usize,
        table: usize,
        row: SimRow,
    },
    DeleteMissing {
        conn: usize,
        table: usize,
        row: SimRow,
    },
    BatchInsert {
        conn: usize,
        table: usize,
        rows: Vec<SimRow>,
    },
    BatchDelete {
        conn: usize,
        table: usize,
        rows: Vec<SimRow>,
    },
    Reinsert {
        conn: usize,
        table: usize,
        row: SimRow,
    },
    PointLookup {
        conn: usize,
        table: usize,
        id: u64,
    },
    PredicateCount {
        conn: usize,
        table: usize,
        col: u16,
        value: AlgebraicValue,
    },
    RangeScan {
        conn: usize,
        table: usize,
        cols: Vec<u16>,
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    },
    FullScan {
        conn: usize,
        table: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedResult {
    Ok,
    Err(ExpectedErrorKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedErrorKind {
    UniqueConstraintViolation,
    MissingRow,
}

impl PlannedInteraction {
    pub fn ok(op: TableOperation) -> Self {
        Self {
            op,
            expected: ExpectedResult::Ok,
        }
    }

    pub fn expected_err(op: TableOperation, kind: ExpectedErrorKind) -> Self {
        Self {
            op,
            expected: ExpectedResult::Err(kind),
        }
    }

    pub fn begin_tx(conn: usize) -> Self {
        Self::ok(TableOperation::BeginTx { conn })
    }

    pub fn commit_tx(conn: usize) -> Self {
        Self::ok(TableOperation::CommitTx { conn })
    }

    pub fn rollback_tx(conn: usize) -> Self {
        Self::ok(TableOperation::RollbackTx { conn })
    }

    pub fn insert(conn: usize, table: usize, row: SimRow) -> Self {
        Self::ok(TableOperation::Insert { conn, table, row })
    }

    pub fn delete(conn: usize, table: usize, row: SimRow) -> Self {
        Self::ok(TableOperation::Delete { conn, table, row })
    }

    pub fn duplicate_insert(conn: usize, table: usize, row: SimRow) -> Self {
        Self::expected_err(
            TableOperation::DuplicateInsert { conn, table, row },
            ExpectedErrorKind::UniqueConstraintViolation,
        )
    }

    pub fn delete_missing(conn: usize, table: usize, row: SimRow) -> Self {
        Self::expected_err(
            TableOperation::DeleteMissing { conn, table, row },
            ExpectedErrorKind::MissingRow,
        )
    }

    pub fn batch_insert(conn: usize, table: usize, rows: Vec<SimRow>) -> Self {
        Self::ok(TableOperation::BatchInsert { conn, table, rows })
    }

    pub fn batch_delete(conn: usize, table: usize, rows: Vec<SimRow>) -> Self {
        Self::ok(TableOperation::BatchDelete { conn, table, rows })
    }

    pub fn reinsert(conn: usize, table: usize, row: SimRow) -> Self {
        Self::ok(TableOperation::Reinsert { conn, table, row })
    }

    pub fn point_lookup(conn: usize, table: usize, id: u64) -> Self {
        Self::ok(TableOperation::PointLookup { conn, table, id })
    }

    pub fn predicate_count(conn: usize, table: usize, col: u16, value: AlgebraicValue) -> Self {
        Self::ok(TableOperation::PredicateCount {
            conn,
            table,
            col,
            value,
        })
    }

    pub fn range_scan(
        conn: usize,
        table: usize,
        cols: Vec<u16>,
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) -> Self {
        Self::ok(TableOperation::RangeScan {
            conn,
            table,
            cols,
            lower,
            upper,
        })
    }

    pub fn full_scan(conn: usize, table: usize) -> Self {
        Self::ok(TableOperation::FullScan { conn, table })
    }
}

/// Final state gathered from a table-workload engine after execution ends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableWorkloadOutcome {
    /// Row count for each table in schema order.
    pub final_row_counts: Vec<u64>,
    /// Full committed rows for each table in schema order.
    pub final_rows: Vec<Vec<SimRow>>,
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
