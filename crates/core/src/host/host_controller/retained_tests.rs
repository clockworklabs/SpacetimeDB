//! Real local durability writers and module hosts. No network or external
//! configuration. The probe counts writers until their first close completes.
use super::*;
use crate::db::persistence::{LocalPersistenceProvider, Persistence};
use crate::host::empty_module;
use futures::FutureExt;
use spacetimedb_durability::{Close, DurableOffset, PreparedTx};
use spacetimedb_paths::FromPathUnchecked;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::Semaphore;

struct Probe {
    opened: AtomicUsize,
    active: AtomicUsize,
    maximum: AtomicUsize,
    block_close: AtomicBool,
    panic_close: AtomicBool,
    close_started: Semaphore,
    release_close: Semaphore,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            opened: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
            block_close: AtomicBool::new(false),
            panic_close: AtomicBool::new(false),
            close_started: Semaphore::new(0),
            release_close: Semaphore::new(0),
        }
    }
}

struct ProbedProvider {
    local: LocalPersistenceProvider,
    probe: Arc<Probe>,
}

#[async_trait]
impl PersistenceProvider for ProbedProvider {
    async fn persistence(&self, database: &Database, replica: u64) -> anyhow::Result<Persistence> {
        let mut persistence = self.local.persistence(database, replica).await?;
        let active = self.probe.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.probe.maximum.fetch_max(active, Ordering::SeqCst);
        self.probe.opened.fetch_add(1, Ordering::SeqCst);
        persistence.durability = Arc::new(ProbedWriter {
            inner: persistence.durability,
            probe: self.probe.clone(),
            closing: AtomicBool::new(false),
        });
        Ok(persistence)
    }
}

struct ProbedWriter {
    inner: Arc<dyn Durability<TxData = Txdata>>,
    probe: Arc<Probe>,
    closing: AtomicBool,
}

impl Durability for ProbedWriter {
    type TxData = Txdata;
    fn append_tx(&self, tx: PreparedTx<Txdata>) {
        self.inner.append_tx(tx);
    }
    fn durable_tx_offset(&self) -> DurableOffset {
        self.inner.durable_tx_offset()
    }
    fn close(&self) -> Close {
        // Reproduce the actual first-caller-owns-join behavior deliberately.
        // A second close must not let another database open before this one.
        if self.closing.swap(true, Ordering::SeqCst) {
            let offset = self.inner.durable_tx_offset().last_seen();
            return async move { offset }.boxed();
        }
        let close = self.inner.close();
        let probe = self.probe.clone();
        async move {
            probe.close_started.add_permits(1);
            if probe.block_close.load(Ordering::SeqCst) {
                probe.release_close.acquire().await.unwrap().forget();
            }
            assert!(
                !probe.panic_close.load(Ordering::SeqCst),
                "injected writer close failure"
            );
            let offset = close.await;
            probe.active.fetch_sub(1, Ordering::SeqCst);
            offset
        }
        .boxed()
    }
}

fn fixture(
    id: u64,
) -> (
    tempfile::TempDir,
    HostController,
    Database,
    Arc<Probe>,
    Arc<AtomicUsize>,
) {
    let directory = tempfile::tempdir().unwrap();
    let data = Arc::new(ServerDataDir::from_path_unchecked(directory.path().to_owned()));
    let program = empty_module::program(1).unwrap();
    let initial = program.clone();
    let lookups = Arc::new(AtomicUsize::new(0));
    let lookup_count = lookups.clone();
    let storage = move |hash| {
        let initial = initial.clone();
        lookup_count.fetch_add(1, Ordering::SeqCst);
        async move { Ok((hash == initial.hash).then_some(initial.bytes)) }
    };
    let probe = Arc::new(Probe::default());
    let controller = HostController::new(
        data.clone(),
        db::Config {
            storage: db::Storage::Disk,
            page_pool_max_size: None,
        },
        HostRuntimeConfig::default(),
        Arc::new(storage),
        Arc::new(NullEnergyMonitor),
        Arc::new(ProbedProvider {
            local: LocalPersistenceProvider::new(data),
            probe: probe.clone(),
        }),
        JobCores::without_pinned_cores(),
    );
    let database = Database {
        id,
        database_identity: Identity::from_u256(id.into()),
        owner_identity: Identity::ONE,
        host_type: HostType::Wasm,
        initial_program: program.hash,
    };
    (directory, controller, database, probe, lookups)
}

