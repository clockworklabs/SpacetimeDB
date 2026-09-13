use super::*;
use crate::db::deployment::{current_deployment, install_publication_fence};
use crate::db::persistence::LocalPersistenceProvider;
use crate::host::empty_module;
use spacetimedb_datastore::locking_tx_datastore::state_view::StateView;
use spacetimedb_datastore::system_tables::{ST_DEPLOYMENT_OPERATION_ID, ST_PUBLISH_FENCE_ID};
use spacetimedb_lib::container::{
    ContainerMode, ContainerResources, ContainerSpec, ImagePlatform, OciDigest, RestartPolicy,
};
use spacetimedb_lib::deployment::{DeploymentSpec, DeploymentSpecV1, ModuleComponent, UserModule, UserModuleKind};
use spacetimedb_lib::{hash_bytes, Uuid};
use spacetimedb_paths::FromPathUnchecked;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct InitialStorage {
    program: Program,
    request: Option<DeploymentCommit>,
    initial_lookups: AtomicUsize,
}

#[async_trait]
impl ExternalStorage for InitialStorage {
    async fn lookup(&self, hash: Hash) -> anyhow::Result<Option<Box<[u8]>>> {
        Ok((self.program.hash == hash).then(|| self.program.bytes.clone()))
    }
    async fn initial_deployment(&self, _: &Database) -> anyhow::Result<Option<DeploymentCommit>> {
        self.initial_lookups.fetch_add(1, Ordering::SeqCst);
        Ok(self.request.clone())
    }
}

fn request(program: &Program, sequence: u64, initial: bool) -> DeploymentCommit {
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    DeploymentCommit {
        operation_id: Uuid::from_u128((now_ms << 80) | (0x7000u128 << 64) | (0x8000u128 << 48) | u128::from(sequence)),
        publication_epoch: sequence,
        publisher: Identity::ONE,
        expected_revision: None,
        expected_last_operation: None,
        prepared_manifest_hash: hash_bytes(sequence.to_le_bytes()),
        deployment: DeploymentSpec::V1(DeploymentSpecV1 {
            module: if initial {
                ModuleComponent::SystemEmpty(spacetimedb_lib::deployment::system_empty::empty().descriptor)
            } else {
                ModuleComponent::User(UserModule {
                    kind: UserModuleKind::Wasm,
                    program_hash: program.hash,
                })
            },
            container: Some(ContainerSpec {
                image_manifest: OciDigest::sha256([4; 32]),
                image_platform: ImagePlatform {
                    os: "linux".into(),
                    architecture: "arm64".into(),
                },
                argv: vec!["/app/server".into()],
                user: "1000:1000".into(),
                working_directory: "/app".into(),
                mode: ContainerMode::Service,
                restart: RestartPolicy::OnFailure,
                env_keys: vec![],
                resources: ContainerResources {
                    cpu_millicores: 1000,
                    memory_bytes: 64 * 1024 * 1024,
                    scratch_bytes: 64 * 1024 * 1024,
                    pids_max: 64,
                },
                ports: vec![],
                mounts: vec![],
                stop_grace_ms: 1000,
            }),
        }),
    }
}

fn fixture(id: u64, bootstrap: bool) -> (tempfile::TempDir, HostController, Database, Arc<InitialStorage>) {
    let directory = tempfile::tempdir().unwrap();
    let data_dir = Arc::new(ServerDataDir::from_path_unchecked(directory.path().to_owned()));
    let program = empty_module::program(empty_module::VERSION).unwrap();
    let storage = Arc::new(InitialStorage {
        request: bootstrap.then(|| request(&program, 1, true)),
        program: program.clone(),
        initial_lookups: AtomicUsize::new(0),
    });
    let controller = HostController::new(
        data_dir.clone(),
        db::Config {
            storage: db::Storage::Disk,
            page_pool_max_size: None,
        },
        HostRuntimeConfig::default(),
        storage.clone(),
        Arc::new(NullEnergyMonitor),
        Arc::new(()),
        Arc::new(LocalPersistenceProvider::new(data_dir)),
        JobCores::without_pinned_cores(),
    );
    let database = Database {
        id,
        database_identity: Identity::from_u256(id.into()),
        owner_identity: Identity::ONE,
        host_type: HostType::Wasm,
        initial_program: program.hash,
        bootstrap_generation: 0,
    };
    (directory, controller, database, storage)
}

