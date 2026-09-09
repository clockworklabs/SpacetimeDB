use super::*;
use crate::db::deployment::{
    install_container_fence, install_publication_fence, record_deployment_commit, DeploymentCommit,
};
use crate::db::relational_db::tests_utils::TestDB;
use crate::host::container_environment as host;
use spacetimedb_datastore::execution_context::Workload;
use spacetimedb_lib::container::*;
use spacetimedb_lib::deployment::{DeploymentSpec, DeploymentSpecV1, ModuleComponent};
use spacetimedb_lib::environment::{EnvironmentConstraint, EnvironmentDeclaration, EnvironmentSchema};
use spacetimedb_lib::{hash_bytes, Identity, Timestamp};
use std::sync::{Arc, Barrier};

fn uuid() -> Uuid {
    Uuid::from_u128(uuid::Uuid::now_v7().as_u128())
}

fn environment_schema(keys: &[String]) -> EnvironmentSchema {
    EnvironmentSchema::new(
        keys.iter()
            .map(|name| EnvironmentDeclaration {
                name: name.clone(),
                constraint: EnvironmentConstraint::AnyString,
                optional: true,
            })
            .collect(),
    )
    .unwrap()
}

fn setup(db: &RelationalDB, keys: Vec<String>) -> EnvironmentSnapshotScope {
    let spec = ContainerSpec {
        image_manifest: OciDigest::sha256([7; 32]),
        image_platform: ImagePlatform {
            os: "linux".into(),
            architecture: "arm64".into(),
        },
        argv: vec!["/app/agent".into()],
        user: "1000:1000".into(),
        working_directory: "/app".into(),
        mode: ContainerMode::Job,
        restart: RestartPolicy::Never,
        env_keys: keys,
        resources: ContainerResources {
            cpu_millicores: 1000,
            memory_bytes: 64 * 1024 * 1024,
            scratch_bytes: 64 * 1024 * 1024,
            pids_max: 64,
        },
        ports: vec![],
        mounts: vec![],
        stop_grace_ms: DEFAULT_STOP_GRACE_MS,
    }
    .normalize(&Default::default())
    .unwrap();
    let schema = environment_schema(&spec.env_keys);
    let generated = spacetimedb_lib::deployment::system_empty::generate(&schema).unwrap();
    let request = DeploymentCommit {
        operation_id: uuid(),
        publication_epoch: 1,
        publisher: db.owner_identity(),
        expected_revision: None,
        prepared_manifest_hash: hash_bytes(b"prepared"),
        deployment: DeploymentSpec::V1(DeploymentSpecV1 {
            module: ModuleComponent::SystemEmpty(generated.descriptor),
            container: Some(spec.clone()),
        }),
    };
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
        install_publication_fence(tx, request.publication_epoch, request.operation_id)?;
        record_deployment_commit(tx, &request, Timestamp::now(), &Default::default())?;
        install_container_fence(db, tx, &self_fence(db, 1, true))?;
        environment::replace(
            db,
            tx,
            &schema,
            &spec.env_keys.iter().map(|key| (key.clone(), "before".into())).collect(),
        )?;
        Ok(())
    })
    .unwrap();
    EnvironmentSnapshotScope {
        cluster: "local-test".into(),
        database_id: 1,
        database_identity: db.database_identity(),
        node_id: 2,
        node_incarnation: uuid(),
        generation: 1,
        deployment_revision: request.deployment.revision().unwrap(),
        start_request: request.operation_id,
        env_generation: uuid(),
        env_keys: spec.env_keys,
    }
}

fn self_fence(db: &RelationalDB, generation: u64, allowed: bool) -> StContainerFenceRow {
    StContainerFenceRow {
        source_identity: db.database_identity().into(),
        generation,
        target_grant_revision: 1,
        target_set_hash: hash_bytes(b"targets"),
        allowed,
    }
}

fn tx<T>(
    db: &RelationalDB,
    action: impl FnOnce(&mut MutTx) -> Result<T, EnvironmentSnapshotError>,
) -> Result<T, EnvironmentSnapshotError> {
    db.with_auto_commit(Workload::ForTests, action)
}

