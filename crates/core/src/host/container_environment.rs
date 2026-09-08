//! Bounded, durable host operations for immutable container environments.
//!
//! The caller must confirm current control authority and authoritative leader
//! before calling. These local host APIs are not an external authentication
//! interface. Historical restore must keep admission closed until current
//! operational fences have been reconciled. No in-memory production fallback.

use crate::db::container_environment::{
    self as storage, EnvironmentClosedReceipt, EnvironmentSnapshotError, SecretEnvironment,
};
use crate::db::relational_db::{MutTx, RelationalDB};
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::traits::IsolationLevel;
use spacetimedb_lib::container_environment::{EnvironmentSnapshotReceipt, EnvironmentSnapshotScope};
use std::sync::{Arc, LazyLock};
use tokio::sync::Semaphore;

/// Bounds queued/blocked transactions even when the async caller is cancelled.
static OPERATIONS: LazyLock<Arc<Semaphore>> = LazyLock::new(|| Arc::new(Semaphore::new(8)));

/// The barrier is specific to this proof. It can advance on an exact retry.
#[derive(Debug)]
pub struct Durable<T> {
    pub receipt: T,
    pub durable_through: u64,
}

pub type DurableSnapshotReceipt = Durable<EnvironmentSnapshotReceipt>;
pub type DurableEnvironmentValues = Durable<SecretEnvironment>;
pub type DurableClosedReceipt = Durable<EnvironmentClosedReceipt>;

pub async fn capture(
    db: Arc<RelationalDB>,
    scope: EnvironmentSnapshotScope,
) -> Result<DurableSnapshotReceipt, EnvironmentSnapshotError> {
    let action_db = db.clone();
    mutate(db, move |tx| storage::capture(&action_db, tx, &scope)).await
}

pub async fn read(
    db: Arc<RelationalDB>,
    receipt: EnvironmentSnapshotReceipt,
) -> Result<DurableEnvironmentValues, EnvironmentSnapshotError> {
    read_with_capacity(db, receipt, OPERATIONS.clone()).await
}

async fn read_with_capacity(
    db: Arc<RelationalDB>,
    receipt: EnvironmentSnapshotReceipt,
    capacity: Arc<Semaphore>,
) -> Result<DurableEnvironmentValues, EnvironmentSnapshotError> {
    let mut durability = db
        .durable_tx_offset()
        .ok_or(EnvironmentSnapshotError::DurabilityUnavailable)?;
    let permit = capacity
        .try_acquire_owned()
        .map_err(|_| EnvironmentSnapshotError::Capacity)?;
    tokio::spawn(async move {
        let _permit = permit;
        let action_db = db.clone();
        let (durable_through, receipt) = tokio::task::spawn_blocking(move || {
            let tx = action_db.begin_tx(Workload::Internal);
            let result = storage::read(&action_db, &tx, &receipt);
            let (offset, metrics, reducer) = action_db.release_tx(tx);
            action_db.report_read_tx_metrics(reducer, metrics);
            result.map(|result| (offset, result))
        })
        .await
        .map_err(|_| EnvironmentSnapshotError::Storage)??;
        durability
            .wait_for(durable_through)
            .await
            .map_err(|_| EnvironmentSnapshotError::DurabilityFailed)?;
        drop(db);
        Ok(Durable {
            receipt,
            durable_through,
        })
    })
    .await
    .map_err(|_| EnvironmentSnapshotError::Storage)?
}

pub async fn close(
    db: Arc<RelationalDB>,
    scope: EnvironmentSnapshotScope,
    closed_through_generation: u64,
) -> Result<DurableClosedReceipt, EnvironmentSnapshotError> {
    let action_db = db.clone();
    mutate(db, move |tx| {
        storage::close(&action_db, tx, &scope, closed_through_generation)
    })
    .await
}

async fn mutate<T: Send + 'static>(
    db: Arc<RelationalDB>,
    action: impl FnOnce(&mut MutTx) -> Result<T, EnvironmentSnapshotError> + Send + 'static,
) -> Result<Durable<T>, EnvironmentSnapshotError> {
    mutate_with_capacity(db, OPERATIONS.clone(), action).await
}

async fn mutate_with_capacity<T: Send + 'static>(
    db: Arc<RelationalDB>,
    capacity: Arc<Semaphore>,
    action: impl FnOnce(&mut MutTx) -> Result<T, EnvironmentSnapshotError> + Send + 'static,
) -> Result<Durable<T>, EnvironmentSnapshotError> {
    let mut durability = db
        .durable_tx_offset()
        .ok_or(EnvironmentSnapshotError::DurabilityUnavailable)?;
    let permit = capacity
        .try_acquire_owned()
        .map_err(|_| EnvironmentSnapshotError::Capacity)?;
    // A cancelled waiter drops only this JoinHandle. The actual operation
    // keeps its database and finite slot until commit and durability finish.
    tokio::spawn(async move {
        let _permit = permit;
        let action_db = db.clone();
        let (durable_through, receipt) = tokio::task::spawn_blocking(move || {
            let tx = action_db.begin_mut_tx(IsolationLevel::Serializable, Workload::Internal);
            let (tx, result) = action_db.with_auto_rollback(tx, action)?;
            let (offset, data, metrics, reducer) = action_db
                .commit_tx(tx)
                .map_err(|_| EnvironmentSnapshotError::Storage)?
                .ok_or(EnvironmentSnapshotError::Storage)?;
            action_db.report_mut_tx_metrics(reducer, metrics, Some(data));
            Ok::<_, EnvironmentSnapshotError>((offset, result))
        })
        .await
        .map_err(|_| EnvironmentSnapshotError::Storage)??;
        durability
            .wait_for(durable_through)
            .await
            .map_err(|_| EnvironmentSnapshotError::DurabilityFailed)?;
        drop(db);
        Ok(Durable {
            receipt,
            durable_through,
        })
    })
    .await
    .map_err(|_| EnvironmentSnapshotError::Storage)?
}

#[cfg(test)]
#[path = "container_environment/durability_tests.rs"]
mod durability_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::relational_db::tests_utils::TestDB;
    use std::time::Duration;

    #[test]
    fn container_environment_cancelled_transaction_retains_capacity_until_worker_finishes() {
        let db = TestDB::durable_without_snapshot_repo().unwrap();
        let capacity = Arc::new(Semaphore::new(1));
        let (entered, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release, release_rx) = std::sync::mpsc::sync_channel(1);
        let blocked = db
            .runtime()
            .unwrap()
            .spawn(mutate_with_capacity(db.db.clone(), capacity.clone(), move |_| {
                entered.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                Err::<(), _>(EnvironmentSnapshotError::InvalidEnvironment)
            }));
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        blocked.abort();
        db.runtime().unwrap().block_on(async {
            assert!(blocked.await.unwrap_err().is_cancelled());
            let retry = mutate_with_capacity(db.db.clone(), capacity.clone(), |_| Ok(())).await;
            assert!(matches!(retry, Err(EnvironmentSnapshotError::Capacity)));
        });
        release.send(()).unwrap();
        db.runtime().unwrap().block_on(async {
            // This waits for the actual cancelled caller's blocking worker to
            // return, not merely for cancellation of its async JoinHandle.
            let permit = tokio::time::timeout(Duration::from_secs(5), capacity.clone().acquire_owned())
                .await
                .unwrap()
                .unwrap();
            drop(permit);
            mutate_with_capacity(db.db.clone(), capacity, |_| Ok(())).await.unwrap();
        });
    }
}
