use std::ops::Bound;

use spacetimedb_sats::AlgebraicValue;

use crate::{
    client::SessionId,
    schema::{ColumnPlan, SchemaPlan, SimRow},
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
    fn fill_pending(&self, planner: &mut ScenarioPlanner<'_>, conn: SessionId);
}

/// One generated workload step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedInteraction {
    pub op: TableOperation,
    /// Generator-side coverage/debug label.
    ///
    /// Correctness must not depend on this field. Properties predict expected
    /// behavior from the model and `op`; this label only preserves intent in
    /// summaries and failure reports.
    pub case: TableInteractionCase,
}

pub type TableWorkloadInteraction = PlannedInteraction;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableOperation {
    /// Start an explicit write transaction on a connection.
    BeginTx { conn: SessionId },
    /// Commit the connection's explicit write transaction.
    CommitTx { conn: SessionId },
    /// Roll back the connection's explicit write transaction.
    RollbackTx { conn: SessionId },
    /// Hold a read snapshot open while later reads observe stable state.
    BeginReadTx { conn: SessionId },
    /// Release a previously opened read snapshot.
    ReleaseReadTx { conn: SessionId },
    /// Insert one or more rows.
    InsertRows {
        conn: SessionId,
        table: usize,
        rows: Vec<SimRow>,
    },
    /// Delete one or more rows.
    DeleteRows {
        conn: SessionId,
        table: usize,
        rows: Vec<SimRow>,
    },
    /// Add a column to an existing table with a default for live rows.
    AddColumn {
        conn: SessionId,
        table: usize,
        column: ColumnPlan,
        default: AlgebraicValue,
    },
    /// Add a non-primary index after data exists.
    AddIndex {
        conn: SessionId,
        table: usize,
        cols: Vec<u16>,
    },
    /// Query a row by primary id and compare against the model.
    PointLookup { conn: SessionId, table: usize, id: u64 },
    /// Count rows by equality on one column and compare against the model.
    PredicateCount {
        conn: SessionId,
        table: usize,
        col: u16,
        value: AlgebraicValue,
    },
    /// Scan an indexed range and compare against model filtering.
    RangeScan {
        conn: SessionId,
        table: usize,
        cols: Vec<u16>,
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    },
    /// Scan all visible rows and compare against the model.
    FullScan { conn: SessionId, table: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableErrorKind {
    UniqueConstraintViolation,
    MissingRow,
    WriteConflict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableInteractionCase {
    BeginTx,
    CommitTx,
    RollbackTx,
    BeginReadTx,
    ReleaseReadTx,
    BeginTxConflict,
    WriteConflictInsert,
    Insert,
    Delete,
    ExactDuplicateInsert,
    UniqueKeyConflictInsert,
    DeleteMissing,
    BatchInsert,
    BatchDelete,
    Reinsert,
    AddColumn,
    AddIndex,
    PointLookup,
    PredicateCount,
    RangeScan,
    FullScan,
}

impl PlannedInteraction {
    pub fn new(op: TableOperation, case: TableInteractionCase) -> Self {
        Self { op, case }
    }

    pub fn begin_tx(conn: SessionId) -> Self {
        Self::new(TableOperation::BeginTx { conn }, TableInteractionCase::BeginTx)
    }

    pub fn commit_tx(conn: SessionId) -> Self {
        Self::new(TableOperation::CommitTx { conn }, TableInteractionCase::CommitTx)
    }

    pub fn rollback_tx(conn: SessionId) -> Self {
        Self::new(TableOperation::RollbackTx { conn }, TableInteractionCase::RollbackTx)
    }

    pub fn begin_read_tx(conn: SessionId) -> Self {
        Self::new(TableOperation::BeginReadTx { conn }, TableInteractionCase::BeginReadTx)
    }

    pub fn release_read_tx(conn: SessionId) -> Self {
        Self::new(
            TableOperation::ReleaseReadTx { conn },
            TableInteractionCase::ReleaseReadTx,
        )
    }

    pub fn begin_tx_conflict(conn: SessionId) -> Self {
        Self::new(TableOperation::BeginTx { conn }, TableInteractionCase::BeginTxConflict)
    }

    pub fn write_conflict_insert(conn: SessionId, table: usize, row: SimRow) -> Self {
        Self::insert_rows(conn, table, vec![row], TableInteractionCase::WriteConflictInsert)
    }

    pub fn insert(conn: SessionId, table: usize, row: SimRow) -> Self {
        Self::insert_with_case(conn, table, row, TableInteractionCase::Insert)
    }

    pub fn insert_with_case(conn: SessionId, table: usize, row: SimRow, case: TableInteractionCase) -> Self {
        Self::insert_rows(conn, table, vec![row], case)
    }

    pub fn delete(conn: SessionId, table: usize, row: SimRow) -> Self {
        Self::delete_with_case(conn, table, row, TableInteractionCase::Delete)
    }

    pub fn delete_with_case(conn: SessionId, table: usize, row: SimRow, case: TableInteractionCase) -> Self {
        Self::delete_rows(conn, table, vec![row], case)
    }

    pub fn exact_duplicate_insert(conn: SessionId, table: usize, row: SimRow) -> Self {
        Self::insert_with_case(conn, table, row, TableInteractionCase::ExactDuplicateInsert)
    }

    pub fn unique_key_conflict_insert(conn: SessionId, table: usize, row: SimRow) -> Self {
        Self::insert_with_case(conn, table, row, TableInteractionCase::UniqueKeyConflictInsert)
    }

    pub fn delete_missing(conn: SessionId, table: usize, row: SimRow) -> Self {
        Self::delete_with_case(conn, table, row, TableInteractionCase::DeleteMissing)
    }

    pub fn batch_insert(conn: SessionId, table: usize, rows: Vec<SimRow>) -> Self {
        Self::insert_rows(conn, table, rows, TableInteractionCase::BatchInsert)
    }

    pub fn batch_delete(conn: SessionId, table: usize, rows: Vec<SimRow>) -> Self {
        Self::delete_rows(conn, table, rows, TableInteractionCase::BatchDelete)
    }

    fn insert_rows(conn: SessionId, table: usize, rows: Vec<SimRow>, case: TableInteractionCase) -> Self {
        Self::new(TableOperation::InsertRows { conn, table, rows }, case)
    }

    fn delete_rows(conn: SessionId, table: usize, rows: Vec<SimRow>, case: TableInteractionCase) -> Self {
        Self::new(TableOperation::DeleteRows { conn, table, rows }, case)
    }

    pub fn add_column(conn: SessionId, table: usize, column: ColumnPlan, default: AlgebraicValue) -> Self {
        Self::new(
            TableOperation::AddColumn {
                conn,
                table,
                column,
                default,
            },
            TableInteractionCase::AddColumn,
        )
    }

    pub fn add_index(conn: SessionId, table: usize, cols: Vec<u16>) -> Self {
        Self::new(
            TableOperation::AddIndex { conn, table, cols },
            TableInteractionCase::AddIndex,
        )
    }

    pub fn point_lookup(conn: SessionId, table: usize, id: u64) -> Self {
        Self::new(
            TableOperation::PointLookup { conn, table, id },
            TableInteractionCase::PointLookup,
        )
    }

    pub fn predicate_count(conn: SessionId, table: usize, col: u16, value: AlgebraicValue) -> Self {
        Self::new(
            TableOperation::PredicateCount {
                conn,
                table,
                col,
                value,
            },
            TableInteractionCase::PredicateCount,
        )
    }

    pub fn range_scan(
        conn: SessionId,
        table: usize,
        cols: Vec<u16>,
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) -> Self {
        Self::new(
            TableOperation::RangeScan {
                conn,
                table,
                cols,
                lower,
                upper,
            },
            TableInteractionCase::RangeScan,
        )
    }

    pub fn full_scan(conn: SessionId, table: usize) -> Self {
        Self::new(TableOperation::FullScan { conn, table }, TableInteractionCase::FullScan)
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

/// Per-session write transaction bookkeeping shared by locking targets.
pub(crate) struct ConnectionWriteState<Tx> {
    /// Open mutable transaction handle for each simulated session.
    pub tx_by_connection: Vec<Option<Tx>>,
    /// Session that currently owns the single-writer lock, if any.
    pub active_writer: Option<SessionId>,
}

impl<Tx> ConnectionWriteState<Tx> {
    pub fn new(connection_count: usize) -> Self {
        Self {
            tx_by_connection: (0..connection_count).map(|_| None).collect(),
            active_writer: None,
        }
    }

    pub fn ensure_known_connection(&self, conn: SessionId) -> Result<(), String> {
        self.tx_by_connection
            .get(conn.as_index())
            .map(|_| ())
            .ok_or_else(|| format!("connection {conn} out of range"))
    }

    pub fn ensure_writer_owner(&self, conn: SessionId, action: &str) -> Result<(), String> {
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
