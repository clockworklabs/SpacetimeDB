//! Transactional immutable container environment snapshots.
//!
//! These are host-only operations. Before invoking them, the adapter must
//! authenticate the assigned service and confirm the exact current control
//! intent and authoritative leader. Identity equality and a cached control row
//! are insufficient. Helpers retain the caller's transaction and do no IO.
//!
//! Historical restore must close admission and reconcile newer operational
//! fences before any snapshot request. A missing Ready snapshot is an error,
//! never permission to recapture values from a restored or current `st_env`.

use super::{
    deployment, environment,
    relational_db::{MutTx, RelationalDB},
};
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::system_tables::{
    StContainerEnvironmentRow, StContainerFenceRow, ST_CONTAINER_ENVIRONMENT_ID, ST_CONTAINER_FENCE_ID,
};
use spacetimedb_lib::container::{validate_env_key, validate_exec_size, ContainerSpec, MAX_ENV_KEYS};
use spacetimedb_lib::container_environment::{EnvironmentSnapshotReceipt, EnvironmentSnapshotScope};
use spacetimedb_lib::{bsatn, SpacetimeType, Uuid};
use spacetimedb_primitives::ColId;
use spacetimedb_sats::AlgebraicValue;
use std::{collections::BTreeMap, fmt};

pub const MAX_SNAPSHOT_BYTES: usize = 256 * 1024;
pub const MAX_RETAINED_SNAPSHOTS: u64 = 64;

/// Error text and Debug never include selected values or serialized records.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnvironmentSnapshotError {
    #[error("container environment request is invalid")]
    InvalidScope,
    #[error("container environment generation is fenced")]
    Fenced,
    #[error("container environment deployment revision does not match")]
    RevisionConflict,
    #[error("container environment snapshot belongs to another immutable scope")]
    ScopeConflict,
    #[error("container environment snapshot has not been captured")]
    NotCaptured,
    #[error("required container environment keys are missing: {0:?}")]
    MissingKeys(Vec<String>),
    #[error("container environment does not satisfy startup limits")]
    InvalidEnvironment,
    #[error("container environment snapshot capacity exhausted")]
    Capacity,
    #[error("container environment snapshot metadata is invalid")]
    CorruptMetadata,
    #[error("container environment storage operation failed")]
    Storage,
    #[error("container environment requires durable storage")]
    DurabilityUnavailable,
    #[error("container environment durability could not be confirmed")]
    DurabilityFailed,
}

impl From<crate::error::DBError> for EnvironmentSnapshotError {
    fn from(_: crate::error::DBError) -> Self {
        Self::Storage
    }
}

#[derive(Clone, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
enum Record {
    V1(RecordV1),
}

#[derive(Clone, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
struct RecordV1 {
    receipt: EnvironmentSnapshotReceipt,
    selected_values: Vec<CapturedValue>,
}

#[derive(Clone, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
struct CapturedValue {
    key: String,
    value: String,
}

/// Selected database values only. The trusted adapter merges verified image
/// defaults and current platform variables, then validates the complete exec.
pub struct SecretEnvironment {
    pub receipt: EnvironmentSnapshotReceipt,
    pub selected_values: BTreeMap<String, String>,
}

