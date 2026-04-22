//! Shared transactional table workload used by datastore-like targets.

mod generation;
mod model;
mod properties;
mod runner;
mod scenarios;
mod types;

pub use generation::{InteractionStream, ScenarioPlanner};
pub use properties::{followup_properties_after_commit, property_interaction, TableProperty};
pub use runner::{execute_interactions, run_generated_with_engine};
pub use scenarios::{default_target_ops, BankingScenario, RandomCrudScenario, TableScenarioId};
pub use types::{
    ConnectionWriteState, TableScenario, TableWorkloadCase, TableWorkloadEngine, TableWorkloadEvent,
    TableWorkloadExecutionFailure, TableWorkloadInteraction, TableWorkloadOutcome,
};
