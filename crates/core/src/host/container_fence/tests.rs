use super::*;
use crate::db::deployment::{check_container_fence, install_container_fence};
use crate::db::relational_db::tests_utils::TestDB;
use spacetimedb_lib::hash_bytes;
use std::time::Duration;

fn fence(source: u64) -> StContainerFenceRow {
    StContainerFenceRow {
        source_identity: Identity::from_u256(source.into()).into(),
        generation: 7,
        target_grant_revision: 3,
        target_set_hash: hash_bytes(b"retained target set"),
        allowed: true,
    }
}

fn install(db: &RelationalDB, row: &StContainerFenceRow) {
    db.with_auto_commit(Workload::ForTests, |tx| install_container_fence(db, tx, row))
        .unwrap();
}

#[tokio::test]
async fn container_fence_pages_follow_identity_index_and_include_denied_rows() {
    let db = TestDB::in_memory().unwrap();
    // Reverse insertion order deliberately differs from source index order.
    for source in (0..130).rev() {
        install(
            &db,
            &StContainerFenceRow {
                allowed: source % 3 == 0,
                ..fence(source)
            },
        );
    }
    let revision = db.hosted_admission().fence_revision();
    let mut after = None;
    let mut identities = Vec::new();
    for expected_size in [64, 64, 2] {
        let page = page(db.db.clone(), after).await.unwrap();
        assert_eq!(page.rows.len(), expected_size);
        assert_eq!(page.complete, expected_size == 2);
        assert_eq!(page.fence_revision, revision);
        identities.extend(page.rows.iter().map(|row| Identity::from(row.source_identity)));
        after = page.next_source;
        assert_eq!(after, identities.last().copied());
    }
    assert_eq!(
        identities,
        (0..130u64).map(|n| Identity::from_u256(n.into())).collect::<Vec<_>>()
    );
    let empty = page(db.db.clone(), after).await.unwrap();
    assert!(empty.rows.is_empty() && empty.complete);
    assert_eq!(empty.next_source, after);
}

#[tokio::test]
async fn container_fence_missing_index_refuses_unbounded_scan_and_memory_refuses_denial() {
    let db = TestDB::in_memory().unwrap();
    let expected = fence(17);
    install(&db, &expected);
    assert!(matches!(
        deny_orphan(db.db.clone(), expected).await,
        Err(FenceOperationError::DurabilityUnavailable)
    ));
    db.with_auto_commit(Workload::ForTests, |tx| {
        db.drop_index(tx, spacetimedb_primitives::IndexId(34))
    })
    .unwrap();
    assert!(matches!(
        page(db.db.clone(), None).await,
        Err(FenceOperationError::IndexUnavailable)
    ));
}

#[test]
fn container_fence_orphan_denial_is_durable_exact_and_cannot_reopen_generation() {
    let db = TestDB::durable_without_snapshot_repo().unwrap();
    let expected = fence(19);
    install(&db, &expected);
    db.hosted_admission().begin().unwrap().complete().unwrap();
    let before = db.hosted_admission().fence_revision();
    let first = db
        .runtime()
        .unwrap()
        .block_on(deny_orphan(db.db.clone(), expected.clone()))
        .unwrap();
    assert_eq!(first.receipt.revision_before, before);
    assert_eq!(first.receipt.revision_after, before + 1);
    assert!(!db.hosted_admission().is_open());
    assert_eq!(
        first.receipt.row,
        StContainerFenceRow {
            allowed: false,
            ..expected.clone()
        }
    );
    assert!(db.durable_tx_offset().unwrap().get().unwrap().unwrap() >= first.durable_through);
    let retry = db
        .runtime()
        .unwrap()
        .block_on(deny_orphan(db.db.clone(), expected.clone()))
        .unwrap();
    assert_eq!(retry.receipt.revision_before, before + 1);
    assert_eq!(retry.receipt.revision_after, before + 1);
    let db = db.reopen().unwrap();
    db.with_auto_commit(Workload::ForTests, |tx| {
        assert!(matches!(
            check_container_fence(
                tx,
                expected.source_identity.into(),
                expected.generation,
                expected.target_grant_revision
            ),
            Err(DeploymentError::ContainerFenced)
        ));
        assert!(matches!(
            install_container_fence(&db, tx, &expected),
            Err(DeploymentError::FenceConflict)
        ));
        let retry = deny_container_fence(&db, tx, &expected)?;
        assert_eq!(retry.row, first.receipt.row);
        assert_eq!(retry.revision_before, retry.revision_after);
        Ok::<_, DeploymentError>(())
    })
    .unwrap();
    install(
        &db,
        &StContainerFenceRow {
            generation: expected.generation + 1,
            ..expected
        },
    );
}

