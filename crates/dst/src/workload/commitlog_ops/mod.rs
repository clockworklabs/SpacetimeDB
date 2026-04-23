//! Commitlog-oriented workload that composes `table_ops` with lifecycle/chaos.

mod generation;
mod types;

pub(crate) use generation::NextInteractionGeneratorComposite;
pub use types::{CommitlogInteraction, CommitlogWorkloadOutcome};
