//! Shared transactional table workload used by datastore-like targets.

mod generation;
mod model;
mod runner;
mod scenarios;
mod types;

pub(crate) use generation::NextInteractionGenerator;
pub(crate) use runner::run_generated_with_engine;
pub use scenarios::TableScenarioId;
pub(crate) use types::{ConnectionWriteState, TableScenario, TableWorkloadEngine};
pub use types::{TableWorkloadInteraction, TableWorkloadOutcome};