#[test]
fn container_fence_orphan_cas_rejects_missing_changed_and_invalid_observations() {
    let db = TestDB::in_memory().unwrap();
    let expected = fence(20);
    db.with_auto_commit(Workload::ForTests, |tx| {
        assert!(matches!(
            deny_container_fence(&db, tx, &expected),
            Err(DeploymentError::FenceConflict)
        ));
        Ok::<_, DeploymentError>(())
    })
    .unwrap();
    install(&db, &expected);
    let next = StContainerFenceRow {
        generation: expected.generation + 1,
        ..expected.clone()
    };
    install(&db, &next);
    let revision = db.hosted_admission().fence_revision();
    db.with_auto_commit(Workload::ForTests, |tx| {
        for stale in [
            expected,
            StContainerFenceRow {
                allowed: false,
                ..next.clone()
            },
            StContainerFenceRow {
                target_set_hash: hash_bytes(b"different"),
                ..next.clone()
            },
            StContainerFenceRow {
                target_grant_revision: 4,
                ..next.clone()
            },
        ] {
            assert!(matches!(
                deny_container_fence(&db, tx, &stale),
                Err(DeploymentError::FenceConflict)
            ));
        }
        check_container_fence(
            tx,
            next.source_identity.into(),
            next.generation,
            next.target_grant_revision,
        )?;
        Ok::<_, DeploymentError>(())
    })
    .unwrap();
    assert_eq!(db.hosted_admission().fence_revision(), revision);
}

#[tokio::test]
async fn container_fence_physical_change_behind_cursor_invalidates_completion() {
    let db = TestDB::in_memory().unwrap();
    install(&db, &fence(100));
    let ticket = db.hosted_admission().begin().unwrap();
    let first = page(db.db.clone(), None).await.unwrap();
    assert!(first.complete);
    install(&db, &fence(1));
    assert!(ticket.complete_with_fence_revision(first.fence_revision).is_err());
    assert!(!db.hosted_admission().is_open());
    let restarted = page(db.db.clone(), None).await.unwrap();
    assert_eq!(restarted.rows.len(), 2);
    assert_ne!(first.fence_revision, restarted.fence_revision);
}

#[test]
fn container_fence_orphan_rollback_keeps_row_and_invalidates_physical_scan() {
    let db = TestDB::in_memory().unwrap();
    let expected = fence(21);
    install(&db, &expected);
    let before = db.hosted_admission().fence_revision();
    let ticket = db.hosted_admission().begin().unwrap();
    let result = db.with_auto_commit(Workload::ForTests, |tx| {
        deny_container_fence(&db, tx, &expected)?;
        Err::<(), _>(DeploymentError::CorruptMetadata)
    });
    assert!(result.is_err());
    assert!(ticket.complete_with_fence_revision(before).is_err());
    db.with_auto_commit(Workload::ForTests, |tx| {
        check_container_fence(
            tx,
            expected.source_identity.into(),
            expected.generation,
            expected.target_grant_revision,
        )
    })
    .unwrap();
}

/// Real local storage with its durable acknowledgment deliberately withheld.
/// This models a lagging quorum without replacing the transaction or writer.
struct DelayedAcknowledgment {
    writer: Arc<dyn spacetimedb_durability::Durability<TxData = crate::db::relational_db::Txdata>>,
    acknowledged: spacetimedb_durability::DurableOffset,
}

impl spacetimedb_durability::Durability for DelayedAcknowledgment {
    type TxData = crate::db::relational_db::Txdata;
    fn append_tx(&self, tx: spacetimedb_durability::PreparedTx<Self::TxData>) {
        self.writer.append_tx(tx);
    }
    fn durable_tx_offset(&self) -> spacetimedb_durability::DurableOffset {
        self.acknowledged.clone()
    }
    fn close(&self) -> spacetimedb_durability::Close {
        self.writer.close()
    }
}

