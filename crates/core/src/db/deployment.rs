//! Transactional deployment and hosted-client fences.
//!
//! Only authenticated host operations may call the mutation functions here.
//! Callers retain the same serializable transaction through module migration,
//! deployment recording, and commit. These functions do not contact control,
//! perform process IO, or turn an unverified client Identity into host authority.

use super::relational_db::{MutTx, RelationalDB};
use crate::error::DBError;
use spacetimedb_datastore::error::DatastoreError;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::system_tables::{
    ConnectionIdViaU128, StConnectionAuthRow, StContainerFenceRow, StDeploymentOperationRow, StDeploymentRow,
    StPublishFenceRow, ST_CONNECTION_AUTH_ID, ST_CONTAINER_FENCE_ID, ST_DEPLOYMENT_ID, ST_DEPLOYMENT_OPERATION_ID,
    ST_PUBLISH_FENCE_ID,
};
use spacetimedb_lib::container::ContainerSpecLimits;
use spacetimedb_lib::deployment::{operation_expiry_ms, DeploymentSpec, DeploymentValidationError};
use spacetimedb_lib::{bsatn, hash_bytes, ConnectionId, Hash, Identity, SpacetimeType, Timestamp, Uuid};
use spacetimedb_primitives::{ColId, TableId};
use spacetimedb_sats::AlgebraicValue;

