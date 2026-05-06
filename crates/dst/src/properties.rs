//! Reusable property runtime shared by DST targets.
//!
//! This module is the boundary between target execution and semantic checking.
//! Targets emit observations and implement [`TargetPropertyAccess`]; property
//! rules compare those observations against either the target's externally
//! visible state, an oracle model, or durable replay state.
//!
//! ## Property Model
//!
//! A property is a named check over a run. It observes generated interactions,
//! target observations, target-visible state, oracle models, and final
//! outcomes. Failures should include a stable property name and enough context
//! to replay the seed or trace.
//!
//! The current catalog is intentionally small and falls into the same groups
//! used by the proposal:
//!
//! - Safety properties: `NotCrash`, `ErrorMatchesOracle`,
//!   `NoMutationMatchesModel`, `DurableReplayMatchesModel`,
//!   `SnapshotCaptureMaintainsPrefix`, `SnapshotRestoreWithinDurablePrefix`,
//!   `BankingTablesMatch`, and `DynamicMigrationAutoInc`.
//! - Model/oracle properties: `PointLookupMatchesModel`,
//!   `PredicateCountMatchesModel`, `RangeScanMatchesModel`,
//!   `FullScanMatchesModel`, and the scenario-specific final table-state check.
//! - Differential and metamorphic properties: `InsertSelect`, `DeleteSelect`,
//!   `SelectSelectOptimizer`, `WhereTrueFalseNull`, and `IndexRangeExcluded`.
//! - Coverage and progress properties are not first-class rules yet. For now,
//!   targets expose operation and outcome counters. Those counters should become
//!   selectable properties once long-running and replication targets need them.

mod rules;
mod runtime;

use std::ops::Bound;

use spacetimedb_sats::AlgebraicValue;

use crate::{
    client::SessionId,
    schema::{SchemaPlan, SimRow},
    workload::{
        commitlog_ops::{DurableReplaySummary, SnapshotObservation},
        table_ops::{TableErrorKind, TableWorkloadInteraction, TableWorkloadOutcome},
    },
};

pub(crate) use runtime::PropertyRuntime;

/// Target adapter for property evaluation.
pub(crate) trait TargetPropertyAccess {
    fn schema_plan(&self) -> &SchemaPlan;
    fn lookup_in_connection(&self, conn: SessionId, table: usize, id: u64) -> Result<Option<SimRow>, String>;
    fn collect_rows_in_connection(&self, conn: SessionId, table: usize) -> Result<Vec<SimRow>, String>;
    fn collect_rows_for_table(&self, table: usize) -> Result<Vec<SimRow>, String>;
    fn count_rows(&self, table: usize) -> Result<usize, String>;
    fn count_by_col_eq(&self, table: usize, col: u16, value: &AlgebraicValue) -> Result<usize, String>;
    fn range_scan(
        &self,
        table: usize,
        cols: &[u16],
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
    ) -> Result<Vec<SimRow>, String>;
}

/// Canonical property IDs that can be selected by targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyKind {
    /// Safety: target execution must not panic.
    ///
    /// Enforced by the shared streaming runner.
    NotCrash,
    /// Metamorphic: an inserted row is immediately visible to the inserting session.
    InsertSelect,
    /// Metamorphic: a deleted row disappears from the deleting session's view.
    DeleteSelect,
    /// Differential: optimized predicate counts agree with direct row projection.
    SelectSelectOptimizer,
    /// Metamorphic: boolean partitions preserve total cardinality.
    WhereTrueFalseNull,
    /// Metamorphic: composite index range scans implement excluded upper bounds correctly.
    IndexRangeExcluded,
    /// Safety: banking scenario debit and credit shadow tables remain identical.
    BankingTablesMatch,
    /// Safety: auto-increment IDs continue advancing after dynamic table migration.
    DynamicMigrationAutoInc,
    /// Safety: durable replay state equals the oracle committed model.
    DurableReplayMatchesModel,
    /// Safety: failed snapshot capture does not publish a newer usable snapshot.
    SnapshotCaptureMaintainsPrefix,
    /// Safety: restored snapshots are within the durable prefix.
    SnapshotRestoreWithinDurablePrefix,
    /// Safety: observed errors match the model-predicted error class.
    ErrorMatchesOracle,
    /// Safety: model-predicted no-op interactions do not mutate visible state.
    NoMutationMatchesModel,
    /// Model/oracle: point lookups match the oracle session-visible model.
    PointLookupMatchesModel,
    /// Model/oracle: predicate counts match the oracle session-visible model.
    PredicateCountMatchesModel,
    /// Model/oracle: range scans match the oracle session-visible model.
    RangeScanMatchesModel,
    /// Model/oracle: full scans match the oracle session-visible model.
    FullScanMatchesModel,
}

