use super::*;
use crate::db::relational_db::tests_utils::{begin_mut_tx, TestDB};
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_datastore::system_tables::{
    StEnvRow, ST_CLIENT_ID, ST_CONNECTION_AUTH_ID, ST_CONNECTION_CREDENTIALS_ID, ST_ENV_ID,
};
use spacetimedb_durability::Durability;
use spacetimedb_lib::deployment::{
    DeploymentSpecV1, ModuleComponent, UserModule, UserModuleKind, PUBLISH_RETRY_WINDOW_MS,
};

fn request(sequence: u64, previous: Option<Hash>) -> DeploymentCommit {
    DeploymentCommit {
        operation_id: Uuid::from_u128(0x01991ec4000070008000000000000000 | u128::from(sequence)),
        publication_epoch: sequence,
        publisher: Identity::from_u256(55u64.into()),
        expected_revision: previous,
        prepared_manifest_hash: hash_bytes(sequence.to_le_bytes()),
        deployment: DeploymentSpec::V1(DeploymentSpecV1 {
            module: ModuleComponent::User(UserModule {
                kind: UserModuleKind::Wasm,
                program_hash: hash_bytes(sequence.to_le_bytes()),
            }),
            container: None,
        }),
    }
}

fn now() -> Timestamp {
    Timestamp::from_micros_since_unix_epoch(((request(1, None).operation_id.as_u128() >> 80) as i64) * 1000)
}

fn transact<T>(
    db: &RelationalDB,
    f: impl FnOnce(&mut MutTx) -> Result<T, DeploymentError>,
) -> Result<T, DeploymentError> {
    db.with_auto_commit(Workload::ForTests, f)
}

#[test]
fn deployment_retry_returns_original_result_after_later_publish_without_mutation() {
    let db = TestDB::in_memory().unwrap();
    let first = request(1, None);
    let limits = ContainerSpecLimits::default();
    let accepted = transact(&db, |tx| {
        install_publication_fence(tx, first.publication_epoch, first.operation_id)?;
        record_deployment_commit(tx, &first, now(), &limits)
    })
    .unwrap();
    let second = request(2, Some(accepted.revision));
    let later = transact(&db, |tx| {
        install_publication_fence(tx, second.publication_epoch, second.operation_id)?;
        record_deployment_commit(tx, &second, now(), &limits)
    })
    .unwrap();
    transact(&db, |tx| {
        assert!(matches!(check_deployment_commit(tx, &first, now(), &limits)?, CommitAdmission::AlreadyCommitted(ref r) if r == &accepted));
        assert_eq!(record_deployment_commit(tx, &first, now(), &limits)?, accepted);
        assert_eq!(current_deployment(tx)?.unwrap().0, later.revision);
        assert_eq!(tx.table_row_count(ST_DEPLOYMENT_OPERATION_ID), Some(2));
        Ok(())
    }).unwrap();
}

#[test]
fn deployment_fence_and_revision_conflicts_fail_before_module_execution() {
    let db = TestDB::in_memory().unwrap();
    let first = request(1, None);
    let mut second = request(2, Some(hash_bytes(b"not current")));
    let limits = ContainerSpecLimits::default();
    transact(&db, |tx| {
        install_publication_fence(tx, second.publication_epoch, second.operation_id)
    })
    .unwrap();
    transact(&db, |tx| {
        assert!(matches!(
            check_deployment_commit(tx, &first, now(), &limits),
            Err(DeploymentError::PublicationFenced)
        ));
        assert!(matches!(
            check_deployment_commit(tx, &second, now(), &limits),
            Err(DeploymentError::RevisionConflict)
        ));
        assert!(matches!(
            install_publication_fence(tx, first.publication_epoch, first.operation_id),
            Err(DeploymentError::PublicationFenced)
        ));
        second.expected_revision = None;
        assert!(matches!(
            check_deployment_commit(tx, &second, now(), &limits)?,
            CommitAdmission::Ready
        ));
        Ok(())
    })
    .unwrap();
}

#[test]
fn deployment_and_module_effects_roll_back_together() {
    let db = TestDB::in_memory().unwrap();
    let request = request(1, None);
    let limits = ContainerSpecLimits::default();
    transact(&db, |tx| {
        install_publication_fence(tx, request.publication_epoch, request.operation_id)
    })
    .unwrap();
    let failed: Result<(), DeploymentError> = transact(&db, |tx| {
        assert!(matches!(
            check_deployment_commit(tx, &request, now(), &limits)?,
            CommitAdmission::Ready
        ));
        // A second persistent table stands in for migration effects in the same
        // transaction. Integration must additionally execute Wasm/JS migrations.
        tx.insert_via_serialize_bsatn(
            ST_ENV_ID,
            &StEnvRow {
                key: "MIGRATED".into(),
                value: "yes".into(),
            },
        )?;
        record_deployment_commit(tx, &request, now(), &limits)?;
        Err(DeploymentError::Database(DBError::Other(anyhow::anyhow!(
            "injected failure before commit"
        ))))
    });
    assert!(failed.is_err());
    transact(&db, |tx| {
        assert!(current_deployment(tx)?.is_none());
        assert_eq!(tx.table_row_count(ST_ENV_ID), Some(0));
        assert_eq!(tx.table_row_count(ST_DEPLOYMENT_OPERATION_ID), Some(0));
        assert!(matches!(
            check_deployment_commit(tx, &request, now(), &limits)?,
            CommitAdmission::Ready
        ));
        Ok(())
    })
    .unwrap();
}

