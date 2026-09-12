//! HTTP publication messages shared by the CLI, dashboard and Cloud. Artifacts
//! are uploaded separately. Complete environment values belong only to the
//! protected publication request; public status and manifests remain value-free.

use super::{
    manifest::PreparedDeploymentManifest, option_uuid_json, uuid_json, DeploymentSpec, PUBLISH_PROTOCOL_VERSION,
};
use crate::{container::OciDigest, Hash, Identity, Uuid};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod request_decode;
pub use request_decode::{PublishRequestError, MAX_PUBLISH_REQUEST_BYTES};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub digest: OciDigest,
    pub size_bytes: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishRequest {
    pub manifest: PreparedDeploymentManifest,
    /// Must match the server-generated reservation when creating a database.
    pub creation: Option<CreationOptions>,
    /// Original uploaded OCI index or executable manifest. Required for Set.
    /// Keep uses the prior retained executable manifest; Remove has no image.
    pub image_source: Option<ArtifactReference>,
    /// Private overrides. Omission preserves stored values.
    #[serde(default, deserialize_with = "request_decode::deserialize_environment")]
    pub environment: BTreeMap<String, String>,
    #[serde(default)]
    pub environment_remove: Vec<String>,
    #[serde(default)]
    pub environment_replace: bool,
}

impl std::fmt::Debug for PublishRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublishRequest")
            .field("manifest", &self.manifest)
            .field("creation", &self.creation)
            .field("image_source", &self.image_source)
            .field("environment", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationOptions {
    pub parent: Option<Identity>,
    pub organization: Option<Identity>,
    pub num_replicas: Option<u32>,
    #[serde(default = "default_anti_affinity")]
    pub enforce_anti_affinity: bool,
}
fn default_anti_affinity() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReserveDatabaseRequest {
    pub version: u32,
    #[serde(with = "uuid_json")]
    pub operation_id: Uuid,
    pub options: CreationOptions,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseReservation {
    pub database_identity: Identity,
    #[serde(with = "uuid_json")]
    pub operation_id: Uuid,
    pub expires_at: String,
    pub staging_open: bool,
    pub artifact_endpoint: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicationPhase {
    Prepared,
    Quiescing,
    Committed,
    Activating,
    Complete,
    AbortedBeforeCommit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationStatus {
    pub database_identity: Identity,
    #[serde(with = "uuid_json")]
    pub operation_id: Uuid,
    pub phase: PublicationPhase,
    pub expected_revision: Option<Hash>,
    #[serde(with = "option_uuid_json")]
    pub expected_last_operation: Option<Uuid>,
    pub publication_epoch: u64,
    pub proposed_revision: Hash,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentStatus {
    pub database_identity: Identity,
    /// None means this database has not yet used managed publication.
    pub revision: Option<Hash>,
    #[serde(with = "option_uuid_json")]
    pub last_operation: Option<Uuid>,
    pub deployment: DeploymentSpec,
    /// Lets Keep select exactly the currently installed program bytes.
    pub module_artifact: ArtifactReference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishPermission {
    pub identity: Identity,
    pub can_publish: bool,
    /// Decimal string preserves all u64 revision values in browser clients.
    pub source_revision: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationCapabilities {
    pub version: u32,
    pub enabled: bool,
    pub artifact_endpoint: Option<String>,
}
impl PublicationCapabilities {
    pub fn disabled() -> Self {
        Self {
            version: PUBLISH_PROTOCOL_VERSION,
            enabled: false,
            artifact_endpoint: None,
        }
    }
}