#[test]
fn container_environment_capture_retry_read_and_durable_reopen() {
    let db = TestDB::durable_without_snapshot_repo().unwrap();
    let scope = setup(&db, vec!["A".into(), "B".into()]);
    let captured = db
        .runtime()
        .unwrap()
        .block_on(host::capture(db.db.clone(), scope.clone()))
        .unwrap();
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
        environment::replace(
            &db,
            tx,
            &environment_schema(&scope.env_keys),
            &BTreeMap::from([("A".into(), "changed".into())]),
        )?;
        Ok(())
    })
    .unwrap();
    let db = db.reopen().unwrap();
    let retried = db
        .runtime()
        .unwrap()
        .block_on(host::capture(db.db.clone(), scope))
        .unwrap();
    assert_eq!(retried.receipt, captured.receipt);
    assert!(retried.durable_through >= captured.durable_through);
    let values = db
        .runtime()
        .unwrap()
        .block_on(host::read(db.db.clone(), retried.receipt))
        .unwrap()
        .receipt;
    assert_eq!(
        values.selected_values,
        BTreeMap::from([("A".into(), "before".into()), ("B".into(), "before".into())])
    );
    assert!(!format!("{values:?}").contains("before"));
}

#[test]
fn container_environment_capture_is_atomic_against_concurrent_environment_mutation() {
    let db = TestDB::in_memory().unwrap();
    let scope = setup(&db, vec!["A".into(), "B".into()]);
    let barrier = Arc::new(Barrier::new(2));
    let writer = {
        let db = db.db.clone();
        let barrier = barrier.clone();
        let schema = environment_schema(&scope.env_keys);
        std::thread::spawn(move || {
            barrier.wait();
            db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
                environment::replace(
                    &db,
                    tx,
                    &schema,
                    &BTreeMap::from([("A".into(), "after".into()), ("B".into(), "after".into())]),
                )?;
                Ok(())
            })
            .unwrap();
        })
    };
    barrier.wait();
    let receipt = tx(&db, |tx| capture(&db, tx, &scope)).unwrap();
    writer.join().unwrap();
    let values = db
        .with_read_only(Workload::ForTests, |state| read(&db, state, &receipt))
        .unwrap();
    assert_eq!(values.selected_values["A"], values.selected_values["B"]);
}

#[test]
fn container_environment_closure_fences_delayed_capture_and_new_instance_observes_new_values() {
    let db = TestDB::durable_without_snapshot_repo().unwrap();
    let old = setup(&db, vec!["A".into()]);
    let captured = db
        .runtime()
        .unwrap()
        .block_on(host::capture(db.db.clone(), old.clone()))
        .unwrap()
        .receipt;
    assert_eq!(
        tx(&db, |tx| close(&db, tx, &old, 1)),
        Err(EnvironmentSnapshotError::Fenced)
    );
    assert_eq!(
        tx(&db, |tx| close(&db, tx, &old, 2)),
        Err(EnvironmentSnapshotError::Fenced)
    );
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
        install_container_fence(&db, tx, &self_fence(&db, 2, true))?;
        environment::replace(
            &db,
            tx,
            &environment_schema(&old.env_keys),
            &BTreeMap::from([("A".into(), "new-boot".into())]),
        )?;
        Ok(())
    })
    .unwrap();
    db.runtime()
        .unwrap()
        .block_on(host::close(db.db.clone(), old.clone(), 2))
        .unwrap();
    let db = db.reopen().unwrap();
    assert_eq!(
        tx(&db, |tx| capture(&db, tx, &old)),
        Err(EnvironmentSnapshotError::Fenced)
    );
    assert!(matches!(
        db.with_read_only(Workload::ForTests, |state| read(&db, state, &captured)),
        Err(EnvironmentSnapshotError::Fenced)
    ));
    db.runtime()
        .unwrap()
        .block_on(host::close(db.db.clone(), old.clone(), 2))
        .unwrap();
    let new = EnvironmentSnapshotScope {
        generation: 2,
        env_generation: uuid(),
        ..old
    };
    let receipt = db
        .runtime()
        .unwrap()
        .block_on(host::capture(db.db.clone(), new))
        .unwrap()
        .receipt;
    assert_ne!(receipt.capture_receipt, captured.capture_receipt);
    assert_eq!(
        db.runtime()
            .unwrap()
            .block_on(host::read(db.db.clone(), receipt))
            .unwrap()
            .receipt
            .selected_values["A"],
        "new-boot"
    );
}