#[test]
fn deployment_operation_cannot_be_reused_by_another_publisher_or_changed_request() {
    let db = TestDB::in_memory().unwrap();
    let original = request(1, None);
    let limits = ContainerSpecLimits::default();
    transact(&db, |tx| {
        install_publication_fence(tx, original.publication_epoch, original.operation_id)?;
        record_deployment_commit(tx, &original, now(), &limits)
    })
    .unwrap();
    transact(&db, |tx| {
        let mut changed = original.clone();
        changed.publisher = Identity::from_u256(99u64.into());
        assert!(matches!(
            check_deployment_commit(tx, &changed, now(), &limits),
            Err(DeploymentError::OperationConflict)
        ));
        changed = original.clone();
        changed.deployment = DeploymentSpec::V1(DeploymentSpecV1 {
            module: ModuleComponent::SystemEmpty(1),
            container: None,
        });
        assert!(matches!(
            check_deployment_commit(tx, &changed, now(), &limits),
            Err(DeploymentError::OperationConflict)
        ));
        // A retained row does not extend the advertised retry window.
        let expired = Timestamp::from_micros_since_unix_epoch(
            now().to_micros_since_unix_epoch() + (PUBLISH_RETRY_WINDOW_MS as i64) * 1000,
        );
        assert!(matches!(
            check_deployment_commit(tx, &original, expired, &limits),
            Err(DeploymentError::Validation(DeploymentValidationError::ExpiredOperation))
        ));
        Ok(())
    })
    .unwrap();
}

#[test]
fn container_fence_revokes_copied_credentials_and_does_not_reopen_at_same_generation() {
    let db = TestDB::in_memory().unwrap();
    let source = Identity::from_u256(777u64.into());
    let first = StContainerFenceRow {
        source_identity: source.into(),
        generation: 1,
        target_grant_revision: 3,
        target_set_hash: hash_bytes(b"targets1"),
        allowed: true,
    };
    transact(&db, |tx| {
        assert!(matches!(
            check_container_fence(tx, source, 1, 3),
            Err(DeploymentError::ContainerFenced)
        ));
        install_container_fence(&db, tx, &first)?;
        install_container_fence(&db, tx, &first)?;
        check_container_fence(tx, source, 1, 3)?;
        assert!(matches!(
            check_container_fence(tx, source, 1, 4),
            Err(DeploymentError::ContainerFenced)
        ));
        Ok(())
    })
    .unwrap();
    let revoked = StContainerFenceRow {
        generation: 2,
        target_grant_revision: 4,
        target_set_hash: hash_bytes(b"targets2"),
        allowed: false,
        ..first.clone()
    };
    transact(&db, |tx| {
        install_container_fence(&db, tx, &revoked)?;
        assert!(matches!(
            check_container_fence(tx, source, 1, 3),
            Err(DeploymentError::ContainerFenced)
        ));
        assert!(matches!(
            check_container_fence(tx, source, 2, 4),
            Err(DeploymentError::ContainerFenced)
        ));
        let reopen = StContainerFenceRow {
            allowed: true,
            ..revoked.clone()
        };
        assert!(matches!(
            install_container_fence(&db, tx, &reopen),
            Err(DeploymentError::FenceConflict)
        ));
        assert!(matches!(
            install_container_fence(&db, tx, &first),
            Err(DeploymentError::FenceConflict)
        ));
        Ok(())
    })
    .unwrap();
}

