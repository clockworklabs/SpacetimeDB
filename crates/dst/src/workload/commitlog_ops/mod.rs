//! Commitlog-oriented workload that composes `table_ops` with lifecycle/chaos.

mod generation;
mod types;

pub(crate) use generation::CommitlogWorkloadSource;
pub use types::{
    CommitlogInteraction, CommitlogWorkloadOutcome, DiskFaultSummary, DurableReplaySummary, InteractionSummary,
    RuntimeSummary, SchemaSummary, SnapshotCaptureStatus, SnapshotObservation, TableOperationSummary,
    TransactionSummary,
};
