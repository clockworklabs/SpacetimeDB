//! Bounded host-only inspection and durable denial of retained source fences.
//!
//! The authenticated platform coordinator owns control membership checks and
//! actor drainage. These APIs do not authorize an external client or expose the
//! protected fence table through SQL, subscriptions, or module syscalls.

use crate::db::deployment::{deny_container_fence, DeploymentError, FenceDenial};
use crate::db::relational_db::RelationalDB;
use crate::host::container_environment::Durable;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::locking_tx_datastore::state_view::ScanOrIndex;
use spacetimedb_datastore::system_tables::{StContainerFenceRow, ST_CONTAINER_FENCE_ID};
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_lib::Identity;
use spacetimedb_primitives::ColId;
use spacetimedb_sats::AlgebraicValue;
use std::ops::Bound;
use std::sync::{Arc, LazyLock};
use tokio::sync::Semaphore;

pub const FENCE_PAGE_SIZE: usize = 64;
static OPERATIONS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(8)));

#[derive(Debug, thiserror::Error)]
pub enum FenceOperationError {
    #[error("receiving host fence operation capacity exhausted")]
    Capacity,
    #[error("receiving host fence storage is unavailable")]
    Storage,
    #[error("receiving host fence durability is unavailable")]
    DurabilityUnavailable,
    #[error("receiving host fence durability failed")]
    DurabilityFailed,
    #[error("receiving host fence index is unavailable")]
    IndexUnavailable,
    #[error("receiving host fence inventory changed")]
    RevisionChanged,
    #[error(transparent)]
    Deployment(#[from] DeploymentError),
}

#[derive(Debug)]
pub struct FencePage {
    /// Includes denied rows so every bounded physical page advances its cursor.
    pub rows: Vec<StContainerFenceRow>,
    /// Last source in this page, or the input cursor if the page is empty.
    pub next_source: Option<Identity>,
    pub complete: bool,
    /// Read under the same database transaction as the rows. Restart the scan
    /// if this differs from the previous page or a known local denial result.
    pub fence_revision: u64,
}

/// Read at most 64 rows using the protected source Identity B-tree. We refuse
/// the datastore's scan fallback: row order and physical work must be bounded.
pub async fn page(db: Arc<RelationalDB>, after: Option<Identity>) -> Result<FencePage, FenceOperationError> {
    let permit = OPERATIONS
        .clone()
        .try_acquire_owned()
        .map_err(|_| FenceOperationError::Capacity)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let tx = db.begin_tx(Workload::Internal);
        let result = (|| {
            let lower = after.map_or(Bound::Unbounded, |identity| {
                Bound::Excluded(AlgebraicValue::U256(identity.to_u256().into()))
            });
            let range = db
                .iter_by_col_range(&tx, ST_CONTAINER_FENCE_ID, ColId(0), (lower, Bound::Unbounded))
                .map_err(|_| FenceOperationError::Storage)?;
            let ScanOrIndex::Index(mut range) = range else {
                return Err(FenceOperationError::IndexUnavailable);
            };
            let rows = range
                .by_ref()
                .take(FENCE_PAGE_SIZE)
                .map(|row| StContainerFenceRow::try_from(row).map_err(|_| FenceOperationError::Storage))
                .collect::<Result<Vec<_>, _>>()?;
            let complete = range.next().is_none();
            let next_source = rows.last().map(|row| row.source_identity.into()).or(after);
            Ok(FencePage {
                rows,
                next_source,
                complete,
                fence_revision: db.hosted_admission().fence_revision(),
            })
        })();
        let (_, metrics, reducer) = db.release_tx(tx);
        db.report_read_tx_metrics(reducer, metrics);
        result
    })
    .await
    .map_err(|_| FenceOperationError::Storage)?
}

