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
    async fn persistence(
        &self,
        database: &crate::db::persistence::Database,
        replica: u64,
    ) -> anyhow::Result<Persistence> {
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
    let program = empty_module::program(empty_module::VERSION).unwrap();
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
        Arc::new(()),
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
        bootstrap_generation: 0,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_idle_module_and_queued_relookup_do_not_break_positive_close() {
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
    let seen_live = controller.get_module_host(database.id).await.unwrap();
    assert!(Arc::ptr_eq(seen_live.relational_db(), current.relational_db()));
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
async fn lifecycle_failed_normal_initialization_joins_writer_before_retry() {
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
                .get_or_launch_module_host(database.clone(), database.id)
                .await
        })
    };
    assert!(!failed.is_finished());
    probe.release_close.add_permits(1);
    assert!(failed.await.unwrap().is_err());
    let reopened = next.await.unwrap().unwrap();
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    probe.block_close.store(false, Ordering::SeqCst);
    controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(probe.active.load(Ordering::SeqCst), 0);
    drop(reopened);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_module_exit_panic_reports_error_after_positive_writer_close_and_allows_reopen() {
    let (_directory, controller, database, probe, _) = fixture(0xc00a);
    let idle = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    lifecycle::FAIL_NEXT_MODULE_EXIT
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
async fn lifecycle_unconfirmed_writer_close_reports_error_and_quarantines_replica() {
    let (_directory, controller, database, probe, _) = fixture(0xc00b);
    let module = controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    probe.panic_close.store(true, Ordering::SeqCst);
    let error = controller
        .exit_module_host(database.id, Duration::from_secs(5))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("writer close panicked"));
    let error = timeout(
        Duration::from_secs(1),
        controller.get_or_launch_module_host(database.clone(), database.id),
    )
    .await
    .unwrap()
    .expect_err("quarantined host must not reopen");
    assert!(error.to_string().contains("unable to lock"));
    assert_eq!(probe.opened.load(Ordering::SeqCst), 1);
    assert!(controller
        .exit_module_host(database.id, Duration::from_secs(1))
        .await
        .unwrap_err()
        .to_string()
        .contains("quarantined"));
    assert_eq!(probe.maximum.load(Ordering::SeqCst), 1);
    drop(module);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lifecycle_panic_callback_before_initial_host_install_still_owns_cleanup() {
    let (_directory, controller, database, probe, _) = fixture(0xc00c);
    lifecycle::PANIC_CALLBACK_AT_INITIAL_SCHEDULER_START
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
async fn lifecycle_stale_panic_callback_does_not_unregister_updated_or_reopened_host() {
    let (_directory, controller, database, probe, _) = fixture(0xc006);
    controller
        .get_or_launch_module_host(database.clone(), database.id)
        .await
        .unwrap();
    let old_callback = {
        let guard = controller.acquire_read_lock(database.id).await.unwrap();
        controller.unregister_fn(guard.as_ref().unwrap().registration.clone(), database.database_identity)
    };
    let mut newer = spacetimedb_lib::deployment::system_empty::empty().bytes.to_vec();
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
        controller.unregister_fn(guard.as_ref().unwrap().registration.clone(), database.database_identity)
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
