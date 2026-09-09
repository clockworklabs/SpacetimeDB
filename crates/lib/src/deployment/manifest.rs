//! Immutable publication recovery inputs. The manifest's SHA-256 artifact
//! digest binds migration intent as well as the effective deployment. Its
//! digest differs from the deployment revision, which excludes migration policy.

use super::{DeploymentSpec, DeploymentValidationError, PublishEnvelope, MAX_DEPLOYMENT_BYTES};
use crate::container::{ContainerSpecLimits, OciDigest};
use crate::{bsatn, Hash, SpacetimeType};

pub const MAX_MODULE_ARTIFACT_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ModuleArtifact {
    /// SHA-256 of the complete stored bytes, distinct from the module's Keccak hash.
    pub digest: OciDigest,
    pub size_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "policy", content = "token", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum PreparedMigrationPolicy {
    Compatible,
    /// The existing migration token binds database Identity and old/new module
    /// hashes. Recovery must retain the originally acknowledged policy.
    BreakClients(Hash),
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PreparedDeploymentManifestV1 {
    pub envelope: PublishEnvelope,
    pub deployment: DeploymentSpec,
    /// Always retained, including for the bundled empty module. Genesis and
    /// later recovery must select exactly the admitted program bytes.
    pub module_artifact: ModuleArtifact,
    pub migration_policy: PreparedMigrationPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "version",
        content = "manifest",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum PreparedDeploymentManifest {
    V1(PreparedDeploymentManifestV1),
}

impl PreparedDeploymentManifest {
    pub fn current(&self) -> &PreparedDeploymentManifestV1 {
        match self {
            Self::V1(manifest) => manifest,
        }
    }

    /// Validate before retaining the artifact. This checks the encoding and
    /// metadata; the artifact service verifies SHA-256/length and the host
    /// validates the selected program, capabilities and actual migration.
    pub fn validate(&self, limits: &ContainerSpecLimits) -> Result<(), DeploymentValidationError> {
        let manifest = self.current();
        if manifest.module_artifact.size_bytes == 0 || manifest.module_artifact.size_bytes > MAX_MODULE_ARTIFACT_BYTES {
            return Err(DeploymentValidationError::TooLarge);
        }
        if manifest.deployment.clone().normalize(limits)? != manifest.deployment {
            return Err(DeploymentValidationError::InvalidEncoding);
        }
        // Using the prepared components as the Keep baseline verifies every
        // explicit Set/Remove action without inventing a prior deployment.
        // The host separately checks the real prior state under its fence.
        if manifest.envelope.resolve(Some(&manifest.deployment), limits)? != manifest.deployment {
            return Err(DeploymentValidationError::InvalidEncoding);
        }
        self.encode()?;
        Ok(())
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
        let manifest: Self = bsatn::from_slice(bytes).map_err(|_| DeploymentValidationError::InvalidEncoding)?;
        // Reject trailing or noncanonical bytes even if the decoder accepts
        // them, so every retained descriptor names one unambiguous manifest.
        if manifest.encode()?.as_ref() != bytes {
            return Err(DeploymentValidationError::InvalidEncoding);
        }
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deployment::{DeploymentSpecV1, ModuleComponent};

    fn manifest() -> PreparedDeploymentManifest {
        PreparedDeploymentManifest::V1(PreparedDeploymentManifestV1 {
            envelope: PublishEnvelope {
                version: super::super::PUBLISH_PROTOCOL_VERSION,
                operation_id: crate::Uuid::from_u128(0x01991ec4000070008000000000000001),
                expected_revision: None,
                module_action: super::super::ModuleAction::Keep,
                container_action: crate::container::ContainerAction::Keep,
            },
            deployment: DeploymentSpec::V1(DeploymentSpecV1 {
                module: ModuleComponent::SystemEmpty(crate::deployment::system_empty::empty().descriptor),
                container: None,
            }),
            module_artifact: ModuleArtifact {
                digest: OciDigest::sha256([17; 32]),
                size_bytes: 250,
            },
            migration_policy: PreparedMigrationPolicy::Compatible,
        })
    }

    #[test]
    fn retained_manifest_preserves_migration_intent_without_changing_revision() {
        let first = manifest();
        let mut acknowledged = first.clone();
        let PreparedDeploymentManifest::V1(value) = &mut acknowledged;
        value.migration_policy = PreparedMigrationPolicy::BreakClients(Hash::from_byte_array([32; 32]));
        assert_eq!(
            first.current().deployment.revision().unwrap(),
            acknowledged.current().deployment.revision().unwrap()
        );
        let encoded = acknowledged.encode().unwrap();
        assert_ne!(first.encode().unwrap(), encoded);
        assert_eq!(PreparedDeploymentManifest::decode(&encoded).unwrap(), acknowledged);
        acknowledged.validate(&Default::default()).unwrap();
    }

    #[test]
    fn retained_manifest_rejects_unknown_encoding_and_invalid_module_bounds() {
        let mut value = manifest();
        let mut trailing = value.encode().unwrap().to_vec();
        trailing.push(0);
        assert!(PreparedDeploymentManifest::decode(&trailing).is_err());
        let mut unknown = value.encode().unwrap().to_vec();
        unknown[0] = 1;
        assert!(PreparedDeploymentManifest::decode(&unknown).is_err());
        let PreparedDeploymentManifest::V1(inner) = &mut value;
        inner.module_artifact.size_bytes = 0;
        assert!(value.validate(&Default::default()).is_err());
        let PreparedDeploymentManifest::V1(inner) = &mut value;
        inner.module_artifact.size_bytes = MAX_MODULE_ARTIFACT_BYTES + 1;
        assert!(value.validate(&Default::default()).is_err());
    }
}