async fn delayed_storage() -> (
    crate::db::relational_db::tests_utils::TempReplicaDir,
    Arc<RelationalDB>,
    crate::db::relational_db::LocalDurability,
    tokio::sync::watch::Sender<Option<u64>>,
) {
    use crate::db::persistence::Persistence;
    use crate::db::relational_db::{local_durability, tests_utils::TempReplicaDir};
    let directory = TempReplicaDir::new().unwrap();
    let (writer, disk_size) = local_durability((*directory).clone(), None).await.unwrap();
    let (acknowledge, acknowledged) = tokio::sync::watch::channel(None);
    let db = Arc::new(
        TestDB::open_db(
            writer.as_history(),
            Some(Persistence {
                durability: Arc::new(DelayedAcknowledgment {
                    writer: writer.clone(),
                    acknowledged: acknowledged.into(),
                }),
                disk_size,
                snapshots: None,
                runtime: tokio::runtime::Handle::current(),
            }),
            None,
            0,
        )
        .unwrap(),
    );
    (directory, db, writer, acknowledge)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn container_fence_cancelled_denial_retains_capacity_through_actual_durability_wait() {
    use spacetimedb_durability::Durability;
    let (_directory, db, writer, acknowledge) = delayed_storage().await;
    let expected = fence(22);
    install(&db, &expected);
    let capacity = Arc::new(Semaphore::new(1));
    let caller = tokio::spawn(deny_with_capacity(db.clone(), expected.clone(), capacity.clone()));
    let row = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let page = page(db.clone(), None).await.unwrap();
            if let Some(row) = page.rows.first().filter(|row| !row.allowed) {
                break row.clone();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!row.allowed);
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert_eq!(capacity.available_permits(), 0);
    assert!(matches!(
        deny_with_capacity(db.clone(), expected.clone(), capacity.clone()).await,
        Err(FenceOperationError::Capacity)
    ));
    let tx = db.begin_tx(Workload::ForTests);
    let (offset, metrics, reducer) = db.release_tx(tx);
    db.report_read_tx_metrics(reducer, metrics);
    let mut actual = writer.durable_tx_offset();
    let durable = tokio::time::timeout(Duration::from_secs(5), actual.wait_for(offset))
        .await
        .unwrap()
        .unwrap();
    acknowledge.send_replace(Some(durable));
    let permit = tokio::time::timeout(Duration::from_secs(5), capacity.clone().acquire_owned())
        .await
        .unwrap()
        .unwrap();
    drop(permit);
    // The detached owner finished only once its physical acknowledgment arrived.
    db.shutdown().await;
    drop(db);
    writer.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn container_fence_retry_barrier_waits_for_prior_commit_and_retains_cancelled_capacity() {
    use spacetimedb_durability::Durability;
    let (_directory, db, writer, acknowledge) = delayed_storage().await;
    let expected = fence(23);
    install(&db, &expected);
    // A previous coordinator has committed but never confirmed durability.
    let denied = db
        .with_auto_commit(Workload::ForTests, |tx| deny_container_fence(&db, tx, &expected))
        .unwrap();
    let capacity = Arc::new(Semaphore::new(1));
    // Changed inventory fails immediately, even while acknowledgments lag.
    assert!(matches!(
        tokio::time::timeout(
            Duration::from_secs(5),
            confirm_with_capacity(db.clone(), denied.revision_before, capacity.clone())
        )
        .await
        .unwrap(),
        Err(FenceOperationError::RevisionChanged)
    ));
    let caller = tokio::spawn(confirm_with_capacity(
        db.clone(),
        denied.revision_after,
        capacity.clone(),
    ));
    tokio::time::timeout(Duration::from_secs(5), async {
        while capacity.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!caller.is_finished());
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert!(matches!(
        confirm_with_capacity(db.clone(), denied.revision_after, capacity.clone()).await,
        Err(FenceOperationError::Capacity)
    ));
    let tx = db.begin_tx(Workload::ForTests);
    let (offset, metrics, reducer) = db.release_tx(tx);
    db.report_read_tx_metrics(reducer, metrics);
    let mut actual = writer.durable_tx_offset();
    let durable = tokio::time::timeout(Duration::from_secs(5), actual.wait_for(offset))
        .await
        .unwrap()
        .unwrap();
    acknowledge.send_replace(Some(durable));
    let permit = tokio::time::timeout(Duration::from_secs(5), capacity.clone().acquire_owned())
        .await
        .unwrap()
        .unwrap();
    drop(permit);
    let proof = confirm_revision(db.clone(), denied.revision_after).await.unwrap();
    assert_eq!(proof.durable_through, offset);
    // The proof is only a barrier, so finalization still checks current revision.
    install(&db, &fence(24));
    assert!(matches!(
        confirm_revision(db.clone(), denied.revision_after).await,
        Err(FenceOperationError::RevisionChanged)
    ));
    db.shutdown().await;
    drop(db);
    writer.close().await;
}
