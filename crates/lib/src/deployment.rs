//! Normalized deployment protocol shared by publication coordinators and hosts.
//!
//! Module artifacts and OCI objects are uploaded separately. A deployment is
//! immutable configuration, never a place for runtime credentials or env values.

use crate::container::{ContainerAction, ContainerSpec, ContainerSpecLimits, ContainerValidationError};
use crate::{bsatn, hash_bytes, Hash, SpacetimeType, Uuid};

#[cfg(feature = "serde")]
pub mod api;
pub mod manifest;

pub const PUBLISH_PROTOCOL_VERSION: u32 = 1;
pub const SYSTEM_EMPTY_MODULE_VERSION: u32 = 1;
/// Immutable built-in program, also used by clients for authorized migration
/// preflight. Replacing these bytes requires a new system-module version.
pub const SYSTEM_EMPTY_MODULE_V1_BYTES: &[u8] = include_bytes!("deployment/system_empty_v1.wasm");
/// Immutable Keccak-256 program identity of the version-1 bundled empty Wasm
/// module. Control can verify initial program bytes without linking the host.
pub const SYSTEM_EMPTY_MODULE_V1_PROGRAM_HASH: Hash = Hash::from_byte_array([
    0x83, 0xcc, 0x1c, 0xc8, 0x79, 0x4f, 0x7a, 0x9a, 0x54, 0x0a, 0x07, 0x43, 0xd0, 0xf8, 0x74, 0xbf, 0x61, 0x3b, 0x76,
    0x67, 0x6c, 0xa9, 0xe8, 0xec, 0xbd, 0xc4, 0xcf, 0xca, 0x19, 0xc7, 0x09, 0xa5,
]);
/// SHA-256 descriptor of the same immutable bundled version-1 bytes. Clients
/// can name SystemEmpty in a prepared manifest without linking the host.
pub const SYSTEM_EMPTY_MODULE_V1_ARTIFACT: manifest::ModuleArtifact = manifest::ModuleArtifact {
    digest: crate::container::OciDigest::sha256([
        0x02, 0xbd, 0x77, 0xcf, 0x25, 0xb4, 0x79, 0x08, 0xf4, 0x06, 0x4c, 0x3d, 0xdf, 0x0f, 0xa7, 0xec, 0xcc, 0xa8,
        0xc6, 0x16, 0x5d, 0x0a, 0x41, 0xc4, 0x45, 0x54, 0x69, 0x54, 0x6b, 0x14, 0xce, 0x02,
    ]),
    size_bytes: 250,
};
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
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", content = "value", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum ModuleComponent {
    SystemEmpty(u32),
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
        if let ModuleComponent::SystemEmpty(version) = spec.module
            && version != SYSTEM_EMPTY_MODULE_VERSION
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
        let prior = previous.map(DeploymentSpec::current);
        let module = match &self.module_action {
            ModuleAction::Keep => prior
                .map(|p| p.module.clone())
                .unwrap_or(ModuleComponent::SystemEmpty(SYSTEM_EMPTY_MODULE_VERSION)),
            ModuleAction::Set(module) => ModuleComponent::User(module.clone()),
            ModuleAction::Remove => ModuleComponent::SystemEmpty(SYSTEM_EMPTY_MODULE_VERSION),
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

#[cfg(test)]
mod tests;
