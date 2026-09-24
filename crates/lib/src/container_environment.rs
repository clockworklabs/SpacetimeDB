//! Immutable application environment identity, independent of expiring launch credentials.
//!
//! These values describe a request. They do not authenticate a client or grant
//! access to environment values. Only the trusted host/control protocol may
//! capture, read, or close a snapshot.

use crate::{Hash, Identity, SpacetimeType, Uuid};

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct EnvironmentSnapshotScope {
    pub cluster: String,
    pub database_id: u64,
    pub database_identity: Identity,
    pub node_id: u64,
    #[cfg_attr(feature = "serde", serde(with = "crate::deployment::uuid_json"))]
    pub node_incarnation: Uuid,
    pub generation: u64,
    pub deployment_revision: Hash,
    #[cfg_attr(feature = "serde", serde(with = "crate::deployment::uuid_json"))]
    pub publication_operation: Uuid,
    pub publication_epoch: u64,
    #[cfg_attr(feature = "serde", serde(with = "crate::deployment::uuid_json"))]
    pub start_request: Uuid,
    #[cfg_attr(feature = "serde", serde(with = "crate::deployment::uuid_json"))]
    pub env_generation: Uuid,
    /// The complete sorted, unique key list of the committed container declaration.
    pub env_keys: Vec<String>,
}

/// Stable identity of a committed capture. No secret values or value hashes.
/// A durability barrier belongs to each proof, not to this immutable receipt.
#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = crate)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct EnvironmentSnapshotReceipt {
    pub scope: EnvironmentSnapshotScope,
    #[cfg_attr(feature = "serde", serde(with = "crate::deployment::uuid_json"))]
    pub capture_receipt: Uuid,
}