#[test]
fn captured_connection_authority_survives_replay_without_inferring_sender_authority() {
    let db = TestDB::durable().unwrap();
    let self_sender = db.database_identity();
    let foreign_sender = Identity::ONE;
    let self_connection = ConnectionId::from_u128(41);
    let foreign_connection = ConnectionId::from_u128(42);
    let ordinary_connection = ConnectionId::from_u128(43);
    transact(&db, |tx| {
        for (connection, sender, flags) in [
            (self_connection, self_sender, 1),
            (foreign_connection, foreign_sender, 0),
        ] {
            tx.insert_st_client(
                sender,
                connection,
                r#"{"iss":"platform","sub":"previously-admitted","exp":1}"#,
            )?;
            record_connection_auth(tx, connection, sender, flags)?;
        }
        tx.insert_st_client(self_sender, ordinary_connection, "ordinary JWT")?;
        Ok(())
    })
    .unwrap();
    // TestDB::reopen expects zero connected clients. Reopen the same committed
    // log explicitly to exercise crash recovery with outstanding connections.
    let (db, durability, runtime, directory) = db.into_parts();
    let runtime = runtime.unwrap();
    let directory = directory.unwrap();
    let durability = durability.unwrap();
    runtime.block_on(db.shutdown());
    drop(db);
    runtime.block_on(durability.close());
    drop(durability);
    let _runtime_guard = runtime.enter();
    let (db, durability) = TestDB::open_existing_durable(
        &directory,
        runtime.handle().clone(),
        0,
        TestDB::DATABASE_IDENTITY,
        TestDB::OWNER,
        true,
    )
    .unwrap();
    transact(&db, |tx| {
        assert_eq!(connection_auth_flags(tx, self_connection, self_sender)?, 1);
        assert_eq!(connection_auth_flags(tx, foreign_connection, foreign_sender)?, 0);
        // Equal sender/database identities do not invent internal authority.
        assert_eq!(connection_auth_flags(tx, ordinary_connection, self_sender)?, 0);
        assert_eq!(tx.table_row_count(ST_CONNECTION_AUTH_ID), Some(2));
        assert!(matches!(
            connection_auth_flags(tx, self_connection, foreign_sender),
            Err(DeploymentError::CorruptMetadata)
        ));
        Ok(())
    })
    .unwrap();
    db.clear_all_clients().unwrap();
    transact(&db, |tx| {
        for table in [ST_CLIENT_ID, ST_CONNECTION_CREDENTIALS_ID, ST_CONNECTION_AUTH_ID] {
            assert_eq!(tx.table_row_count(table), Some(0));
        }
        Ok(())
    })
    .unwrap();
    runtime.block_on(db.shutdown());
    drop(db);
    runtime.block_on(durability.close());
}

#[test]
fn connection_auth_and_client_rows_share_connect_and_cleanup_transactions() {
    let db = TestDB::in_memory().unwrap();
    let sender = db.database_identity();
    let connection = ConnectionId::from_u128(77);
    let rejected: Result<(), DeploymentError> = transact(&db, |tx| {
        tx.insert_st_client(sender, connection, "JWT")?;
        record_connection_auth(tx, connection, sender, 1)?;
        Err(DeploymentError::CorruptMetadata)
    });
    assert!(rejected.is_err());
    transact(&db, |tx| {
        assert!(tx.st_client_row(sender, connection).is_none());
        assert_eq!(connection_auth_flags(tx, connection, sender)?, 0);
        tx.insert_st_client(sender, connection, "JWT")?;
        record_connection_auth(tx, connection, sender, 1)?;
        Ok(())
    })
    .unwrap();
    // A failed callback transaction cannot partially delete its captured auth.
    let failed_callback: Result<(), DeploymentError> = transact(&db, |tx| {
        tx.delete_st_client(sender, connection, db.database_identity())?;
        Err(DeploymentError::CorruptMetadata)
    });
    assert!(failed_callback.is_err());
    transact(&db, |tx| {
        assert!(tx.st_client_row(sender, connection).is_some());
        assert_eq!(connection_auth_flags(tx, connection, sender)?, 1);
        // Both successful callbacks and fallback cleanup use this deletion path.
        tx.delete_st_client(sender, connection, db.database_identity())?;
        Ok(())
    })
    .unwrap();
    transact(&db, |tx| {
        for table in [ST_CLIENT_ID, ST_CONNECTION_CREDENTIALS_ID, ST_CONNECTION_AUTH_ID] {
            assert_eq!(tx.table_row_count(table), Some(0));
        }
        Ok(())
    })
    .unwrap();
}

#[test]
fn container_fence_installation_serializes_with_admitted_transactions() {
    let db = TestDB::in_memory().unwrap();
    let source = Identity::from_u256(778u64.into());
    let first = StContainerFenceRow {
        source_identity: source.into(),
        generation: 1,
        target_grant_revision: 0,
        target_set_hash: hash_bytes(b"self"),
        allowed: true,
    };
    transact(&db, |tx| install_container_fence(&db, tx, &first)).unwrap();
    let admitted = begin_mut_tx(&db);
    check_container_fence(&admitted, source, 1, 0).unwrap();
    // A fence cannot commit midway through an already admitted transaction.
    assert!(db
        .try_begin_mut_tx(
            spacetimedb_datastore::traits::IsolationLevel::Serializable,
            Workload::ForTests
        )
        .is_none());
    let _ = db.rollback_mut_tx(admitted);
    transact(&db, |tx| {
        install_container_fence(&db, tx, &StContainerFenceRow { generation: 2, ..first })
    })
    .unwrap();
    transact(&db, |tx| {
        assert!(matches!(
            check_container_fence(tx, source, 1, 0),
            Err(DeploymentError::ContainerFenced)
        ));
        check_container_fence(tx, source, 2, 0)
    })
    .unwrap();
}
