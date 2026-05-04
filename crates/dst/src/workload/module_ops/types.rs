use spacetimedb_sats::AlgebraicType;

use crate::client::SessionId;

/// Single v1 scenario for standalone host target.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HostScenarioId {
    #[default]
    HostSmoke,
}

/// Reducer metadata used by the typed argument generator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleReducerSpec {
    pub name: String,
    pub params: Vec<AlgebraicType>,
}

/// One standalone-host interaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModuleInteraction {
    CallReducer {
        session: SessionId,
        reducer: String,
        args: Vec<spacetimedb_sats::AlgebraicValue>,
    },
    WaitScheduled {
        millis: u64,
    },
    CloseReopen,
    NoOp,
}

/// Run summary for standalone-host target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleWorkloadOutcome {
    pub steps_executed: usize,
    pub reducer_calls: usize,
    pub scheduler_waits: usize,
    pub reopens: usize,
    pub noops: usize,
    pub expected_errors: usize,
}
