//! Control-plane container inspection and idempotent lifecycle messages.
use crate::{container::endpoints::ContainerEndpoint, deployment::uuid_json, Hash, Identity, Uuid};
use serde::{Deserialize, Serialize};

mod resources;
pub use resources::{ContainerConfiguration, ReportedUsage, ResourceLimits, UsageTotals};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
}
impl ContainerAction {
    pub const fn path(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerOperationRequest {
    #[serde(with = "uuid_json")]
    pub request_id: Uuid,
}

/// Confirms admission only. It does not claim physical stop or runtime readiness.
/// An exact retry returns the generation originally recorded for this request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerOperationReceipt {
    pub database_identity: Identity,
    #[serde(with = "uuid_json")]
    pub request_id: Uuid,
    pub action: ContainerAction,
    #[serde(with = "decimal_u64")]
    pub generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesiredState {
    Stopped,
    Running,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedState {
    Pending,
    Starting,
    Running,
    Ready,
    Draining,
    Stopped,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    None,
    Fencing,
    NoLeader,
    NodeUnavailable,
    Capacity,
    BalanceUnavailable,
    BalanceExhausted,
    TargetUnavailable,
    PullFailed,
    LaunchFailed,
    ReadinessFailed,
    ExitFailure,
    OutOfMemory,
    NodePressure,
    LeaseExpired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentInstance {
    #[serde(with = "decimal_u64")]
    pub generation: u64,
    pub state: ObservedState,
    pub observed_revision: Option<Hash>,
    /// Application snapshot identity only. No values or hashes of values.
    pub applied_env_generation: Option<String>,
    pub exit_code: Option<i32>,
    pub oom_killed: bool,
    pub condition: Condition,
    /// Last accepted cumulative measurement for this exact generation. Missing
    /// or not-yet-reported usage is None, never manufactured zero consumption.
    pub usage: Option<ReportedUsage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationalState {
    pub desired_revision: Hash,
    pub desired_state: DesiredState,
    #[serde(with = "decimal_u64")]
    pub generation: u64,
    pub condition: Condition,
    pub restart_pending: bool,
    pub restart_attempt: u32,
    /// Unix milliseconds, represented exactly for browser clients.
    #[serde(with = "decimal_u64")]
    pub restart_not_before_ms: u64,
    /// Reports only this operational generation. A historical attempt is never
    /// presented as the currently authorized replacement.
    pub current_instance: Option<CurrentInstance>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum EndpointStatus {
    Available { endpoints: Vec<ContainerEndpoint> },
    Pending,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerStatus {
    pub database_identity: Identity,
    pub published: bool,
    /// Latest published image and limits, which may differ from a draining
    /// instance. No argv, environment, credentials, or private node addresses.
    pub configuration: Option<ContainerConfiguration>,
    pub operational: Option<OperationalState>,
    /// Address discovery is independent of readiness and remains available
    /// while stopped. Pending never means an empty declaration.
    pub endpoints: EndpointStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerErrorCode {
    InvalidRequest,
    AccessDenied,
    NotFound,
    Conflict,
    Unavailable,
    OutcomeUnknown,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContainerApiError {
    pub error: ContainerErrorCode,
}

pub(super) mod decimal_u64 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(number: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(number)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        let text = String::deserialize(deserializer)?;
        let value: u64 = text.parse().map_err(serde::de::Error::custom)?;
        if value.to_string() != text {
            return Err(serde::de::Error::custom("expected canonical decimal u64"));
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_preserves_full_generation_and_rejects_lossy_or_extra_fields() {
        let receipt = ContainerOperationReceipt {
            database_identity: Identity::ONE,
            request_id: Uuid::from_u128(0x01950000000070008000000000000001),
            action: ContainerAction::Restart,
            generation: u64::MAX,
        };
        let mut value = serde_json::to_value(&receipt).unwrap();
        assert_eq!(value["generation"], u64::MAX.to_string());
        assert_eq!(
            serde_json::from_value::<ContainerOperationReceipt>(value.clone()).unwrap(),
            receipt
        );
        for generation in [
            serde_json::json!(1),
            serde_json::json!("01"),
            serde_json::json!("18446744073709551616"),
        ] {
            value["generation"] = generation;
            assert!(serde_json::from_value::<ContainerOperationReceipt>(value.clone()).is_err());
        }
        value = serde_json::to_value(receipt).unwrap();
        value["credential"] = serde_json::json!("unexpected");
        assert!(serde_json::from_value::<ContainerOperationReceipt>(value).is_err());
    }
    #[test]
    fn pending_discovery_is_distinct_from_empty() {
        assert_ne!(
            serde_json::to_value(EndpointStatus::Pending).unwrap(),
            serde_json::to_value(EndpointStatus::Available { endpoints: vec![] }).unwrap()
        );
    }
}