#[test]
fn container_environment_full_scope_conflicts_and_missing_ready_record_fail_closed() {
    let db = TestDB::in_memory().unwrap();
    let scope = setup(&db, vec!["A".into()]);
    let receipt = tx(&db, |tx| capture(&db, tx, &scope)).unwrap();
    let mut variants = Vec::new();
    let mut changed = scope.clone();
    changed.cluster = "other".into();
    variants.push(changed);
    let mut changed = scope.clone();
    changed.database_id += 1;
    variants.push(changed);
    let mut changed = scope.clone();
    changed.node_id += 1;
    variants.push(changed);
    let mut changed = scope.clone();
    changed.node_incarnation = uuid();
    variants.push(changed);
    let mut changed = scope.clone();
    changed.start_request = uuid();
    variants.push(changed);
    let mut changed = scope.clone();
    changed.env_generation = uuid();
    variants.push(changed);
    let mut changed = scope.clone();
    changed.env_keys.clear();
    variants.push(changed);
    for changed in variants {
        assert_eq!(
            tx(&db, |tx| capture(&db, tx, &changed)),
            Err(EnvironmentSnapshotError::ScopeConflict)
        );
    }
    let mut changed = scope.clone();
    changed.deployment_revision = hash_bytes(b"other");
    assert_eq!(
        tx(&db, |tx| capture(&db, tx, &changed)),
        Err(EnvironmentSnapshotError::RevisionConflict)
    );
    let mut fork = scope;
    fork.database_identity = Identity::from_u256(99u64.into());
    assert_eq!(
        tx(&db, |tx| capture(&db, tx, &fork)),
        Err(EnvironmentSnapshotError::InvalidScope)
    );
    // Simulate a restored/lost Ready record. Resolve may not reinterpret its UUID.
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
        tx.clear_table(ST_CONTAINER_ENVIRONMENT_ID)?;
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        db.with_read_only(Workload::ForTests, |state| read(&db, state, &receipt)),
        Err(EnvironmentSnapshotError::NotCaptured)
    ));
}

#[test]
fn container_environment_missing_empty_invalid_and_capacity_are_atomic_and_redacted() {
    let db = TestDB::in_memory().unwrap();
    let scope = setup(&db, vec!["A".into(), "B".into()]);
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
        environment::replace(
            &db,
            tx,
            &environment_schema(&scope.env_keys),
            &BTreeMap::from([("A".into(), "before".into())]),
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        tx(&db, |tx| capture(&db, tx, &scope)),
        Err(EnvironmentSnapshotError::MissingKeys(vec!["B".into()]))
    );
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
        environment::replace(
            &db,
            tx,
            &environment_schema(&scope.env_keys),
            &BTreeMap::from([("A".into(), "before".into()), ("B".into(), "secret\0value".into())]),
        )?;
        Ok(())
    })
    .unwrap();
    let failure = tx(&db, |tx| capture(&db, tx, &scope)).unwrap_err();
    assert_eq!(failure, EnvironmentSnapshotError::InvalidEnvironment);
    assert!(!format!("{failure:?}: {failure}").contains("secret"));
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
        environment::replace(
            &db,
            tx,
            &environment_schema(&scope.env_keys),
            &BTreeMap::from([("A".into(), "before".into()), ("B".into(), "".into())]),
        )?;
        Ok(())
    })
    .unwrap();
    for generation in 1..=MAX_RETAINED_SNAPSHOTS {
        db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
            install_container_fence(&db, tx, &self_fence(&db, generation, true))?;
            Ok(())
        })
        .unwrap();
        let request = EnvironmentSnapshotScope {
            generation,
            env_generation: uuid(),
            ..scope.clone()
        };
        let receipt = tx(&db, |tx| capture(&db, tx, &request)).unwrap();
        assert_eq!(
            db.with_read_only(Workload::ForTests, |state| read(&db, state, &receipt))
                .unwrap()
                .selected_values["B"],
            ""
        );
    }
    let generation = MAX_RETAINED_SNAPSHOTS + 1;
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
        install_container_fence(&db, tx, &self_fence(&db, generation, true))?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        tx(&db, |tx| capture(
            &db,
            tx,
            &EnvironmentSnapshotScope { generation, ..scope }
        )),
        Err(EnvironmentSnapshotError::Capacity)
    );
}