/// The coordinator must have excluded this exact source from current control
/// authority. For a check that must serialize with an inventory lock, use the
/// synchronous `deployment::deny_container_fence` inside the caller's own
/// serializable transaction instead. Neither API performs that control check.
pub async fn deny_orphan(
    db: Arc<RelationalDB>,
    expected: StContainerFenceRow,
) -> Result<Durable<FenceDenial>, FenceOperationError> {
    deny_with_capacity(db, expected, OPERATIONS.clone()).await
}

/// Confirm durability of all fences visible at one exact physical revision.
/// In particular, a retry that sees a previous owner's committed denial must
/// still wait for that denial's storage acknowledgment. The coordinator must
/// recheck the revision under its final database transaction before admission;
/// this receipt does not freeze future mutations or confer control authority.
pub async fn confirm_revision(
    db: Arc<RelationalDB>,
    expected_physical: u64,
) -> Result<Durable<()>, FenceOperationError> {
    confirm_with_capacity(db, expected_physical, OPERATIONS.clone()).await
}

async fn confirm_with_capacity(
    db: Arc<RelationalDB>,
    expected_physical: u64,
    capacity: Arc<Semaphore>,
) -> Result<Durable<()>, FenceOperationError> {
    let mut durability = db
        .durable_tx_offset()
        .ok_or(FenceOperationError::DurabilityUnavailable)?;
    let permit = capacity
        .try_acquire_owned()
        .map_err(|_| FenceOperationError::Capacity)?;
    tokio::spawn(async move {
        let _permit = permit;
        let action_db = db.clone();
        let durable_through = tokio::task::spawn_blocking(move || {
            let tx = action_db.begin_tx(Workload::Internal);
            let physical = action_db.hosted_admission().fence_revision();
            let (offset, metrics, reducer) = action_db.release_tx(tx);
            action_db.report_read_tx_metrics(reducer, metrics);
            if physical != expected_physical || physical == u64::MAX {
                return Err(FenceOperationError::RevisionChanged);
            }
            Ok(offset)
        })
        .await
        .map_err(|_| FenceOperationError::Storage)??;
        durability
            .wait_for(durable_through)
            .await
            .map_err(|_| FenceOperationError::DurabilityFailed)?;
        drop(db);
        Ok(Durable {
            receipt: (),
            durable_through,
        })
    })
    .await
    .map_err(|_| FenceOperationError::Storage)?
}

async fn deny_with_capacity(
    db: Arc<RelationalDB>,
    expected: StContainerFenceRow,
    capacity: Arc<Semaphore>,
) -> Result<Durable<FenceDenial>, FenceOperationError> {
    let mut durability = db
        .durable_tx_offset()
        .ok_or(FenceOperationError::DurabilityUnavailable)?;
    let permit = capacity
        .try_acquire_owned()
        .map_err(|_| FenceOperationError::Capacity)?;
    // A caller's cancellation drops only this JoinHandle. The owned task keeps
    // both the permit and database alive through physical commit and durability.
    tokio::spawn(async move {
        let _permit = permit;
        let action_db = db.clone();
        let (durable_through, receipt) = tokio::task::spawn_blocking(move || {
            let tx = action_db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
            let (tx, receipt) =
                action_db.with_auto_rollback(tx, |tx| deny_container_fence(&action_db, tx, &expected))?;
            let (offset, data, metrics, reducer) = action_db
                .commit_tx(tx)
                .map_err(|_| FenceOperationError::Storage)?
                .ok_or(FenceOperationError::Storage)?;
            action_db.report_mut_tx_metrics(reducer, metrics, Some(data));
            Ok::<_, FenceOperationError>((offset, receipt))
        })
        .await
        .map_err(|_| FenceOperationError::Storage)??;
        durability
            .wait_for(durable_through)
            .await
            .map_err(|_| FenceOperationError::DurabilityFailed)?;
        // Do not drop the physical database before the owned durability wait.
        drop(db);
        Ok(Durable {
            receipt,
            durable_through,
        })
    })
    .await
    .map_err(|_| FenceOperationError::Storage)?
}

#[cfg(test)]
mod tests;