#[tokio::test(flavor = "multi_thread")]
async fn bootstrap_commits_fence_program_and_deployment_and_reopens_from_disk() {
    let (_directory, controller, database, storage) = fixture(0xdd01, true);
    let module = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    let expected = storage.request.as_ref().unwrap().deployment.revision().unwrap();
    module.relational_db().with_read_only(Workload::Internal, |tx| {
        assert_eq!(current_deployment(tx).unwrap().unwrap().0, expected);
        assert_eq!(tx.table_row_count(ST_PUBLISH_FENCE_ID), Some(1));
        assert_eq!(tx.table_row_count(ST_DEPLOYMENT_OPERATION_ID), Some(1));
    });
    assert_eq!(
        module.relational_db().program().unwrap().unwrap().hash,
        database.initial_program
    );
    drop(module);
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    let module = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    assert_eq!(
        storage.initial_lookups.load(Ordering::SeqCst),
        1,
        "replay must not re-run bootstrap intent"
    );
    module.relational_db().with_read_only(Workload::Internal, |tx| {
        assert_eq!(current_deployment(tx).unwrap().unwrap().0, expected);
        assert_eq!(tx.table_row_count(ST_DEPLOYMENT_OPERATION_ID), Some(1));
    });
    drop(module);
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn activation_failure_after_commit_closes_old_host_and_recovers_committed_program() {
    let (_directory, controller, database, _storage) = fixture(0xdd02, false);
    let old_module = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    let watcher = controller.watch_module_host(database.id).await.unwrap();
    let mut bytes = spacetimedb_lib::deployment::system_empty::empty().bytes.to_vec();
    bytes.extend_from_slice(&[0, 3, 1, b'x', 1]);
    let newer = Program::from_bytes(ModuleKind::WASM, bytes);
    let publication = request(&newer, 1, false);
    old_module
        .relational_db()
        .with_auto_commit(Workload::Internal, |tx| {
            install_publication_fence(tx, publication.publication_epoch, publication.operation_id)
        })
        .unwrap();
    FAIL_NEXT_DEPLOYMENT_ACTIVATION
        .lock()
        .insert(database.database_identity);
    let error = controller
        .update_module_host_with_deployment(
            database.clone(),
            HostType::Wasm,
            database.id,
            newer.bytes.clone(),
            MigrationPolicy::Compatible,
            Some(publication.clone()),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("injected scheduler activation failure"));
    assert!(
        controller.get_module_host(database.id).await.is_err(),
        "old executable must not remain available"
    );
    assert!(watcher.has_changed().is_err(), "old client watcher must close");
    assert_eq!(old_module.relational_db().program().unwrap().unwrap().hash, newer.hash);
    old_module.relational_db().with_read_only(Workload::Internal, |tx| {
        assert_eq!(
            current_deployment(tx).unwrap().unwrap().0,
            publication.deployment.revision().unwrap()
        );
    });
    drop(watcher);
    drop(old_module);
    let recovered = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    assert_eq!(recovered.info.module_hash, newer.hash);
    recovered.relational_db().with_read_only(Workload::Internal, |tx| {
        assert_eq!(
            current_deployment(tx).unwrap().unwrap().0,
            publication.deployment.revision().unwrap()
        );
        assert_eq!(tx.table_row_count(ST_DEPLOYMENT_OPERATION_ID), Some(1));
    });
    drop(recovered);
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
}

struct DeclaredInitialEnvironment {
    database: Database,
    values: std::collections::BTreeMap<String, String>,
    loads: AtomicUsize,
}

#[async_trait]
impl InitialEnvironmentSource for DeclaredInitialEnvironment {
    async fn load(&self, database: &Database) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
        anyhow::ensure!(
            database.id == self.database.id
                && database.database_identity == self.database.database_identity
                && database.owner_identity == self.database.owner_identity
                && database.initial_program == self.database.initial_program
                && database.bootstrap_generation == self.database.bootstrap_generation,
            "bootstrap configuration does not match the exact persisted database generation"
        );
        self.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.values.clone())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn declared_builtin_merges_environment_atomically_and_reopens() {
    use spacetimedb_lib::environment::{
        EnvironmentConstraint, EnvironmentDeclaration, EnvironmentSchema, EnvironmentUpdate,
    };
    use std::collections::BTreeMap;

    let schema = EnvironmentSchema::new(vec![
        EnvironmentDeclaration {
            name: "REQUIRED".into(),
            constraint: EnvironmentConstraint::AnyString,
            optional: false,
        },
        EnvironmentDeclaration {
            name: "MODE".into(),
            constraint: EnvironmentConstraint::OneOf(vec!["ready".into(), "paused".into()]),
            optional: false,
        },
        EnvironmentDeclaration {
            name: "FIXED".into(),
            constraint: EnvironmentConstraint::Literal("constant".into()),
            optional: false,
        },
        EnvironmentDeclaration {
            name: "OPTIONAL".into(),
            constraint: EnvironmentConstraint::AnyString,
            optional: true,
        },
    ])
    .unwrap();
    let initial_values = BTreeMap::from([
        ("REQUIRED".into(), "initial".into()),
        ("MODE".into(), "ready".into()),
        ("FIXED".into(), "constant".into()),
        ("OPTIONAL".into(), "remove-on-next-publish".into()),
    ]);
    let program = empty_module::declared_program(&schema).unwrap();
    let database = Database {
        id: 0xdd03,
        database_identity: Identity::from_u256(0xdd03_u64.into()),
        owner_identity: Identity::ONE,
        host_type: HostType::Wasm,
        initial_program: program.hash,
        bootstrap_generation: 7,
    };
    let descriptor = spacetimedb_lib::deployment::SystemEmptyModule {
        version: empty_module::VERSION,
        program_hash: program.hash,
    };
    let mut initial = request(&program, 1, true);
    let DeploymentSpec::V1(spec) = &mut initial.deployment;
    spec.module = ModuleComponent::SystemEmpty(descriptor);
    spec.container.as_mut().unwrap().env_keys = vec!["FIXED".into(), "MODE".into(), "REQUIRED".into()];
    let revision = initial.deployment.revision().unwrap();
    let storage = Arc::new(InitialStorage {
        program: program.clone(),
        request: Some(initial.clone()),
        initial_lookups: AtomicUsize::new(0),
    });
    let environment = Arc::new(DeclaredInitialEnvironment {
        database: database.clone(),
        values: initial_values.clone(),
        loads: AtomicUsize::new(0),
    });
    let directory = tempfile::tempdir().unwrap();
    let data = Arc::new(ServerDataDir::from_path_unchecked(directory.path().to_owned()));
    let controller = HostController::new(
        data.clone(),
        db::Config {
            storage: db::Storage::Disk,
            page_pool_max_size: None,
        },
        HostRuntimeConfig::default(),
        storage.clone(),
        Arc::new(NullEnergyMonitor),
        Arc::new(()),
        Arc::new(LocalPersistenceProvider::new(data)),
        JobCores::without_pinned_cores(),
    )
    .with_initial_environment_source(environment.clone());

    let result = std::panic::AssertUnwindSafe(async {
        let launched = controller
            .get_or_launch_module_host_with_bootstrap(database.clone(), database.id)
            .await
            .unwrap();
        if let Some(completion) = launched.bootstrap_completion {
            assert_eq!(completion.bootstrap_generation(), database.bootstrap_generation);
            completion.wait().await.unwrap();
        }
        let module = launched.module;
        assert_eq!(module.info.module_def.environment(), &schema);
        assert!(module.info.module_def.tables().next().is_none());
        let assert_state = |module: &ModuleHost, expected: &BTreeMap<String, String>, receipts, epoch| {
            assert!(empty_module::matches_program(
                &descriptor,
                &module.relational_db().program().unwrap().unwrap()
            ));
            module.relational_db().with_read_only(Workload::Internal, |tx| {
                assert_eq!(db::environment::snapshot(tx).unwrap(), *expected);
                assert_eq!(current_deployment(tx).unwrap().unwrap().0, revision);
                let cursor = db::deployment::current_publication(tx).unwrap().unwrap();
                assert_eq!(cursor.publication_epoch, epoch);
                assert_eq!(tx.table_row_count(ST_DEPLOYMENT_OPERATION_ID), Some(receipts));
            });
        };
        assert_state(&module, &initial_values, 1, 1);
        let replacement = BTreeMap::from([
            ("REQUIRED".into(), "replaced".into()),
            ("MODE".into(), "ready".into()),
            ("FIXED".into(), "constant".into()),
            ("UNDECLARED".into(), "stored".into()),
        ]);
        let publish_request = |sequence, previous_operation| {
            let mut publication = request(&program, sequence, true);
            publication.deployment = initial.deployment.clone();
            publication.expected_revision = Some(revision);
            publication.expected_last_operation = Some(previous_operation);
            publication
        };
        let publication = publish_request(2, initial.operation_id);
        let accepted_operation = publication.operation_id;
        let accepted_publication = publication.clone();
        module
            .relational_db()
            .with_auto_commit(Workload::Internal, |tx| {
                install_publication_fence(tx, publication.publication_epoch, publication.operation_id)
            })
            .unwrap();
        let updated = controller
            .update_module_host_with_environment_options_and_deployment(
                database.clone(),
                HostType::Wasm,
                database.id,
                program.bytes.clone(),
                MigrationPolicy::Compatible,
                EnvironmentUpdate {
                    values: BTreeMap::from([
                        ("REQUIRED".into(), "replaced".into()),
                        ("UNDECLARED".into(), "stored".into()),
                    ]),
                    remove: vec!["OPTIONAL".into()],
                    replace: false,
                },
                None,
                Some(publication),
            )
            .await
            .unwrap();
        let UpdateDatabaseResult::UpdatePerformed {
            tx_offset,
            durable_offset,
        } = updated
        else {
            panic!("unchanged-code ENV publication must commit a replacement: {updated:?}");
        };
        let offset = tx_offset.await.unwrap();
        durable_offset.unwrap().wait_for(offset).await.unwrap();
        let module = controller.get_module_host(database.id).await.unwrap();
        assert_state(&module, &replacement, 2, 2);

        // The same normalized deployment revision does not authorize replacing
        // ENV prepared against the earlier committed operation.
        let stale = publish_request(3, initial.operation_id);
        module
            .relational_db()
            .with_auto_commit(Workload::Internal, |tx| {
                install_publication_fence(tx, stale.publication_epoch, stale.operation_id)
            })
            .unwrap();
        assert!(controller
            .update_module_host_with_environment_and_deployment(
                database.clone(),
                HostType::Wasm,
                database.id,
                program.bytes.clone(),
                MigrationPolicy::Compatible,
                initial_values.clone(),
                Some(stale),
            )
            .await
            .is_err());
        assert_state(
            &controller.get_module_host(database.id).await.unwrap(),
            &replacement,
            2,
            2,
        );

        let mut invalid_sets = Vec::new();
        let mut invalid = replacement.clone();
        invalid.remove("REQUIRED");
        invalid_sets.push(invalid);
        for (key, value) in [("MODE", "unknown"), ("FIXED", "changed"), ("INVALID-NAME", "denied")] {
            let mut invalid = replacement.clone();
            invalid.insert(key.into(), value.into());
            invalid_sets.push(invalid);
        }
        for (index, invalid) in invalid_sets.into_iter().enumerate() {
            let publication = publish_request(4 + index as u64, accepted_operation);
            module
                .relational_db()
                .with_auto_commit(Workload::Internal, |tx| {
                    install_publication_fence(tx, publication.publication_epoch, publication.operation_id)
                })
                .unwrap();
            assert!(controller
                .update_module_host_with_environment_options_and_deployment(
                    database.clone(),
                    HostType::Wasm,
                    database.id,
                    program.bytes.clone(),
                    MigrationPolicy::Compatible,
                    EnvironmentUpdate {
                        values: invalid,
                        replace: true,
                        ..Default::default()
                    },
                    None,
                    Some(publication),
                )
                .await
                .is_err());
            let current = controller.get_module_host(database.id).await.unwrap();
            assert_state(&current, &replacement, 2, 2);
        }
        // The exact builtin bytes remain part of admission even though a valid
        // Wasm custom section would leave the extracted declarations unchanged.
        let mut different_bytes = program.bytes.to_vec();
        different_bytes.extend_from_slice(&[0, 3, 1, b'x', 1]);
        let publication = publish_request(8, accepted_operation);
        module
            .relational_db()
            .with_auto_commit(Workload::Internal, |tx| {
                install_publication_fence(tx, publication.publication_epoch, publication.operation_id)
            })
            .unwrap();
        assert!(controller
            .update_module_host_with_environment_and_deployment(
                database.clone(),
                HostType::Wasm,
                database.id,
                different_bytes.into(),
                MigrationPolicy::Compatible,
                replacement.clone(),
                Some(publication),
            )
            .await
            .is_err());
        assert_state(
            &controller.get_module_host(database.id).await.unwrap(),
            &replacement,
            2,
            2,
        );
        let next = publish_request(9, accepted_operation);
        module
            .relational_db()
            .with_auto_commit(Workload::Internal, |tx| {
                install_publication_fence(tx, next.publication_epoch, next.operation_id)
            })
            .unwrap();
        let later = controller
            .update_module_host_with_environment_options_and_deployment(
                database.clone(),
                HostType::Wasm,
                database.id,
                program.bytes.clone(),
                MigrationPolicy::Compatible,
                EnvironmentUpdate::from(BTreeMap::from([("UNDECLARED".into(), "later".into())])),
                None,
                Some(next),
            )
            .await
            .unwrap();
        assert!(later.was_successful());
        let mut replacement = replacement;
        replacement.insert("UNDECLARED".into(), "later".into());
        // Old exact retries must return the original receipt without reapplying
        // old values over a newer committed environment.
        let retry = controller
            .update_module_host_with_environment_options_and_deployment(
                database.clone(),
                HostType::Wasm,
                database.id,
                program.bytes.clone(),
                MigrationPolicy::Compatible,
                EnvironmentUpdate::from(BTreeMap::from([("UNDECLARED".into(), "stored".into())])),
                None,
                Some(accepted_publication),
            )
            .await
            .unwrap();
        assert!(matches!(retry, UpdateDatabaseResult::DeploymentAlreadyCommitted { .. }));
        assert_state(
            &controller.get_module_host(database.id).await.unwrap(),
            &replacement,
            3,
            9,
        );
        drop(module);
        controller.exit_module_host_and_join(database.id).await.unwrap();
        let reopened = controller
            .get_or_launch_module_host(database.clone(), database.id)
            .await
            .unwrap();
        assert_state(&reopened, &replacement, 3, 9);
        assert_eq!(storage.initial_lookups.load(Ordering::SeqCst), 1);
        assert_eq!(environment.loads.load(Ordering::SeqCst), 1);
    })
    .catch_unwind()
    .await;
    // Always join the existing close owner before releasing its filesystem.
    let cleanup = controller.exit_module_host_and_join(database.id).await;
    if let Err(panic) = result {
        if let Err(error) = cleanup {
            log::error!("declared-builtin fixture cleanup failed after assertion: {error:#}");
        }
        std::panic::resume_unwind(panic);
    }
    cleanup.unwrap();
}