async fn closed_seed(controller: &HostController, database: &Database, probe: &Probe) {
    controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    probe.close_started.acquire().await.unwrap().forget();
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
}

async fn wait_for_close(probe: &Probe) {
    timeout(Duration::from_secs(5), probe.close_started.acquire())
        .await
        .unwrap()
        .unwrap()
        .forget();
}

async fn wait_for_capacity(controller: &HostController) {
    timeout(Duration::from_secs(5), async {
        while controller.retained_capacity.available_permits() != 2 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_missing_history_does_not_create_a_replica_or_lookup_program() {
    let (_directory, controller, database, probe, lookups) = fixture(0xc001);
    assert!(controller
        .with_retained_database(database.clone(), database.id, |_, _| async { Ok(()) })
        .await
        .is_err());
    assert!(!controller.data_dir.replica(database.id).0.exists());
    assert_eq!(probe.opened.load(Ordering::SeqCst), 0);
    assert_eq!(lookups.load(Ordering::SeqCst), 0);
    assert!(controller.managed_replicas().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_idle_module_and_queued_relookup_do_not_break_positive_close() {
    let (_directory, controller, database, probe, _) = fixture(0xc002);
    let idle = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    probe.block_close.store(true, Ordering::SeqCst);
    let exiting = {
        let controller = controller.clone();
        tokio::spawn(async move { controller.exit_module_host(0xc002, Duration::from_secs(5)).await })
    };
    wait_for_close(&probe).await;
    let lookup = {
        let controller = controller.clone();
        let database = database.clone();
        tokio::spawn(async move {
            controller
                .get_or_launch_module_host(database.clone(), database.id)
                .await
        })
    };
    assert!(
        controller
            .exit_module_host(database.id, Duration::from_millis(10))
            .await
            .is_err(),
        "a timeout is not positive closure"
    );
    exiting.abort();
    assert!(exiting.await.unwrap_err().is_cancelled());
    assert_eq!(probe.opened.load(Ordering::SeqCst), 1);
    assert_eq!(probe.active.load(Ordering::SeqCst), 1);
    probe.block_close.store(false, Ordering::SeqCst);
    probe.release_close.add_permits(1);
    let current = timeout(Duration::from_secs(5), lookup).await.unwrap().unwrap().unwrap();
    assert_eq!(probe.opened.load(Ordering::SeqCst), 2);
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    assert!(!Arc::ptr_eq(idle.relational_db(), current.relational_db()));
    let seen_live = controller
        .with_retained_database(database.clone(), database.id, |db, module| async move {
            Ok(module.is_some() && db.metadata()?.is_some())
        })
        .await
        .unwrap();
    assert!(seen_live);
    assert_eq!(probe.opened.load(Ordering::SeqCst), 2);
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    assert!(controller.managed_replicas().is_empty());
    drop((idle, current));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_cancelled_call_keeps_capacity_and_writer_until_positive_close() {
    let (_directory, controller, database, probe, _) = fixture(0xc003);
    closed_seed(&controller, &database, &probe).await;
    probe.block_close.store(true, Ordering::SeqCst);
    let first = {
        let controller = controller.clone();
        let database = database.clone();
        tokio::spawn(async move {
            controller
                .with_retained_database(database.clone(), database.id, |_, module| async move {
                    assert!(module.is_none());
                    Ok(())
                })
                .await
        })
    };
    wait_for_close(&probe).await;
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    assert_eq!(controller.retained_capacity.available_permits(), 1);
    let second = {
        let controller = controller.clone();
        let database = database.clone();
        tokio::spawn(async move {
            controller
                .with_retained_database(database.clone(), database.id, |_, _| async { Ok(()) })
                .await
        })
    };
    timeout(Duration::from_secs(5), async {
        while controller.retained_capacity.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(controller
        .with_retained_database(database.clone(), database.id, |_, _| async { Ok(()) })
        .await
        .is_err());
    assert_eq!(probe.opened.load(Ordering::SeqCst), 2);
    probe.release_close.add_permits(1);
    wait_for_close(&probe).await;
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    probe.release_close.add_permits(1);
    second.await.unwrap().unwrap();
    wait_for_capacity(&controller).await;
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    assert!(controller.managed_replicas().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_failed_replay_joins_the_first_close_before_retry() {
    let (_directory, controller, database, probe, _) = fixture(0xc004);
    closed_seed(&controller, &database, &probe).await;
    let mut wrong_owner = database.clone();
    wrong_owner.owner_identity = Identity::from_u256(123456_u64.into());
    probe.block_close.store(true, Ordering::SeqCst);
    let failed = {
        let controller = controller.clone();
        tokio::spawn(async move {
            controller
                .with_retained_database(wrong_owner.clone(), wrong_owner.id, |_, _| async { Ok(()) })
                .await
        })
    };
    wait_for_close(&probe).await;
    assert!(!failed.is_finished());
    let next = {
        let controller = controller.clone();
        let database = database.clone();
        tokio::spawn(async move {
            controller
                .with_retained_database(database.clone(), database.id, |db, _| async move {
                    Ok(db.metadata()?.is_some())
                })
                .await
        })
    };
    tokio::task::yield_now().await;
    assert_eq!(probe.opened.load(Ordering::SeqCst), 2);
    probe.release_close.add_permits(1);
    assert!(failed.await.unwrap().is_err());
    wait_for_close(&probe).await;
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    probe.release_close.add_permits(1);
    assert!(next.await.unwrap().unwrap());
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_operation_panic_closes_writer_and_releases_its_pin() {
    let (_directory, controller, database, probe, _) = fixture(0xc005);
    closed_seed(&controller, &database, &probe).await;
    assert!(controller
        .with_retained_database(database.clone(), database.id, |_, _| async {
            panic!("injected trusted operation panic");
            #[allow(unreachable_code)]
            Ok::<_, anyhow::Error>(())
        })
        .await
        .is_err());
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    controller
        .with_retained_database(database.clone(), database.id, |_, _| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    assert!(controller.managed_replicas().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_failed_normal_initialization_closes_writer_before_cold_retry() {
    let (_directory, controller, database, probe, _) = fixture(0xc008);
    closed_seed(&controller, &database, &probe).await;
    let mut wrong_owner = database.clone();
    wrong_owner.owner_identity = Identity::from_u256(123456_u64.into());
    probe.block_close.store(true, Ordering::SeqCst);
    let failed = {
        let controller = controller.clone();
        tokio::spawn(async move {
            controller
                .get_or_launch_module_host(wrong_owner.clone(), wrong_owner.id)
                .await
        })
    };
    wait_for_close(&probe).await;
    let next = {
        let controller = controller.clone();
        let database = database.clone();
        tokio::spawn(async move {
            controller
                .with_retained_database(database.clone(), database.id, |_, module| async move {
                    assert!(module.is_none());
                    Ok(())
                })
                .await
        })
    };
    assert!(!failed.is_finished());
    probe.release_close.add_permits(1);
    assert!(failed.await.unwrap().is_err());
    wait_for_close(&probe).await;
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    probe.release_close.add_permits(1);
    next.await.unwrap().unwrap();
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_nonempty_history_without_initialized_module_is_rejected() {
    let (_directory, controller, database, probe, lookups) = fixture(0xc009);
    let (db, _, joined) = retained::open_database(&controller, &database, database.id, false, None)
        .await
        .unwrap();
    db.with_auto_commit(Workload::ForTests, |tx| {
        crate::db::environment::set(&db, tx, "PENDING", "value")
    })
    .unwrap();
    assert!(db.metadata().unwrap().is_none());
    joined.unwrap().close().await;
    drop(db);
    let result = controller
        .with_retained_database(database.clone(), database.id, |_, _| async {
            panic!("uninitialized storage must never reach the operation");
            #[allow(unreachable_code)]
            Ok::<_, anyhow::Error>(())
        })
        .await;
    assert!(result.unwrap_err().to_string().contains("not initialized"));
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    assert_eq!(lookups.load(Ordering::SeqCst), 0);
    assert!(controller.managed_replicas().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_module_exit_panic_reports_error_after_positive_writer_close_and_allows_reopen() {
    let (_directory, controller, database, probe, _) = fixture(0xc00a);
    let idle = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    retained::FAIL_NEXT_MODULE_EXIT
        .lock()
        .insert(database.database_identity);
    let error = controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("module exit panicked"));
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    assert!(controller.managed_replicas().is_empty());
    controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    drop(idle);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_unconfirmed_writer_close_reports_error_and_quarantines_capacity_and_replica() {
    let (_directory, controller, database, probe, _) = fixture(0xc00b);
    closed_seed(&controller, &database, &probe).await;
    probe.panic_close.store(true, Ordering::SeqCst);
    let error = controller
        .with_retained_database(database.clone(), database.id, |_, _| async { Ok(()) })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("writer close panicked"));
    assert_eq!(controller.retained_capacity.available_permits(), 1);
    let error = timeout(
        Duration::from_secs(1),
        controller.get_or_launch_module_host(database.clone(), database.id),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.to_string().contains("unable to lock"));
    assert_eq!(probe.opened.load(Ordering::SeqCst), 2);
    assert!(controller
        .exit_module_host(database.id, Duration::from_secs(1))
        .await
        .unwrap_err()
        .to_string()
        .contains("quarantined"));
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_panic_callback_before_initial_host_install_still_owns_cleanup() {
    let (_directory, controller, database, probe, _) = fixture(0xc00c);
    retained::PANIC_CALLBACK_AT_INITIAL_SCHEDULER_START
        .lock()
        .insert(database.database_identity);
    let idle = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    // No explicit exit is requested until the injected scheduler interleaving
    // has independently started the actual storage writer close.
    wait_for_close(&probe).await;
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    assert!(controller.get_module_host(database.id).await.is_err());
    controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    drop(idle);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_stale_panic_callback_does_not_unregister_updated_or_reopened_host() {
    let (_directory, controller, database, probe, _) = fixture(0xc006);
    controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    let old_callback = {
        let guard = controller.acquire_read_lock(database.id).await.unwrap();
        controller.unregister_fn(guard.as_ref().unwrap().registration.clone())
    };
    let mut newer = empty_module::VERSION_1_BYTES.to_vec();
    newer.extend_from_slice(&[0, 3, 1, b'x', 1]);
    let newer_hash = spacetimedb_lib::hash_bytes(&newer);
    controller
        .update_module_host(
            database.clone(),
            HostType::Wasm,
            database.id,
            newer.into(),
            MigrationPolicy::Compatible,
        )
        .await
        .unwrap();
    old_callback();
    assert_eq!(
        controller.get_module_host(database.id).await.unwrap().info.module_hash,
        newer_hash
    );
    assert_eq!(probe.active.load(Ordering::SeqCst), 1);
    let current_callback = {
        let guard = controller.acquire_read_lock(database.id).await.unwrap();
        controller.unregister_fn(guard.as_ref().unwrap().registration.clone())
    };
    current_callback();
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    assert!(controller.get_module_host(database.id).await.is_err());
    controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    current_callback();
    assert!(controller.get_module_host(database.id).await.is_ok());
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn retained_cold_snapshot_cleanup_never_executes_stored_javascript_or_opens_admission() {
    use crate::db::deployment::{install_container_fence, install_publication_fence, record_deployment_commit};
    use crate::host::container_environment;
    use spacetimedb_datastore::system_tables::StContainerFenceRow;
    use spacetimedb_lib::container::*;
    use spacetimedb_lib::container_environment::EnvironmentSnapshotScope;
    use spacetimedb_lib::deployment::{DeploymentSpec, DeploymentSpecV1, ModuleComponent, UserModule, UserModuleKind};
    use spacetimedb_lib::{hash_bytes, Uuid};

    let (_directory, controller, database, probe, lookups) = fixture(0xc007);
    let idle = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    let hostile = Program::from_bytes(
        ModuleKind::JS,
        b"throw new Error('cold storage must never execute this module');".to_vec(),
    );
    let uuid = || Uuid::from_u128(uuid::Uuid::now_v7().as_u128());
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
        env_keys: vec!["SECRET".into()],
        resources: ContainerResources {
            cpu_millicores: 1000,
            memory_bytes: 64 * 1024 * 1024,
            scratch_bytes: 64 * 1024 * 1024,
            pids_max: 64,
        },
        ports: vec![],
        mounts: vec![],
        stop_grace_ms: DEFAULT_STOP_GRACE_MS,
    };
    let publication = DeploymentCommit {
        operation_id: uuid(),
        publication_epoch: 1,
        publisher: database.owner_identity,
        expected_revision: None,
        prepared_manifest_hash: hash_bytes(b"cold cleanup fixture"),
        deployment: DeploymentSpec::V1(DeploymentSpecV1 {
            module: ModuleComponent::User(UserModule {
                kind: UserModuleKind::Js,
                program_hash: hostile.hash,
            }),
            container: Some(spec.clone()),
        }),
    };
    let fence = StContainerFenceRow {
        source_identity: database.database_identity.into(),
        generation: 1,
        target_grant_revision: 1,
        target_set_hash: hash_bytes(b"targets"),
        allowed: true,
    };
    idle.relational_db()
        .with_auto_commit(Workload::ForTests, |tx| -> anyhow::Result<()> {
            idle.relational_db().update_program(tx, hostile)?;
            install_publication_fence(tx, publication.publication_epoch, publication.operation_id)?;
            record_deployment_commit(tx, &publication, Timestamp::now(), &Default::default())?;
            install_container_fence(idle.relational_db(), tx, &fence)?;
            crate::db::environment::set(idle.relational_db(), tx, "SECRET", "retained-fixture-secret")?;
            Ok(())
        })
        .unwrap();
    let scope = EnvironmentSnapshotScope {
        cluster: "local-test".into(),
        database_id: database.id,
        database_identity: database.database_identity,
        node_id: 2,
        node_incarnation: uuid(),
        generation: 1,
        deployment_revision: publication.deployment.revision().unwrap(),
        start_request: publication.operation_id,
        env_generation: uuid(),
        env_keys: spec.env_keys,
    };
    let receipt = container_environment::capture(idle.relational_db().clone(), scope.clone())
        .await
        .unwrap();
    idle.relational_db()
        .hosted_admission()
        .begin()
        .unwrap()
        .complete()
        .unwrap();
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    let expected_scope = scope.clone();
    controller
        .with_retained_database(database.clone(), database.id, move |db, module| async move {
            assert!(module.is_none());
            assert!(!db.hosted_admission().is_open());
            let values = container_environment::read(db.clone(), receipt.receipt).await?;
            assert_eq!(values.receipt.selected_values["SECRET"], "retained-fixture-secret");
            db.with_auto_commit(Workload::ForTests, |tx| {
                install_container_fence(
                    &db,
                    tx,
                    &StContainerFenceRow {
                        generation: 2,
                        allowed: false,
                        ..fence
                    },
                )
            })?;
            container_environment::close(db, expected_scope, 2).await?;
            Ok(())
        })
        .await
        .unwrap();
    assert!(controller.get_module_host(database.id).await.is_err());
    assert_eq!(lookups.load(Ordering::SeqCst), 1);
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    assert_eq!(controller.retained_capacity.available_permits(), 2);
    drop(idle);
}
