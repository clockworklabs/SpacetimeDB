//! Shared transactional table workload used by table-oriented targets.

mod generation;
mod model;
mod scenarios;
pub(crate) mod strategies;
mod types;

pub(crate) use generation::TableWorkloadSource;
pub(crate) use model::{PredictedOutcome, TableOracle};
pub use scenarios::TableScenarioId;
pub(crate) use types::{ConnectionWriteState, TableScenario};
pub use types::{TableErrorKind, TableInteractionCase, TableOperation, TableWorkloadInteraction, TableWorkloadOutcome};