#[derive(Debug, thiserror::Error)]
pub enum DeploymentError {
    #[error("this database requires the deployment publication protocol")]
    CoordinatorRequired,
    #[error("the supplied module does not match the prepared deployment")]
    ProgramMismatch,
    #[error("the prepared module must advertise hosted_auth_v1 to attach a container")]
    UnsupportedHostedModule,
    #[error("the publication coordinator no longer owns the database fence")]
    PublicationFenced,
    #[error("the expected deployment revision does not match the database")]
    RevisionConflict,
    #[error("operation ID is already bound to a different publication")]
    OperationConflict,
    #[error("container generation or grant is not authorized by this database")]
    ContainerFenced,
    #[error("a conflicting or older container fence cannot replace current authority")]
    FenceConflict,
    #[error("the receiving host fence revision is exhausted")]
    FenceRevisionExhausted,
    #[error("deployment metadata is inconsistent")]
    CorruptMetadata,
    #[error(transparent)]
    Validation(#[from] DeploymentValidationError),
    #[error(transparent)]
    Datastore(#[from] DatastoreError),
    #[error(transparent)]
    Database(#[from] DBError),
}

/// Prepared by the authorized coordinator after recording durable intent.
/// The publisher is its verified original caller, never a guest-selected claim.
#[derive(Clone, Debug)]
pub struct DeploymentCommit {
    pub operation_id: Uuid,
    pub publication_epoch: u64,
    pub publisher: Identity,
    pub expected_revision: Option<Hash>,
    pub prepared_manifest_hash: Hash,
    pub deployment: DeploymentSpec,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishResult {
    #[serde(with = "spacetimedb_lib::deployment::uuid_json")]
    pub operation_id: Uuid,
    pub previous_revision: Option<Hash>,
    pub revision: Hash,
}

#[derive(Clone, Debug, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
enum CommitReceipt {
    V1(CommitReceiptV1),
}

#[derive(Clone, Debug, SpacetimeType)]
#[sats(crate = spacetimedb_lib)]
struct CommitReceiptV1 {
    request_hash: Hash,
    publisher: Identity,
    result: PublishResult,
}

#[derive(Clone, Debug)]
pub enum CommitAdmission {
    /// Return this result without repeating module init/update or any effects.
    AlreadyCommitted(PublishResult),
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AbortResult {
    /// The commit won the race. Recovery must converge on this deployment.
    AlreadyCommitted(PublishResult),
    /// This epoch can no longer admit a commit, and the prior revision remains.
    Aborted { previous_revision: Option<Hash> },
}

/// Check the actual program selected by the host, not a caller-provided
/// capability bit. The program bytes are hashed here because `Program` also
/// has a public constructor which accepts a previously computed hash.
pub fn validate_deployment_program(
    request: &DeploymentCommit,
    program: &spacetimedb_datastore::traits::Program,
    module: &spacetimedb_schema::def::ModuleDef,
) -> Result<(), DeploymentError> {
    use spacetimedb_datastore::system_tables::ModuleKind;
    use spacetimedb_lib::deployment::{ModuleComponent, UserModuleKind};
    if hash_bytes(&program.bytes) != program.hash {
        return Err(DeploymentError::ProgramMismatch);
    }
    match &request.deployment.current().module {
        ModuleComponent::User(expected)
            if expected.program_hash == program.hash
                && matches!(
                    (expected.kind, program.kind),
                    (UserModuleKind::Wasm, ModuleKind::WASM) | (UserModuleKind::Js, ModuleKind::JS)
                ) => {}
        ModuleComponent::SystemEmpty(module) if crate::host::empty_module::matches_program(module, program) => {}
        _ => return Err(DeploymentError::ProgramMismatch),
    }
    if request.deployment.current().container.is_some() && !module.supports_hosted_auth_v1() {
        return Err(DeploymentError::UnsupportedHostedModule);
    }
    Ok(())
}

/// Legacy raw module publication must be routed through the coordinator once
/// a database has deployment metadata or a publication fence, including after
/// an attempt aborts or its container is removed. In particular a legacy
/// request cannot race the first prepared container publication.
/// Call while holding the transaction that changes the program/schema.
pub fn require_unmanaged_publication(tx: &MutTx) -> Result<(), DeploymentError> {
    if current_deployment(tx)?.is_some() || singleton(tx, ST_PUBLISH_FENCE_ID)?.is_some() {
        return Err(DeploymentError::CoordinatorRequired);
    }
    Ok(())
}

/// Foreign callers depend on the receiving module's bindings just as self
/// callers do. A module replacement cannot silently erase that capability
/// while an admitted generation can still address the database.
pub fn validate_active_hosted_grants(
    tx: &MutTx,
    module: &spacetimedb_schema::def::ModuleDef,
) -> Result<(), DeploymentError> {
    if !module.supports_hosted_auth_v1() {
        for row in tx.iter(ST_CONTAINER_FENCE_ID)? {
            if StContainerFenceRow::try_from(row)?.allowed {
                return Err(DeploymentError::UnsupportedHostedModule);
            }
        }
    }
    Ok(())
}

fn singleton<S: StateView>(
    state: &S,
    table: TableId,
) -> Result<Option<spacetimedb_table::table::RowRef<'_>>, DeploymentError> {
    Ok(state.iter_by_col_eq(table, ColId(0), &AlgebraicValue::U8(0))?.next())
}

pub fn current_deployment<S: StateView>(state: &S) -> Result<Option<(Hash, DeploymentSpec)>, DeploymentError> {
    let Some(row) = singleton(state, ST_DEPLOYMENT_ID)? else {
        return Ok(None);
    };
    let row = StDeploymentRow::try_from(row)?;
    let spec = DeploymentSpec::decode(&row.payload)?;
    if spec.revision()? != row.revision {
        return Err(DeploymentError::CorruptMetadata);
    }
    Ok(Some((row.revision, spec)))
}

/// Monotonic compare-and-set, serialized against user-database commits.
/// Advancing this epoch does not itself quiesce a container or authorize launch.
pub fn install_publication_fence(
    tx: &mut MutTx,
    publication_epoch: u64,
    operation_id: Uuid,
) -> Result<(), DeploymentError> {
    if publication_epoch == 0 || operation_id == Uuid::NIL {
        return Err(DeploymentError::PublicationFenced);
    }
    let next = StPublishFenceRow {
        key: 0,
        publication_epoch,
        operation_id: operation_id.as_u128(),
    };
    if let Some(current) = singleton(tx, ST_PUBLISH_FENCE_ID)?
        .map(StPublishFenceRow::try_from)
        .transpose()?
    {
        if current == next {
            return Ok(());
        }
        if current.publication_epoch >= publication_epoch {
            return Err(DeploymentError::PublicationFenced);
        }
    }
    tx.clear_table(ST_PUBLISH_FENCE_ID)?;
    tx.insert_via_serialize_bsatn(ST_PUBLISH_FENCE_ID, &next)?;
    Ok(())
}

/// Close an admitted publication before reporting an abort to control. Run in
/// one serializable transaction and await its durability before releasing the
/// control operation or resuming the previous container under a fresh generation.
/// A delayed commit and this transaction serialize on the same database fence.
///
/// The nil operation ID is a closed-epoch marker, never a publish operation.
/// Keeping the epoch makes closure irreversible at that epoch while allowing
/// the next control-allocated epoch to install its own operation normally.
pub fn abort_deployment_commit(tx: &mut MutTx, request: &DeploymentCommit) -> Result<AbortResult, DeploymentError> {
    if let Some(result) = committed_deployment_operation(tx, request)? {
        return Ok(AbortResult::AlreadyCommitted(result));
    }
    // Recovery must be able to close an expired operation too. Its expiry
    // prevents new commits, but cannot substitute for a durable abort fence.
    // Control owns the immutable epoch-to-operation mapping; authenticate and
    // resolve that recorded operation before calling this function.
    let fence = singleton(tx, ST_PUBLISH_FENCE_ID)?
        .map(StPublishFenceRow::try_from)
        .transpose()?;
    let fence = fence
        .filter(|row| {
            row.publication_epoch == request.publication_epoch
                && (row.operation_id == request.operation_id.as_u128() || row.operation_id == 0)
        })
        .ok_or(DeploymentError::PublicationFenced)?;
    if current_deployment(tx)?.map(|(revision, _)| revision) != request.expected_revision {
        return Err(DeploymentError::RevisionConflict);
    }
    if fence.operation_id == 0 {
        return Ok(AbortResult::Aborted {
            previous_revision: request.expected_revision,
        });
    }
    tx.clear_table(ST_PUBLISH_FENCE_ID)?;
    tx.insert_via_serialize_bsatn(
        ST_PUBLISH_FENCE_ID,
        &StPublishFenceRow {
            key: 0,
            publication_epoch: request.publication_epoch,
            operation_id: 0,
        },
    )?;
    Ok(AbortResult::Aborted {
        previous_revision: request.expected_revision,
    })
}

fn normalized_request(
    request: &DeploymentCommit,
    limits: &ContainerSpecLimits,
) -> Result<(DeploymentSpec, Hash, Hash), DeploymentError> {
    let spec = request.deployment.clone().normalize(limits)?;
    if spec != request.deployment {
        return Err(DeploymentValidationError::InvalidEncoding.into());
    }
    let (revision, request_hash) = request_identity(request)?;
    Ok((spec, revision, request_hash))
}

fn request_identity(request: &DeploymentCommit) -> Result<(Hash, Hash), DeploymentError> {
    if request.operation_id.get_version() != Some(spacetimedb_sats::uuid::Version::V7) {
        return Err(DeploymentValidationError::InvalidOperationId.into());
    }
    if request.publication_epoch == 0 {
        return Err(DeploymentError::PublicationFenced);
    }
    // These bytes were normalized at admission. Do not re-apply today's
    // resource eligibility when inspecting yesterday's committed outcome.
    let revision = request.deployment.revision()?;
    let mut bytes = b"spacetimedb/deployment-operation\0".to_vec();
    bytes.extend_from_slice(
        &bsatn::to_vec(&(
            request.operation_id,
            request.publication_epoch,
            request.publisher,
            request.expected_revision,
            request.prepared_manifest_hash,
            revision,
        ))
        .map_err(|_| DeploymentError::CorruptMetadata)?,
    );
    Ok((revision, hash_bytes(bytes)))
}

/// Call before module execution, while holding the transaction later used for
/// migration. A cache or pre-enqueue check cannot replace this admission check.
pub fn check_deployment_commit(
    tx: &MutTx,
    request: &DeploymentCommit,
    now: Timestamp,
    limits: &ContainerSpecLimits,
) -> Result<CommitAdmission, DeploymentError> {
    let now_ms = u64::try_from(now.to_micros_since_unix_epoch()).map_err(|_| DeploymentError::CorruptMetadata)? / 1000;
    operation_expiry_ms(request.operation_id, now_ms)?;
    if let Some(result) = committed_deployment_operation(tx, request)? {
        return Ok(CommitAdmission::AlreadyCommitted(result));
    }
    normalized_request(request, limits)?;
    let fence = singleton(tx, ST_PUBLISH_FENCE_ID)?
        .map(StPublishFenceRow::try_from)
        .transpose()?;
    if !fence.is_some_and(|f| {
        f.publication_epoch == request.publication_epoch && f.operation_id == request.operation_id.as_u128()
    }) {
        return Err(DeploymentError::PublicationFenced);
    }
    if current_deployment(tx)?.map(|(revision, _)| revision) != request.expected_revision {
        return Err(DeploymentError::RevisionConflict);
    }
    Ok(CommitAdmission::Ready)
}

/// Inspect the exact retained commit receipt during host recovery. Unlike
/// admitting a client retry, inspecting an existing outcome does not expire.
/// This never authorizes module execution. An absent receipt is not proof of
/// abort: close the epoch atomically before reporting an abort to control.
/// Retain active operations' receipts until their control recovery completes.
pub fn committed_deployment_operation<S: StateView>(
    state: &S,
    request: &DeploymentCommit,
) -> Result<Option<PublishResult>, DeploymentError> {
    let (revision, request_hash) = request_identity(request)?;
    let operation_key = AlgebraicValue::U128(request.operation_id.as_u128().into());
    if let Some(row) = state
        .iter_by_col_eq(ST_DEPLOYMENT_OPERATION_ID, ColId(0), &operation_key)?
        .next()
    {
        let row = StDeploymentOperationRow::try_from(row)?;
        let CommitReceipt::V1(receipt) =
            bsatn::from_slice(&row.commit_result).map_err(|_| DeploymentError::CorruptMetadata)?;
        if receipt.request_hash != request_hash || receipt.publisher != request.publisher {
            return Err(DeploymentError::OperationConflict);
        }
        if receipt.result.revision != revision
            || row.committed_revision != revision
            || row.previous_revision != receipt.result.previous_revision
            || receipt.result.operation_id != request.operation_id
        {
            return Err(DeploymentError::CorruptMetadata);
        }
        return Ok(Some(receipt.result));
    }
    Ok(None)
}

/// Recognize the durable closed marker when recovering a control operation
/// whose abort report was lost. The authenticated caller must resolve control's
/// immutable epoch-to-operation binding before using this host-only API.
pub fn deployment_publication_aborted<S: StateView>(
    state: &S,
    request: &DeploymentCommit,
) -> Result<bool, DeploymentError> {
    request_identity(request)?;
    let fence = singleton(state, ST_PUBLISH_FENCE_ID)?
        .map(StPublishFenceRow::try_from)
        .transpose()?;
    if !fence.is_some_and(|row| row.publication_epoch == request.publication_epoch && row.operation_id == 0) {
        return Ok(false);
    }
    if current_deployment(state)?.map(|(revision, _)| revision) != request.expected_revision {
        return Err(DeploymentError::RevisionConflict);
    }
    Ok(true)
}

/// Record after successful module initialization/migration in that same
/// transaction. An error must roll back the entire transaction, including the
/// module changes. The caller waits for durability before reporting acceptance.
pub fn record_deployment_commit(
    tx: &mut MutTx,
    request: &DeploymentCommit,
    now: Timestamp,
    limits: &ContainerSpecLimits,
) -> Result<PublishResult, DeploymentError> {
    if let CommitAdmission::AlreadyCommitted(result) = check_deployment_commit(tx, request, now, limits)? {
        return Ok(result);
    }
    let (spec, revision, request_hash) = normalized_request(request, limits)?;
    let result = PublishResult {
        operation_id: request.operation_id,
        previous_revision: request.expected_revision,
        revision,
    };
    let receipt = CommitReceipt::V1(CommitReceiptV1 {
        request_hash,
        publisher: request.publisher,
        result: result.clone(),
    });
    let receipt = bsatn::to_vec(&receipt)
        .map_err(|_| DeploymentError::CorruptMetadata)?
        .into_boxed_slice();
    let expires_ms = operation_expiry_ms(request.operation_id, (now.to_micros_since_unix_epoch() / 1000) as u64)?;
    let expires_us = i64::try_from(expires_ms * 1000).map_err(|_| DeploymentError::CorruptMetadata)?;
    let row = StDeploymentRow {
        key: 0,
        revision,
        last_operation_id: request.operation_id.as_u128(),
        payload: spec.encode()?,
    };
    tx.clear_table(ST_DEPLOYMENT_ID)?;
    tx.insert_via_serialize_bsatn(ST_DEPLOYMENT_ID, &row)?;
    tx.insert_via_serialize_bsatn(
        ST_DEPLOYMENT_OPERATION_ID,
        &StDeploymentOperationRow {
            operation_id: request.operation_id.as_u128(),
            previous_revision: request.expected_revision,
            committed_revision: revision,
            commit_result: receipt,
            expires_at: Timestamp::from_micros_since_unix_epoch(expires_us).into(),
        },
    )?;
    Ok(result)
}

/// Install the frozen generation/grant tuple. Changes at the same generation
/// cannot reopen a revoked grant, even if a stale coordinator changes only the
/// target-set hash. Control must allocate a new generation for every barrier.
pub fn install_container_fence(
    db: &RelationalDB,
    tx: &mut MutTx,
    next: &StContainerFenceRow,
) -> Result<(), DeploymentError> {
    if next.generation == 0 {
        return Err(DeploymentError::FenceConflict);
    }
    let key: AlgebraicValue = next.source_identity.into();
    let previous = tx
        .iter_by_col_eq(ST_CONTAINER_FENCE_ID, ColId(0), &key)?
        .next()
        .map(|row| StContainerFenceRow::try_from(row).map(|value| (row.pointer(), value)))
        .transpose()?;
    if let Some((pointer, previous)) = previous {
        if previous == *next {
            return Ok(());
        }
        if next.generation <= previous.generation || next.target_grant_revision < previous.target_grant_revision {
            return Err(DeploymentError::FenceConflict);
        }
        db.hosted_admission()
            .fences_changed()
            .map_err(|_| DeploymentError::FenceRevisionExhausted)?;
        db.delete(tx, ST_CONTAINER_FENCE_ID, [pointer]);
    } else {
        db.hosted_admission()
            .fences_changed()
            .map_err(|_| DeploymentError::FenceRevisionExhausted)?;
    }
    tx.insert_via_serialize_bsatn(ST_CONTAINER_FENCE_ID, next)?;
    Ok(())
}

/// An exact denial of a previously observed allowed fence. Revision counters
/// belong to this live database open, not to the durable generation namespace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FenceDenial {
    pub row: StContainerFenceRow,
    pub revision_before: u64,
    pub revision_after: u64,
}

/// Deny an orphan without lowering or inventing a control generation.
///
/// The trusted coordinator must establish the source's absence from current
/// control inventory, then recheck that inventory revision while holding this
/// same serializable transaction. This function does not confer authority to
/// any external caller. Only the exact allowed tuple, or its already-denied
/// form, is accepted. Ordinary installation cannot reopen this generation.
pub fn deny_container_fence(
    db: &RelationalDB,
    tx: &mut MutTx,
    expected: &StContainerFenceRow,
) -> Result<FenceDenial, DeploymentError> {
    if !expected.allowed || expected.generation == 0 {
        return Err(DeploymentError::FenceConflict);
    }
    let key: AlgebraicValue = expected.source_identity.into();
    let (pointer, current) = tx
        .iter_by_col_eq(ST_CONTAINER_FENCE_ID, ColId(0), &key)?
        .next()
        .map(|row| StContainerFenceRow::try_from(row).map(|value| (row.pointer(), value)))
        .transpose()?
        .ok_or(DeploymentError::FenceConflict)?;
    let denied = StContainerFenceRow {
        allowed: false,
        ..expected.clone()
    };
    let (revision_before, revision_after) = if current == denied {
        let revision = db.hosted_admission().fence_revision();
        (revision, revision)
    } else {
        if current != *expected {
            return Err(DeploymentError::FenceConflict);
        }
        // The gate is closed before the row changes. A rollback still
        // conservatively invalidates any concurrently progressing page scan.
        let revisions = db
            .hosted_admission()
            .fences_changed()
            .map_err(|_| DeploymentError::FenceRevisionExhausted)?;
        db.delete(tx, ST_CONTAINER_FENCE_ID, [pointer]);
        tx.insert_via_serialize_bsatn(ST_CONTAINER_FENCE_ID, &denied)?;
        revisions
    };
    Ok(FenceDenial {
        row: denied,
        revision_before,
        revision_after,
    })
}

/// Called with verified hosted credentials inside every admitted transaction,
/// including each later transaction of a procedure. Signature, audience, expiry,
/// capability, and interface checks are additional receiving-host requirements.
pub fn check_container_fence<S: StateView>(
    state: &S,
    source: Identity,
    generation: u64,
    target_grant_revision: u64,
) -> Result<(), DeploymentError> {
    let key = AlgebraicValue::U256(source.to_u256().into());
    let fence = state
        .iter_by_col_eq(ST_CONTAINER_FENCE_ID, ColId(0), &key)?
        .next()
        .map(StContainerFenceRow::try_from)
        .transpose()?;
    if !fence
        .is_some_and(|f| f.allowed && f.generation == generation && f.target_grant_revision == target_grant_revision)
    {
        return Err(DeploymentError::ContainerFenced);
    }
    Ok(())
}

/// Capture validated hosted connection authority in the transaction inserting st_client.
/// This is host-only metadata; ordinary connections leave no row and retain flags zero.
pub(crate) fn record_connection_auth(
    tx: &mut MutTx,
    connection_id: ConnectionId,
    sender: Identity,
    call_auth_flags: u32,
) -> Result<(), DeploymentError> {
    tx.insert_via_serialize_bsatn(
        ST_CONNECTION_AUTH_ID,
        &StConnectionAuthRow {
            connection_id: connection_id.into(),
            sender_identity: sender.into(),
            call_auth_flags,
        },
    )?;
    Ok(())
}

/// Recover captured authority for a host-dispatched disconnect event. It is not
/// a new container admission and does not require a still-valid credential/lease.
pub(crate) fn connection_auth_flags<S: StateView>(
    state: &S,
    connection_id: ConnectionId,
    sender: Identity,
) -> Result<u32, DeploymentError> {
    let key: AlgebraicValue = ConnectionIdViaU128::from(connection_id).into();
    let row = state
        .iter_by_col_eq(ST_CONNECTION_AUTH_ID, ColId(0), &key)?
        .next()
        .map(StConnectionAuthRow::try_from)
        .transpose()?;
    let Some(row) = row else { return Ok(0) };
    if row.sender_identity.0 != sender {
        return Err(DeploymentError::CorruptMetadata);
    }
    Ok(row.call_auth_flags)
}

#[cfg(test)]
mod tests;
