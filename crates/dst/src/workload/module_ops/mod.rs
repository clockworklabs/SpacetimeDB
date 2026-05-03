//! Workload for standalone host/module testing.

mod generation;
mod types;

pub(crate) use generation::ModuleWorkloadSource;
pub use types::{HostScenarioId, ModuleInteraction, ModuleReducerSpec, ModuleWorkloadOutcome};
