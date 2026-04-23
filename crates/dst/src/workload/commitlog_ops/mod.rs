//! Commitlog-oriented workload that composes `table_ops` with lifecycle/chaos.

mod generation;
mod types;

pub(crate) use generation::{materialize_case, InteractionStream};
pub use types::{CommitlogInteraction, CommitlogWorkloadCase, CommitlogWorkloadFailure, CommitlogWorkloadOutcome};
