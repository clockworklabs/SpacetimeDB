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
        prepared_manifest_hash: hash_bytes(sequence.to_le_bytes()),
        deployment: DeploymentSpec::V1(DeploymentSpecV1 {
            module: if initial {
                ModuleComponent::SystemEmpty(1)
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
    let program = empty_module::program(1).unwrap();
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
    let mut bytes = empty_module::VERSION_1_BYTES.to_vec();
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