impl fmt::Debug for SecretEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretEnvironment")
            .field("receipt", &self.receipt)
            .field("selected_values", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentClosedReceipt {
    pub scope: EnvironmentSnapshotScope,
    pub closed_through_generation: u64,
}

fn valid_uuid(id: Uuid) -> bool {
    matches!(
        id.get_version(),
        Some(spacetimedb_sats::uuid::Version::V4 | spacetimedb_sats::uuid::Version::V7)
    )
}

fn validate_scope(db: &RelationalDB, scope: &EnvironmentSnapshotScope) -> Result<(), EnvironmentSnapshotError> {
    if db.database_identity() != scope.database_identity
        || scope.database_id == 0
        || scope.node_id == 0
        || scope.generation == 0
        || scope.cluster.is_empty()
        || scope.cluster.len() > 256
        || scope.cluster.contains('\0')
        || !valid_uuid(scope.node_incarnation)
        || !valid_uuid(scope.start_request)
        || !valid_uuid(scope.env_generation)
        || scope.env_keys.len() > MAX_ENV_KEYS
        || scope.env_keys.windows(2).any(|keys| keys[0] >= keys[1])
        || scope.env_keys.iter().any(|key| validate_env_key(key).is_err())
    {
        return Err(EnvironmentSnapshotError::InvalidScope);
    }
    Ok(())
}

fn fence(
    state: &impl StateView,
    scope: &EnvironmentSnapshotScope,
) -> Result<StContainerFenceRow, EnvironmentSnapshotError> {
    state
        .iter_by_col_eq(
            ST_CONTAINER_FENCE_ID,
            ColId(0),
            &AlgebraicValue::U256(scope.database_identity.to_u256().into()),
        )
        .map_err(|_| EnvironmentSnapshotError::Storage)?
        .next()
        .map(StContainerFenceRow::try_from)
        .transpose()
        .map_err(|_| EnvironmentSnapshotError::CorruptMetadata)?
        .ok_or(EnvironmentSnapshotError::Fenced)
}

fn admitted_spec(
    db: &RelationalDB,
    state: &impl StateView,
    scope: &EnvironmentSnapshotScope,
) -> Result<ContainerSpec, EnvironmentSnapshotError> {
    validate_scope(db, scope)?;
    let current = fence(state, scope)?;
    if current.generation != scope.generation || !current.allowed {
        return Err(EnvironmentSnapshotError::Fenced);
    }
    let (revision, deployment) = deployment::current_deployment(state)
        .map_err(|_| EnvironmentSnapshotError::CorruptMetadata)?
        .ok_or(EnvironmentSnapshotError::RevisionConflict)?;
    if revision != scope.deployment_revision {
        return Err(EnvironmentSnapshotError::RevisionConflict);
    }
    let spec = deployment
        .current()
        .container
        .as_ref()
        .ok_or(EnvironmentSnapshotError::RevisionConflict)?;
    if spec.env_keys != scope.env_keys {
        return Err(EnvironmentSnapshotError::ScopeConflict);
    }
    Ok(spec.clone())
}

fn lookup(
    state: &impl StateView,
    scope: &EnvironmentSnapshotScope,
) -> Result<Option<RecordV1>, EnvironmentSnapshotError> {
    let Some(row) = state
        .iter_by_col_eq(
            ST_CONTAINER_ENVIRONMENT_ID,
            ColId(0),
            &AlgebraicValue::U64(scope.generation),
        )
        .map_err(|_| EnvironmentSnapshotError::Storage)?
        .next()
        .map(StContainerEnvironmentRow::try_from)
        .transpose()
        .map_err(|_| EnvironmentSnapshotError::CorruptMetadata)?
    else {
        return Ok(None);
    };
    if row.payload.len() > MAX_SNAPSHOT_BYTES {
        return Err(EnvironmentSnapshotError::CorruptMetadata);
    }
    let Record::V1(record) = bsatn::from_slice(&row.payload).map_err(|_| EnvironmentSnapshotError::CorruptMetadata)?;
    if record.receipt.scope != *scope {
        return Err(EnvironmentSnapshotError::ScopeConflict);
    }
    if !valid_uuid(record.receipt.capture_receipt)
        || !record
            .selected_values
            .iter()
            .map(|entry| &entry.key)
            .eq(scope.env_keys.iter())
    {
        return Err(EnvironmentSnapshotError::CorruptMetadata);
    }
    Ok(Some(record))
}

fn validate_values(spec: &ContainerSpec, values: &BTreeMap<String, String>) -> Result<(), EnvironmentSnapshotError> {
    // The at-most-256 individually bounded values also bound this temporary allocation.
    if values
        .values()
        .any(|value| value.contains('\0') || spacetimedb_lib::environment::validate_value(value).is_err())
    {
        return Err(EnvironmentSnapshotError::InvalidEnvironment);
    }
    let env = values
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    validate_exec_size(&spec.argv, &env).map_err(|_| EnvironmentSnapshotError::InvalidEnvironment)
}

/// Capture exactly once under an open confirmed control intent. Returning this
/// receipt is not a durability acknowledgment; the host wrapper supplies that.
pub fn capture(
    db: &RelationalDB,
    tx: &mut MutTx,
    scope: &EnvironmentSnapshotScope,
) -> Result<EnvironmentSnapshotReceipt, EnvironmentSnapshotError> {
    let spec = admitted_spec(db, tx, scope)?;
    if let Some(record) = lookup(tx, scope)? {
        validate_values(
            &spec,
            &record
                .selected_values
                .into_iter()
                .map(|entry| (entry.key, entry.value))
                .collect(),
        )?;
        return Ok(record.receipt);
    }
    if tx
        .table_row_count(ST_CONTAINER_ENVIRONMENT_ID)
        .ok_or(EnvironmentSnapshotError::Storage)?
        >= MAX_RETAINED_SNAPSHOTS
    {
        return Err(EnvironmentSnapshotError::Capacity);
    }
    let mut values = BTreeMap::new();
    let mut missing = Vec::new();
    for key in &scope.env_keys {
        match environment::get(tx, key).map_err(|_| EnvironmentSnapshotError::Storage)? {
            Some(value) => {
                values.insert(key.clone(), value);
            }
            None => missing.push(key.clone()),
        }
    }
    if !missing.is_empty() {
        return Err(EnvironmentSnapshotError::MissingKeys(missing));
    }
    validate_values(&spec, &values)?;
    let receipt = EnvironmentSnapshotReceipt {
        scope: scope.clone(),
        capture_receipt: Uuid::from_u128(uuid::Uuid::new_v4().as_u128()),
    };
    let payload = bsatn::to_vec(&Record::V1(RecordV1 {
        receipt: receipt.clone(),
        selected_values: values
            .into_iter()
            .map(|(key, value)| CapturedValue { key, value })
            .collect(),
    }))
    .map_err(|_| EnvironmentSnapshotError::CorruptMetadata)?;
    if payload.len() > MAX_SNAPSHOT_BYTES {
        return Err(EnvironmentSnapshotError::Capacity);
    }
    tx.insert_via_serialize_bsatn(
        ST_CONTAINER_ENVIRONMENT_ID,
        &StContainerEnvironmentRow {
            generation: scope.generation,
            payload: payload.into(),
        },
    )
    .map_err(|_| EnvironmentSnapshotError::Storage)?;
    Ok(receipt)
}

/// Read only an existing receipt after fresh exact control/lease confirmation.
pub fn read(
    db: &RelationalDB,
    state: &impl StateView,
    receipt: &EnvironmentSnapshotReceipt,
) -> Result<SecretEnvironment, EnvironmentSnapshotError> {
    let spec = admitted_spec(db, state, &receipt.scope)?;
    let record = lookup(state, &receipt.scope)?.ok_or(EnvironmentSnapshotError::NotCaptured)?;
    if record.receipt != *receipt {
        return Err(EnvironmentSnapshotError::ScopeConflict);
    }
    let selected_values = record
        .selected_values
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect();
    validate_values(&spec, &selected_values)?;
    Ok(SecretEnvironment {
        receipt: record.receipt,
        selected_values,
    })
}

/// Delete only after a newer own-source fence irreversibly rejects every old
/// capture. The adapter also confirms positive historical control closure.
/// Missing rows remain idempotently closed; no per-UUID TTL is an authority.
pub fn close(
    db: &RelationalDB,
    tx: &mut MutTx,
    scope: &EnvironmentSnapshotScope,
    closed_through_generation: u64,
) -> Result<EnvironmentClosedReceipt, EnvironmentSnapshotError> {
    validate_scope(db, scope)?;
    let current = fence(tx, scope)?;
    if closed_through_generation <= scope.generation || current.generation < closed_through_generation {
        return Err(EnvironmentSnapshotError::Fenced);
    }
    if lookup(tx, scope)?.is_some() {
        let pointer = tx
            .iter_by_col_eq(
                ST_CONTAINER_ENVIRONMENT_ID,
                ColId(0),
                &AlgebraicValue::U64(scope.generation),
            )
            .map_err(|_| EnvironmentSnapshotError::Storage)?
            .next()
            .ok_or(EnvironmentSnapshotError::Storage)?
            .pointer();
        db.delete(tx, ST_CONTAINER_ENVIRONMENT_ID, [pointer]);
    }
    Ok(EnvironmentClosedReceipt {
        scope: scope.clone(),
        closed_through_generation,
    })
}

#[cfg(test)]
mod tests;