#[derive(Clone, Debug)]
pub(crate) struct DynamicMigrationProbe {
    pub slot: u32,
    pub from_version: u32,
    pub to_version: u32,
    pub existing_rows: Vec<SimRow>,
    pub inserted_row: SimRow,
}

#[derive(Clone, Debug)]
pub(crate) enum TableMutation {
    Inserted {
        table: usize,
        requested: SimRow,
        returned: SimRow,
    },
    Deleted {
        table: usize,
        row: SimRow,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum TableObservation {
    Applied,
    Mutated {
        conn: SessionId,
        mutations: Vec<TableMutation>,
        in_tx: bool,
    },
    ObservedError(TableErrorKind),
    PointLookup {
        conn: SessionId,
        table: usize,
        id: u64,
        actual: Option<SimRow>,
    },
    PredicateCount {
        conn: SessionId,
        table: usize,
        col: u16,
        value: AlgebraicValue,
        actual: usize,
    },
    RangeScan {
        conn: SessionId,
        table: usize,
        cols: Vec<u16>,
        lower: Bound<AlgebraicValue>,
        upper: Bound<AlgebraicValue>,
        actual: Vec<SimRow>,
    },
    FullScan {
        conn: SessionId,
        table: usize,
        actual: Vec<SimRow>,
    },
    CommitOrRollback,
}

#[derive(Clone, Debug)]
pub(crate) enum CommitlogObservation {
    Table(TableObservation),
    Applied,
    Skipped,
    DynamicMigrationProbe(DynamicMigrationProbe),
    Snapshot(SnapshotObservation),
    DurableReplay(DurableReplaySummary),
}

struct PropertyContext<'a> {
    access: &'a dyn TargetPropertyAccess,
    models: &'a runtime::PropertyModels,
}

#[derive(Clone, Debug)]
enum PropertyEvent<'a> {
    TableInteractionApplied,
    RowInserted {
        conn: SessionId,
        table: usize,
        returned: &'a SimRow,
        in_tx: bool,
    },
    RowDeleted {
        conn: SessionId,
        table: usize,
        row: &'a SimRow,
        in_tx: bool,
    },
    ObservedError {
        observed: TableErrorKind,
        predicted: TableErrorKind,
        subject: Option<(SessionId, usize)>,
        interaction: &'a TableWorkloadInteraction,
    },
    NoMutation {
        subject: Option<(SessionId, usize)>,
        interaction: &'a TableWorkloadInteraction,
        observation: &'a TableObservation,
    },
    PointLookup {
        conn: SessionId,
        table: usize,
        id: u64,
        actual: &'a Option<SimRow>,
    },
    PredicateCount {
        conn: SessionId,
        table: usize,
        col: u16,
        value: &'a AlgebraicValue,
        actual: usize,
    },
    RangeScan {
        conn: SessionId,
        table: usize,
        cols: &'a [u16],
        lower: &'a Bound<AlgebraicValue>,
        upper: &'a Bound<AlgebraicValue>,
        actual: &'a [SimRow],
    },
    FullScan {
        conn: SessionId,
        table: usize,
        actual: &'a [SimRow],
    },
    CommitOrRollback,
    DynamicMigrationProbe(&'a DynamicMigrationProbe),
    SnapshotCapture(&'a SnapshotObservation),
    DurableReplay(&'a DurableReplaySummary),
    TableWorkloadFinished(&'a TableWorkloadOutcome),
}
