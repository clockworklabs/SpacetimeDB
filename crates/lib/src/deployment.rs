//! Normalized deployment protocol shared by publication coordinators and hosts.
//!
//! Module artifacts and OCI objects are uploaded separately. A deployment is
//! immutable configuration, never a place for runtime credentials or env values.

use crate::container::{ContainerAction, ContainerSpec, ContainerSpecLimits, ContainerValidationError};
use crate::{bsatn, hash_bytes, Hash, SpacetimeType, Uuid};

#[cfg(feature = "serde")]
pub mod api;
pub mod manifest;
pub mod system_empty;
pub use system_empty::SystemEmptyModule;

pub const PUBLISH_PROTOCOL_VERSION: u32 = 1;
pub const SYSTEM_EMPTY_MODULE_VERSION: u32 = system_empty::VERSION;
pub const MAX_DEPLOYMENT_BYTES: usize = 256 * 1024;
pub const PUBLISH_RETRY_WINDOW_MS: u64 = 7 * 24 * 60 * 60 * 1000;
pub const MAX_OPERATION_CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum UserModuleKind {
    Wasm,
    Js,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct UserModule {
    pub kind: UserModuleKind,
    /// The existing module program hash, not an OCI object digest.
    pub program_hash: Hash,
}

/// Explicit module replacement/removal, with its own exported schema name.
#[derive(Clone, Debug, Default, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "action", content = "value", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum ModuleAction {
    #[default]
    Keep,
    Set(UserModule),
    /// Remove user code and select the exact platform module generated from
    /// container configuration. Schema changes use this explicit action too.
    Remove(SystemEmptyModule),
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum ModuleComponent {
    SystemEmpty(SystemEmptyModule),
    User(UserModule),
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct DeploymentSpecV1 {
    pub module: ModuleComponent,
    pub container: Option<ContainerSpec>,
}

/// Persist the discriminant along with the payload. Unknown encodings fail to
/// decode; a restore must never fall back to an empty or older deployment.
#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "version",
        content = "deployment",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum DeploymentSpec {
    V1(DeploymentSpecV1),
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PublishEnvelope {
    pub version: u32,
    #[cfg_attr(feature = "serde", serde(with = "uuid_json"))]
    pub operation_id: Uuid,
    /// Exact compare-and-set precondition. None means no deployment record has
    /// been installed yet, rather than permission to overwrite any revision.
    pub expected_revision: Option<Hash>,
    /// Last committed operation paired with the revision; ENV-only publications
    /// can retain the same revision while changing this cursor.
    #[cfg_attr(feature = "serde", serde(with = "option_uuid_json"))]
    pub expected_last_operation: Option<Uuid>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub module_action: ModuleAction,
    #[cfg_attr(feature = "serde", serde(default))]
    pub container_action: ContainerAction,
}

#[derive(Debug, thiserror::Error)]
pub enum DeploymentValidationError {
    #[error("unsupported publish protocol version")]
    UnsupportedVersion,
    #[error("unsupported platform empty-module version")]
    UnsupportedEmptyModule,
    #[error("operation_id must be a version 7 UUID")]
    InvalidOperationId,
    #[error("publication operation has expired")]
    ExpiredOperation,
    #[error("publication operation timestamp is too far in the future")]
    FutureOperation,
    #[error("deployment exceeds the protocol size limit")]
    TooLarge,
    #[error("deployment encoding is invalid or unsupported")]
    InvalidEncoding,
    #[error(transparent)]
    Container(#[from] ContainerValidationError),
}

impl DeploymentSpec {
    pub fn current(&self) -> &DeploymentSpecV1 {
        match self {
            Self::V1(spec) => spec,
        }
    }

    pub fn normalize(self, limits: &ContainerSpecLimits) -> Result<Self, DeploymentValidationError> {
        let Self::V1(mut spec) = self;
        if let ModuleComponent::SystemEmpty(module) = &spec.module
            && module.version != SYSTEM_EMPTY_MODULE_VERSION
        {
            return Err(DeploymentValidationError::UnsupportedEmptyModule);
        }
        spec.container = spec.container.map(|spec| spec.normalize(limits)).transpose()?;
        let spec = Self::V1(spec);
        spec.encode()?;
        Ok(spec)
    }

    pub fn encode(&self) -> Result<Box<[u8]>, DeploymentValidationError> {
        let bytes = bsatn::to_vec(self).map_err(|_| DeploymentValidationError::InvalidEncoding)?;
        if bytes.len() > MAX_DEPLOYMENT_BYTES {
            return Err(DeploymentValidationError::TooLarge);
        }
        Ok(bytes.into())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DeploymentValidationError> {
        if bytes.len() > MAX_DEPLOYMENT_BYTES {
            return Err(DeploymentValidationError::TooLarge);
        }
        bsatn::from_slice(bytes).map_err(|_| DeploymentValidationError::InvalidEncoding)
    }

    /// Call on the result of normalize. Unlike a request fingerprint, this is
    /// independent of operation ID, publisher, and mutable execution status.
    pub fn revision(&self) -> Result<Hash, DeploymentValidationError> {
        let mut bytes = b"spacetimedb/deployment\0".to_vec();
        bytes.extend_from_slice(&self.encode()?);
        Ok(hash_bytes(bytes))
    }
}

impl PublishEnvelope {
    pub fn resolve(
        &self,
        previous: Option<&DeploymentSpec>,
        limits: &ContainerSpecLimits,
    ) -> Result<DeploymentSpec, DeploymentValidationError> {
        if self.version != PUBLISH_PROTOCOL_VERSION {
            return Err(DeploymentValidationError::UnsupportedVersion);
        }
        use spacetimedb_sats::uuid::Version;
        if !matches!(self.operation_id.get_version(), Some(Version::V7)) {
            return Err(DeploymentValidationError::InvalidOperationId);
        }
        if self.expected_revision.is_some() != self.expected_last_operation.is_some() {
            return Err(DeploymentValidationError::InvalidEncoding);
        }
        if self
            .expected_last_operation
            .is_some_and(|id| id.get_version() != Some(Version::V7))
        {
            return Err(DeploymentValidationError::InvalidOperationId);
        }
        let prior = previous.map(DeploymentSpec::current);
        let module = match &self.module_action {
            ModuleAction::Keep => prior
                .map(|p| p.module.clone())
                .unwrap_or(ModuleComponent::SystemEmpty(system_empty::empty().descriptor)),
            ModuleAction::Set(module) => ModuleComponent::User(module.clone()),
            ModuleAction::Remove(module) => ModuleComponent::SystemEmpty(*module),
        };
        let container = match &self.container_action {
            ContainerAction::Keep => prior.and_then(|p| p.container.clone()),
            ContainerAction::Set(container) => Some(container.clone()),
            ContainerAction::Remove => None,
        };
        DeploymentSpec::V1(DeploymentSpecV1 { module, container }).normalize(limits)
    }

    pub fn requires_container_permission(&self) -> bool {
        matches!(self.container_action, ContainerAction::Set(_))
    }
}

/// UUIDv7 embeds its creation millisecond. This makes an expired retry
/// distinguishable from a new request even after its ledger row is collected.
/// A fresh UUID with a changed timestamp is a different operation.
pub fn operation_expiry_ms(id: Uuid, now_ms: u64) -> Result<u64, DeploymentValidationError> {
    if id.get_version() != Some(spacetimedb_sats::uuid::Version::V7) {
        return Err(DeploymentValidationError::InvalidOperationId);
    }
    let created_ms = (id.as_u128() >> 80) as u64;
    if created_ms > now_ms.saturating_add(MAX_OPERATION_CLOCK_SKEW_MS) {
        return Err(DeploymentValidationError::FutureOperation);
    }
    let expires_ms = created_ms + PUBLISH_RETRY_WINDOW_MS;
    if now_ms >= expires_ms {
        return Err(DeploymentValidationError::ExpiredOperation);
    }
    Ok(expires_ms)
}

#[cfg(feature = "serde")]
pub mod uuid_json {
    use super::Uuid;
    pub fn serialize<S: serde::Serializer>(id: &Uuid, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(id)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Uuid, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Uuid::parse_str(&value).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
pub mod option_uuid_json {
    use super::Uuid;
    pub fn serialize<S: serde::Serializer>(id: &Option<Uuid>, serializer: S) -> Result<S::Ok, S::Error> {
        match id {
            Some(id) => serializer.serialize_some(&id.to_string()),
            None => serializer.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<Uuid>, D::Error> {
        let value = <Option<String> as serde::Deserialize>::deserialize(deserializer)?;
        value
            .map(|value| Uuid::parse_str(&value).map_err(serde::de::Error::custom))
            .transpose()
    }
}

#[cfg(test)]
mod tests;
