//! Workload for standalone host/module testing.

mod generation;
mod types;

pub(crate) use generation::NextInteractionGenerator;
pub use types::{HostScenarioId, ModuleInteraction, ModuleReducerSpec, ModuleWorkloadOutcome};