#[test]
fn container_environment_history_is_hidden_from_privileged_sql_and_subscriptions() {
    use crate::db::sql::ast::SchemaViewer;
    use spacetimedb_expr::check::SchemaView;
    use spacetimedb_lib::identity::AuthCtx;
    let db = TestDB::in_memory().unwrap();
    let scope = setup(&db, vec!["A".into()]);
    tx(&db, |tx| capture(&db, tx, &scope)).unwrap();
    let auth = AuthCtx::for_current(db.owner_identity());
    db.with_read_only(Workload::ForTests, |state| {
        let schema = SchemaViewer::new(state, &auth);
        assert!(schema.schema_for_table(ST_CONTAINER_ENVIRONMENT_ID).is_none());
        assert!(schema.table_id("st_container_environment").is_none());
        for sql in [
            "SELECT * FROM st_container_environment",
            "SELECT h.* FROM st_container_environment h JOIN st_env e ON h.generation = 1",
            "DELETE FROM st_container_environment",
            "UPDATE st_container_environment SET generation = 2",
        ] {
            assert!(
                spacetimedb_query::compile_sql_stmt(sql, &schema, &auth).is_err(),
                "{sql}"
            );
        }
        assert!(spacetimedb_query::compile_sql_stmt("SELECT * FROM st_env", &schema, &auth).is_ok());
        assert!(crate::subscription::query::compile_read_only_query(
            &auth,
            state,
            "SELECT * FROM st_container_environment"
        )
        .is_err());
        assert!(crate::subscription::subscription::get_all(
            |db, tx| db.get_all_tables(tx).map(Vec::into_iter),
            &db,
            state,
            &auth
        )
        .unwrap()
        .is_empty());
    });
}

#[test]
fn container_environment_host_api_requires_durability() {
    let db = TestDB::in_memory().unwrap();
    let scope = setup(&db, vec![]);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(matches!(
        rt.block_on(host::capture(db.db.clone(), scope)),
        Err(EnvironmentSnapshotError::DurabilityUnavailable)
    ));
}

#[test]
fn container_environment_concurrent_closure_cannot_reopen_collected_generation() {
    let db = TestDB::in_memory().unwrap();
    let scope = setup(&db, vec!["A".into()]);
    let barrier = Arc::new(Barrier::new(2));
    let capture_thread = {
        let db = db.db.clone();
        let scope = scope.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            tx(&db, |tx| capture(&db, tx, &scope))
        })
    };
    barrier.wait();
    db.with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<_> {
        install_container_fence(&db, tx, &self_fence(&db, 2, false))?;
        close(&db, tx, &scope, 2)?;
        Ok(())
    })
    .unwrap();
    assert!(matches!(
        capture_thread.join().unwrap(),
        Ok(_) | Err(EnvironmentSnapshotError::Fenced)
    ));
    assert_eq!(
        tx(&db, |tx| capture(&db, tx, &scope)),
        Err(EnvironmentSnapshotError::Fenced)
    );
    db.with_read_only(Workload::ForTests, |state| {
        assert_eq!(state.table_row_count(ST_CONTAINER_ENVIRONMENT_ID), Some(0))
    });
}
