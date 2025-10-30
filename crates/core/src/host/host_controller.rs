use super::module_host::{EventStatus, ModuleHost, ModuleInfo, NoSuchModule};
use super::scheduler::SchedulerStarter;
use super::wasmtime::WasmtimeRuntime;
use super::{Scheduler, UpdateDatabaseResult};
use crate::client::{ClientActorId, ClientName};
use crate::database_logger::DatabaseLogger;
use crate::db::persistence::PersistenceProvider;
use crate::db::relational_db::{self, DiskSizeFn, RelationalDB, Txdata};
use crate::db::{self, spawn_tx_metrics_recorder};
use crate::energy::{EnergyMonitor, EnergyQuanta, NullEnergyMonitor};
use crate::host::module_host::ModuleRuntime as _;
use crate::host::v8::V8Runtime;
use crate::messages::control_db::{Database, HostType};
use crate::module_host_context::ModuleCreationContext;
use crate::replica_context::ReplicaContext;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use crate::subscription::module_subscription_manager::{spawn_send_worker, SubscriptionManager};
use crate::util::asyncify;
use crate::util::jobs::{JobCores, SingleCoreExecutor};
use crate::worker_metrics::WORKER_METRICS;
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use durability::{Durability, EmptyHistory};
use log::{info, trace, warn};
use parking_lot::Mutex;
use spacetimedb_data_structures::error_stream::ErrorStream;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_datastore::db_metrics::data_size::DATA_SIZE_METRICS;
use spacetimedb_datastore::db_metrics::DB_METRICS;
use spacetimedb_datastore::traits::Program;
use spacetimedb_durability::{self as durability};
use spacetimedb_lib::{hash_bytes, AlgebraicValue, Identity, Timestamp};
use spacetimedb_paths::server::{ReplicaDir, ServerDataDir};
use spacetimedb_paths::FromPathUnchecked;
use spacetimedb_sats::hash::Hash;
use spacetimedb_schema::auto_migrate::{ponder_migrate, AutoMigrateError, MigrationPolicy, PrettyPrintStyle};
use spacetimedb_schema::def::ModuleDef;
use spacetimedb_table::page_pool::PagePool;
use std::future::Future;
use std::ops::Deref;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::{watch, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock as AsyncRwLock};
use tokio::task::AbortHandle;

// TODO:
//
// - [db::Config] should be per-[Database]

/// A shared mutable cell containing a module host and associated database.
type HostCell = Arc<AsyncRwLock<Option<Host>>>;

/// The registry of all running hosts.
type Hosts = Arc<Mutex<IntMap<u64, HostCell>>>;

pub type ExternalDurability = (Arc<dyn Durability<TxData = Txdata>>, DiskSizeFn);

#[async_trait]
pub trait ExternalStorage: Send + Sync + 'static {
    async fn lookup(&self, program_hash: Hash) -> anyhow::Result<Option<Box<[u8]>>>;
}
#[async_trait]
impl<F, Fut> ExternalStorage for F
where
    F: Fn(Hash) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<Option<Box<[u8]>>>> + Send,
{
    async fn lookup(&self, program_hash: Hash) -> anyhow::Result<Option<Box<[u8]>>> {
        self(program_hash).await
    }
}

pub type ProgramStorage = Arc<dyn ExternalStorage>;

/// A host controller manages the lifecycle of spacetime databases and their
/// associated modules.
///
/// This type is, and must remain, cheap to clone.
/// All of its fields should either be [`Copy`], enclosed in an [`Arc`],
/// or have some other fast [`Clone`] implementation.
#[derive(Clone)]
pub struct HostController {
    /// Map of all hosts managed by this controller,
    /// keyed by database instance id.
    hosts: Hosts,
    /// The root directory for database data.
    pub data_dir: Arc<ServerDataDir>,
    /// The default configuration to use for databases created by this
    /// controller.
    default_config: db::Config,
    /// The [`ProgramStorage`] to query when instantiating a module.
    program_storage: ProgramStorage,
    /// The [`EnergyMonitor`] used by this controller.
    energy_monitor: Arc<dyn EnergyMonitor>,
    /// Provides persistence services for each replica.
    persistence: Arc<dyn PersistenceProvider>,
    /// The page pool all databases will use by cloning the ref counted pool.
    pub page_pool: PagePool,
    /// The runtimes for running our modules.
    runtimes: Arc<HostRuntimes>,
    /// The CPU cores that are reserved for ModuleHost operations to run on.
    db_cores: JobCores,
}

struct HostRuntimes {
    wasmtime: WasmtimeRuntime,
    v8: V8Runtime,
}

impl HostRuntimes {
    fn new(data_dir: Option<&ServerDataDir>) -> Arc<Self> {
        let wasmtime = WasmtimeRuntime::new(data_dir);
        let v8 = V8Runtime::default();
        Arc::new(Self { wasmtime, v8 })
    }
}

#[derive(Clone, Debug)]
pub struct ReducerCallResult {
    pub outcome: ReducerOutcome,
    pub energy_used: EnergyQuanta,
    pub execution_duration: Duration,
}

impl ReducerCallResult {
    pub fn is_err(&self) -> bool {
        self.outcome.is_err()
    }

    pub fn is_ok(&self) -> bool {
        !self.is_err()
    }
}

impl From<ReducerCallResult> for Result<(), anyhow::Error> {
    fn from(value: ReducerCallResult) -> Self {
        value.outcome.into_result()
    }
}

#[derive(Clone, Debug)]
pub enum ReducerOutcome {
    Committed,
    Failed(String),
    BudgetExceeded,
}

impl ReducerOutcome {
    pub fn into_result(self) -> anyhow::Result<()> {
        match self {
            Self::Committed => Ok(()),
            Self::Failed(e) => Err(anyhow::anyhow!(e)),
            Self::BudgetExceeded => Err(anyhow::anyhow!("reducer ran out of energy")),
        }
    }

    pub fn is_err(&self) -> bool {
        !matches!(self, Self::Committed)
    }
}

impl From<&EventStatus> for ReducerOutcome {
    fn from(status: &EventStatus) -> Self {
        match &status {
            EventStatus::Committed(_) => ReducerOutcome::Committed,
            EventStatus::Failed(e) => ReducerOutcome::Failed(e.clone()),
            EventStatus::OutOfEnergy => ReducerOutcome::BudgetExceeded,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProcedureCallResult {
    pub return_val: AlgebraicValue,
    pub execution_duration: Duration,
    pub start_timestamp: Timestamp,
}

impl HostController {
    pub fn new(
        data_dir: Arc<ServerDataDir>,
        default_config: db::Config,
        program_storage: ProgramStorage,
        energy_monitor: Arc<impl EnergyMonitor>,
        persistence: Arc<dyn PersistenceProvider>,
        db_cores: JobCores,
    ) -> Self {
        Self {
            hosts: <_>::default(),
            default_config,
            program_storage,
            energy_monitor,
            persistence,
            runtimes: HostRuntimes::new(Some(&data_dir)),
            data_dir,
            page_pool: PagePool::new(default_config.page_pool_max_size),
            db_cores,
        }
    }

    /// Replace the [`ProgramStorage`] used by this controller.
    pub fn set_program_storage(&mut self, ps: ProgramStorage) {
        self.program_storage = ps;
    }

    /// Get a [`ModuleHost`] managed by this controller, or launch it from
    /// persistent state.
    ///
    /// If the host is not running, it is started according to the default
    /// [`db::Config`] set for this controller.
    ///   The underlying database is restored from existing data at its
    /// canonical filesystem location _iff_ the default config mandates disk
    /// storage.
    ///
    /// The module will be instantiated from the program bytes stored in an
    /// existing database.
    ///   If the database is empty, the `program_bytes_address` of the given
    /// [`Database`] will be used to load the program from the controller's
    /// [`ProgramStorage`]. The initialization procedure (schema creation,
    /// `__init__` reducer) will be invoked on the found module, and the
    /// database will be marked as initialized.
    ///
    /// See also: [`Self::get_module_host`]
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn get_or_launch_module_host(&self, database: Database, replica_id: u64) -> anyhow::Result<ModuleHost> {
        let mut rx = self.watch_maybe_launch_module_host(database, replica_id).await?;
        let module = rx.borrow_and_update();
        Ok(module.clone())
    }

    /// Like [`Self::get_or_launch_module_host`], use a [`ModuleHost`] managed
    /// by this controller, or launch it if it is not running.
    ///
    /// Instead of a [`ModuleHost`], this returns a [`watch::Receiver`] which
    /// gets notified each time the module is updated.
    ///
    /// See also: [`Self::watch_module_host`]
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn watch_maybe_launch_module_host(
        &self,
        database: Database,
        replica_id: u64,
    ) -> anyhow::Result<watch::Receiver<ModuleHost>> {
        // Try a read lock first.
        {
            let guard = self.acquire_read_lock(replica_id).await;
            if let Some(host) = &*guard {
                trace!("cached host {}/{}", database.database_identity, replica_id);
                return Ok(host.module.subscribe());
            }
        }

        // We didn't find a running module, so take a write lock.
        // Since [`tokio::sync::RwLock`] doesn't support upgrading of read locks,
        // we'll need to check again if a module was added meanwhile.
        let mut guard = self.acquire_write_lock(replica_id).await;
        if let Some(host) = &*guard {
            trace!(
                "cached host {}/{} (lock upgrade)",
                database.database_identity,
                replica_id
            );
            return Ok(host.module.subscribe());
        }

        trace!("launch host {}/{}", database.database_identity, replica_id);

        // `HostController::clone` is fast,
        // as all of its fields are either `Copy` or wrapped in `Arc`.
        let this = self.clone();

        // `try_init_host` is not cancel safe, as it will spawn other async tasks
        // which hold a filesystem lock past when `try_init_host` returns or is cancelled.
        // This means that, if `try_init_host` is cancelled, subsequent calls will fail.
        //
        // This is problematic because Axum will cancel its handler tasks if the client disconnects,
        // and this method is called from Axum handlers, e.g. for the subscribe route.
        // `tokio::spawn` a task to build the `Host` and install it in the `guard`,
        // so that it will run to completion even if the caller goes away.
        //
        // Note that `tokio::spawn` only cancels its tasks when the runtime shuts down,
        // at which point we won't be calling `try_init_host` again anyways.
        let rx = tokio::spawn(async move {
            let host = this.try_init_host(database, replica_id).await?;

            let rx = host.module.subscribe();
            *guard = Some(host);

            Ok::<_, anyhow::Error>(rx)
        })
        .await??;

        Ok(rx)
    }

    /// Construct an in-memory instance of `database` running `program`,
    /// initialize it, then immediately destroy it.
    ///
    /// This is used during an initial, fresh publish operation
    /// in order to check the `program`'s validity as a module,
    /// since some validity checks we'd like to do (e.g. typechecking RLS filters)
    /// require a fully instantiated database.
    ///
    /// This is not necessary during hotswap publishes,
    /// as the automigration planner and executor accomplish the same validity checks.
    pub async fn check_module_validity(&self, database: Database, program: Program) -> anyhow::Result<Arc<ModuleInfo>> {
        Host::try_init_in_memory_to_check(
            &self.runtimes,
            self.page_pool.clone(),
            database,
            program,
            // This takes a db core to check validity, and we will later take
            // another core to actually run the module. Due to the round-robin
            // algorithm that JobCores uses, that will likely just be the same
            // core - there's not a concern that we'll only end up using 1/2
            // of the actual cores.
            self.db_cores.take(),
        )
        .await
    }

    /// Run a computation on the [`RelationalDB`] of a [`ModuleHost`] managed by
    /// this controller, launching the host if necessary.
    ///
    /// If the computation `F` panics, the host is removed from this controller,
    /// releasing its resources.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn using_database<F, T>(&self, database: Database, replica_id: u64, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&RelationalDB) -> T + Send + 'static,
        T: Send + 'static,
    {
        trace!("using database {}/{}", database.database_identity, replica_id);
        let module = self.get_or_launch_module_host(database, replica_id).await?;
        let on_panic = self.unregister_fn(replica_id);
        scopeguard::defer_on_unwind!({
            warn!("database operation panicked");
            on_panic();
        });

        let db = module.replica_ctx().relational_db.clone();
        let result = module.on_module_thread("using_database", move || f(&db)).await?;
        Ok(result)
    }

    /// Update the [`ModuleHost`] identified by `replica_id` to the given
    /// program.
    ///
    /// The host may not be running, in which case it is spawned (see
    /// [`Self::get_or_launch_module_host`] for details on what this entails).
    ///
    /// If the host was running, and the update fails, the previous version of
    /// the host keeps running.
    #[tracing::instrument(level = "trace", skip_all, err)]
    pub async fn update_module_host(
        &self,
        database: Database,
        host_type: HostType,
        replica_id: u64,
        program_bytes: Box<[u8]>,
        policy: MigrationPolicy,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        let program = Program {
            hash: hash_bytes(&program_bytes),
            bytes: program_bytes,
        };
        trace!(
            "update module host {}/{}: genesis={} update-to={}",
            database.database_identity,
            replica_id,
            database.initial_program,
            program.hash
        );

        let mut guard = self.acquire_write_lock(replica_id).await;

        // `HostController::clone` is fast,
        // as all of its fields are either `Copy` or wrapped in `Arc`.
        let this = self.clone();

        // `try_init_host` is not cancel safe, as it will spawn other async tasks
        // which hold a filesystem lock past when `try_init_host` returns or is cancelled.
        // This means that, if `try_init_host` is cancelled, subsequent calls will fail.
        //
        // The rest of this future is also not cancel safe, as it will `Option::take` out of the guard
        // at the start of the block and then store back into it at the end.
        //
        // This is problematic because Axum will cancel its handler tasks if the client disconnects,
        // and this method is called from Axum handlers, e.g. for the publish route.
        // `tokio::spawn` a task to update the contents of `guard`,
        // so that it will run to completion even if the caller goes away.
        //
        // Note that `tokio::spawn` only cancels its tasks when the runtime shuts down,
        // at which point we won't be calling `try_init_host` again anyways.
        let update_result = tokio::spawn(async move {
            let mut host = match guard.take() {
                None => {
                    trace!("host not running, try_init");
                    this.try_init_host(database, replica_id).await?
                }
                Some(host) => {
                    trace!("host found, updating");
                    host
                }
            };
            let update_result = host
                .update_module(
                    this.runtimes.clone(),
                    host_type,
                    program,
                    policy,
                    this.energy_monitor.clone(),
                    this.unregister_fn(replica_id),
                    this.db_cores.take(),
                )
                .await?;

            *guard = Some(host);

            Ok::<_, anyhow::Error>(update_result)
        })
        .await??;

        Ok(update_result)
    }

    pub async fn migrate_plan(
        &self,
        database: Database,
        host_type: HostType,
        replica_id: u64,
        program_bytes: Box<[u8]>,
        style: PrettyPrintStyle,
    ) -> anyhow::Result<MigratePlanResult> {
        let program = Program {
            hash: hash_bytes(&program_bytes),
            bytes: program_bytes,
        };
        trace!(
            "migrate plan {}/{}: genesis={} update-to={}",
            database.database_identity,
            replica_id,
            database.initial_program,
            program.hash
        );

        let guard = self.acquire_read_lock(replica_id).await;
        let host = guard.as_ref().ok_or(NoSuchModule)?;

        host.migrate_plan(host_type, program, style).await
    }

    /// Release all resources of the [`ModuleHost`] identified by `replica_id`,
    /// and deregister it from the controller.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn exit_module_host(&self, replica_id: u64) -> Result<(), anyhow::Error> {
        trace!("exit module host {replica_id}");
        let lock = self.hosts.lock().remove(&replica_id);
        if let Some(lock) = lock {
            if let Some(host) = lock.write_owned().await.take() {
                let module = host.module.borrow().clone();
                module.exit().await;
                let table_names = module.info().module_def.tables().map(|t| t.name.deref());
                remove_database_gauges(&module.info().database_identity, table_names);
            }
        }

        Ok(())
    }

    /// Get the [`ModuleHost`] identified by `replica_id` or return an error
    /// if it is not registered with the controller.
    ///
    /// See [`Self::get_or_launch_module_host`] for a variant which launches
    /// the host if it is not running.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn get_module_host(&self, replica_id: u64) -> Result<ModuleHost, NoSuchModule> {
        trace!("get module host {replica_id}");
        let guard = self.acquire_read_lock(replica_id).await;
        guard
            .as_ref()
            .map(|Host { module, .. }| module.borrow().clone())
            .ok_or(NoSuchModule)
    }

    /// Subscribe to updates of the [`ModuleHost`] identified by `replica_id`,
    /// or return an error if it is not registered with the controller.
    ///
    /// See [`Self::watch_maybe_launch_module_host`] for a variant which
    /// launches the host if it is not running.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn watch_module_host(&self, replica_id: u64) -> Result<watch::Receiver<ModuleHost>, NoSuchModule> {
        trace!("watch module host {replica_id}");
        let guard = self.acquire_read_lock(replica_id).await;
        guard
            .as_ref()
            .map(|Host { module, .. }| module.subscribe())
            .ok_or(NoSuchModule)
    }

    /// `true` if the module host `replica_id` is currently registered with
    /// the controller.
    pub async fn has_module_host(&self, replica_id: u64) -> bool {
        self.acquire_read_lock(replica_id).await.is_some()
    }

    /// On-panic callback passed to [`ModuleHost`]s created by this controller.
    ///
    /// Removes the module with the given `replica_id` from this controller.
    fn unregister_fn(&self, replica_id: u64) -> impl Fn() + Send + Sync + 'static {
        let hosts = Arc::downgrade(&self.hosts);
        move || {
            if let Some(hosts) = hosts.upgrade() {
                hosts.lock().remove(&replica_id);
            }
        }
    }

    async fn acquire_write_lock(&self, replica_id: u64) -> OwnedRwLockWriteGuard<Option<Host>> {
        let lock = self.hosts.lock().entry(replica_id).or_default().clone();
        lock.write_owned().await
    }

    async fn acquire_read_lock(&self, replica_id: u64) -> OwnedRwLockReadGuard<Option<Host>> {
        let lock = self.hosts.lock().entry(replica_id).or_default().clone();
        lock.read_owned().await
    }

    async fn try_init_host(&self, database: Database, replica_id: u64) -> anyhow::Result<Host> {
        let database_identity = database.database_identity;
        Host::try_init(self, database, replica_id)
            .await
            .with_context(|| format!("failed to init replica {} for {}", replica_id, database_identity))
    }
}

fn stored_program_hash(db: &RelationalDB) -> anyhow::Result<Option<Hash>> {
    let meta = db.metadata()?;
    Ok(meta.map(|meta| meta.program_hash))
}

async fn make_replica_ctx(
    path: ReplicaDir,
    database: Database,
    replica_id: u64,
    relational_db: Arc<RelationalDB>,
) -> anyhow::Result<ReplicaContext> {
    let logger = tokio::task::block_in_place(move || Arc::new(DatabaseLogger::open_today(path.module_logs())));
    let send_worker_queue = spawn_send_worker(Some(database.database_identity));
    let subscriptions = Arc::new(parking_lot::RwLock::new(SubscriptionManager::new(
        send_worker_queue.clone(),
    )));
    let downgraded = Arc::downgrade(&subscriptions);
    let subscriptions = ModuleSubscriptions::new(
        relational_db.clone(),
        subscriptions,
        send_worker_queue,
        database.owner_identity,
    );

    // If an error occurs when evaluating a subscription,
    // we mark each client that was affected,
    // and we remove those clients from the manager async.
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let Some(subscriptions) = downgraded.upgrade() else {
                break;
            };
            // This should happen on the module thread, but we haven't created the module yet.
            asyncify(move || subscriptions.write().remove_dropped_clients()).await
        }
    });

    Ok(ReplicaContext {
        database,
        replica_id,
        logger,
        subscriptions,
        relational_db,
    })
}

/// Initialize a module host for the given program.
/// The passed replica_ctx may not be configured for this version of the program's database schema yet.
#[allow(clippy::too_many_arguments)]
async fn make_module_host(
    runtimes: Arc<HostRuntimes>,
    host_type: HostType,
    replica_ctx: Arc<ReplicaContext>,
    scheduler: Scheduler,
    program: Program,
    energy_monitor: Arc<dyn EnergyMonitor>,
    unregister: impl Fn() + Send + Sync + 'static,
    executor: SingleCoreExecutor,
) -> anyhow::Result<(Program, ModuleHost)> {
    // `make_actor` is blocking, as it needs to compile the wasm to native code,
    // which may be computationally expensive - sometimes up to 1s for a large module.
    // TODO: change back to using `spawn_rayon` here - asyncify runs on tokio blocking
    //       threads, but those aren't for computation. Also, wasmtime uses rayon
    //       to run compilation in parallel, so it'll need to run stuff in rayon anyway.
    asyncify(move || {
        let database_identity = replica_ctx.database_identity;

        let mcc = ModuleCreationContext {
            replica_ctx,
            scheduler,
            program: &program,
            energy_monitor,
        };

        let start = Instant::now();
        let module_host = match host_type {
            HostType::Wasm => {
                let (actor, init_inst) = runtimes.wasmtime.make_actor(mcc)?;
                trace!("wasmtime::make_actor blocked for {:?}", start.elapsed());
                ModuleHost::new(actor, init_inst, unregister, executor, database_identity)
            }
            HostType::Js => {
                let (actor, init_inst) = runtimes.v8.make_actor(mcc)?;
                trace!("v8::make_actor blocked for {:?}", start.elapsed());
                ModuleHost::new(actor, init_inst, unregister, executor, database_identity)
            }
        };
        Ok((program, module_host))
    })
    .await
}

async fn load_program(storage: &ProgramStorage, hash: Hash) -> anyhow::Result<Program> {
    let bytes = storage
        .lookup(hash)
        .await?
        .with_context(|| format!("program {hash} not found"))?;
    Ok(Program { hash, bytes })
}

struct LaunchedModule {
    replica_ctx: Arc<ReplicaContext>,
    module_host: ModuleHost,
    scheduler: Scheduler,
    scheduler_starter: SchedulerStarter,
}

#[allow(clippy::too_many_arguments)]
async fn launch_module(
    database: Database,
    replica_id: u64,
    program: Program,
    on_panic: impl Fn() + Send + Sync + 'static,
    relational_db: Arc<RelationalDB>,
    energy_monitor: Arc<dyn EnergyMonitor>,
    replica_dir: ReplicaDir,
    runtimes: Arc<HostRuntimes>,
    executor: SingleCoreExecutor,
) -> anyhow::Result<(Program, LaunchedModule)> {
    let db_identity = database.database_identity;
    let host_type = database.host_type;

    let replica_ctx = make_replica_ctx(replica_dir, database, replica_id, relational_db)
        .await
        .map(Arc::new)?;
    let (scheduler, scheduler_starter) = Scheduler::open(replica_ctx.relational_db.clone());
    let (program, module_host) = make_module_host(
        runtimes.clone(),
        host_type,
        replica_ctx.clone(),
        scheduler.clone(),
        program,
        energy_monitor.clone(),
        on_panic,
        executor,
    )
    .await?;

    trace!("launched database {} with program {}", db_identity, program.hash);

    Ok((
        program,
        LaunchedModule {
            replica_ctx,
            module_host,
            scheduler,
            scheduler_starter,
        },
    ))
}

/// Update a module.
///
/// If the `db` is not initialized yet (i.e. its program hash is `None`),
/// return an error.
///
/// Otherwise, if `db.program_hash` matches the given `program_hash`, do
/// nothing and return an empty `UpdateDatabaseResult`.
///
/// Otherwise, invoke `module.update_database` and return the result.
async fn update_module(
    db: &RelationalDB,
    module: &ModuleHost,
    program: Program,
    old_module_info: Arc<ModuleInfo>,
    policy: MigrationPolicy,
) -> anyhow::Result<UpdateDatabaseResult> {
    let addr = db.database_identity();
    match stored_program_hash(db)? {
        None => Err(anyhow!("database `{addr}` not yet initialized")),
        Some(stored) => {
            let res = if stored == program.hash {
                info!("database `{}` up to date with program `{}`", addr, program.hash);
                UpdateDatabaseResult::NoUpdateNeeded
            } else {
                info!("updating `{}` from {} to {}", addr, stored, program.hash);
                module.update_database(program, old_module_info, policy).await?
            };

            Ok(res)
        }
    }
}

/// Encapsulates a database, associated module, and auxiliary state.
struct Host {
    /// The [`ModuleHost`], providing the callable reducer API.
    ///
    /// Modules may be updated via [`Host::update_module`].
    /// The module is wrapped in a [`watch::Sender`] to allow for "hot swapping":
    /// clients may subscribe to the channel, so they get the most recent
    /// [`ModuleHost`] version or an error if the [`Host`] was dropped.
    module: watch::Sender<ModuleHost>,
    /// Pointer to the `module`'s [`ReplicaContext`].
    ///
    /// The database stays the same if and when the module is updated via
    /// [`Host::update_module`].
    replica_ctx: Arc<ReplicaContext>,
    /// Scheduler for repeating reducers, operating on the current `module`.
    scheduler: Scheduler,
    /// Handle to the metrics collection task started via [`disk_monitor`].
    ///
    /// The task collects metrics from the `replica_ctx`, and so stays alive as long
    /// as the `replica_ctx` is live. The task is aborted when [`Host`] is dropped.
    disk_metrics_recorder_task: AbortHandle,
    /// Handle to the task responsible for recording metrics for each transaction.
    /// The task is aborted when [`Host`] is dropped.
    tx_metrics_recorder_task: AbortHandle,
}

impl Host {
    /// Attempt to instantiate a [`Host`] from persistent storage.
    ///
    /// Note that this does **not** run module initialization routines, but may
    /// create on-disk artifacts if the host / database did not exist.
    #[tracing::instrument(level = "debug", skip_all)]
    async fn try_init(host_controller: &HostController, database: Database, replica_id: u64) -> anyhow::Result<Self> {
        let HostController {
            data_dir,
            default_config: config,
            program_storage,
            energy_monitor,
            runtimes,
            persistence,
            page_pool,
            ..
        } = host_controller;
        let on_panic = host_controller.unregister_fn(replica_id);
        let replica_dir = data_dir.replica(replica_id);
        let (tx_metrics_queue, tx_metrics_recorder_task) = spawn_tx_metrics_recorder();

        let (db, connected_clients) = match config.storage {
            db::Storage::Memory => RelationalDB::open(
                &replica_dir,
                database.database_identity,
                database.owner_identity,
                EmptyHistory::new(),
                None,
                Some(tx_metrics_queue),
                page_pool.clone(),
            )?,
            db::Storage::Disk => {
                // Open a read-only copy of the local durability to replay from.
                let (history, _) = relational_db::local_durability(
                    replica_dir.commit_log(),
                    // No need to include a snapshot request channel here, 'cause we're only reading from this instance.
                    None,
                )
                .await?;
                let persistence = persistence.persistence(&database, replica_id).await?;
                let (db, clients) = RelationalDB::open(
                    &replica_dir,
                    database.database_identity,
                    database.owner_identity,
                    history,
                    Some(persistence),
                    Some(tx_metrics_queue),
                    page_pool.clone(),
                )
                // Make sure we log the source chain of the error
                // as a single line, with the help of `anyhow`.
                .map_err(anyhow::Error::from)
                .inspect_err(|e| {
                    tracing::error!(
                        database = %database.database_identity,
                        replica = replica_id,
                        "Failed to open database: {e:#}"
                    );
                })?;

                (db, clients)
            }
        };
        let (program, program_needs_init) = match db.program()? {
            // Launch module with program from existing database.
            Some(program) => (program, false),
            // Database is empty, load program from external storage and run
            // initialization.
            None => (load_program(program_storage, database.initial_program).await?, true),
        };

        let (program, launched) = launch_module(
            database,
            replica_id,
            program,
            on_panic,
            Arc::new(db),
            energy_monitor.clone(),
            replica_dir,
            runtimes.clone(),
            host_controller.db_cores.take(),
        )
        .await?;

        if program_needs_init {
            let call_result = launched.module_host.init_database(program).await?;
            if let Some(call_result) = call_result {
                Result::from(call_result)?;
            }
        } else {
            drop(program)
        }

        let LaunchedModule {
            replica_ctx,
            module_host,
            scheduler,
            scheduler_starter,
        } = launched;

        // Disconnect dangling clients.
        for (identity, connection_id) in connected_clients {
            module_host
                .call_identity_disconnected(identity, connection_id)
                .await
                .with_context(|| {
                    format!(
                        "Error calling disconnect for {} {} on {}",
                        identity, connection_id, replica_ctx.database_identity
                    )
                })?;
        }
        // We should have no clients left, but we do this just in case.
        // This should only matter if we crashed with something in st_client_credentials,
        // then restarted with an older version of the code that doesn't use st_client_credentials.
        // That case would cause some permanently dangling st_client_credentials.
        // Since we have no clients on startup, this should be safe to do regardless.
        module_host.clear_all_clients().await?;

        scheduler_starter.start(&module_host)?;
        let disk_metrics_recorder_task = tokio::spawn(metric_reporter(replica_ctx.clone())).abort_handle();

        Ok(Host {
            module: watch::Sender::new(module_host),
            replica_ctx,
            scheduler,
            disk_metrics_recorder_task,
            tx_metrics_recorder_task,
        })
    }

    /// Construct an in-memory instance of `database` running `program`,
    /// initialize it, then immediately destroy it.
    ///
    /// This is used during an initial, fresh publish operation
    /// in order to check the `program`'s validity as a module,
    /// since some validity checks we'd like to do (e.g. typechecking RLS filters)
    /// require a fully instantiated database.
    ///
    /// This is not necessary during hotswap publishes,
    /// as the automigration planner and executor accomplish the same validity checks.
    async fn try_init_in_memory_to_check(
        runtimes: &Arc<HostRuntimes>,
        page_pool: PagePool,
        database: Database,
        program: Program,
        executor: SingleCoreExecutor,
    ) -> anyhow::Result<Arc<ModuleInfo>> {
        // Even in-memory databases acquire a lockfile.
        // Grab a tempdir to put that lockfile in.
        let phony_replica_dir = TempDir::with_prefix("spacetimedb-publish-in-memory-check")
            .context("Error creating temporary directory to house temporary database during publish")?;

        // Leave the `TempDir` instance in place, so that its destructor will still run.
        let phony_replica_dir = ReplicaDir::from_path_unchecked(phony_replica_dir.path().to_owned());

        let (db, _connected_clients) = RelationalDB::open(
            &phony_replica_dir,
            database.database_identity,
            database.owner_identity,
            EmptyHistory::new(),
            None,
            None,
            page_pool,
        )?;

        let (program, launched) = launch_module(
            database,
            0,
            program,
            // No need to register a callback here:
            // proper publishes use it to unregister a panicked module,
            // but this module is not registered in the first place.
            || log::error!("launch_module on_panic called for temporary publish in-memory instance"),
            Arc::new(db),
            Arc::new(NullEnergyMonitor),
            phony_replica_dir,
            runtimes.clone(),
            executor,
        )
        .await?;

        let call_result = launched.module_host.init_database(program).await?;
        if let Some(call_result) = call_result {
            Result::from(call_result)?;
        }

        Ok(launched.module_host.info)
    }

    /// Attempt to replace this [`Host`]'s [`ModuleHost`] with a new one running
    /// the program `program_hash`.
    ///
    /// The associated [`ReplicaContext`] stays the same.
    ///
    /// Executes [`ModuleHost::update_database`] on the newly instantiated
    /// module, updating the database schema and invoking the `__update__`
    /// reducer if it is defined.
    /// If this succeeds, the current module is replaced with the new one,
    /// otherwise it stays the same.
    ///
    /// Either way, the [`UpdateDatabaseResult`] is returned.
    #[allow(clippy::too_many_arguments)]
    async fn update_module(
        &mut self,
        runtimes: Arc<HostRuntimes>,
        host_type: HostType,
        program: Program,
        policy: MigrationPolicy,
        energy_monitor: Arc<dyn EnergyMonitor>,
        on_panic: impl Fn() + Send + Sync + 'static,
        executor: SingleCoreExecutor,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        let replica_ctx = &self.replica_ctx;
        let (scheduler, scheduler_starter) = Scheduler::open(self.replica_ctx.relational_db.clone());

        let (program, module) = make_module_host(
            runtimes,
            host_type,
            replica_ctx.clone(),
            scheduler.clone(),
            program,
            energy_monitor,
            on_panic,
            executor,
        )
        .await?;

        // Get the old module info to diff against when building a migration plan.
        let old_module_info = self.module.borrow().info.clone();

        let update_result =
            update_module(&replica_ctx.relational_db, &module, program, old_module_info, policy).await?;

        // Only replace the module + scheduler if the update succeeded.
        // Otherwise, we want the database to continue running with the old state.
        match update_result {
            UpdateDatabaseResult::NoUpdateNeeded | UpdateDatabaseResult::UpdatePerformed => {
                self.scheduler = scheduler;
                scheduler_starter.start(&module)?;
                let old_module = self.module.send_replace(module);
                old_module.exit().await;
            }

            // In this case, we need to disconnect all clients connected to the old module
            UpdateDatabaseResult::UpdatePerformedWithClientDisconnect => {
                // Replace the module first, so that new clients get the new module.
                let old_watcher = std::mem::replace(&mut self.module, watch::Sender::new(module.clone()));

                // Disconnect all clients connected to the old module.
                let connected_clients = replica_ctx.relational_db.connected_clients()?;
                for (identity, connection_id) in connected_clients {
                    let client_actor_id = ClientActorId {
                        identity,
                        connection_id,
                        name: ClientName(0),
                    };
                    //NOTE: This will call disconnect reducer of the new module, not the old one.
                    //It makes sense, as relationaldb is already updated to the new module.
                    module.disconnect_client(client_actor_id).await;
                }

                self.scheduler = scheduler;
                scheduler_starter.start(&module)?;
                // exit the old module, drop the `old_watcher` afterwards,
                // which will signal websocket clients that the module is gone.
                let old_module = old_watcher.borrow().clone();
                old_module.exit().await;
            }
            _ => {}
        }

        Ok(update_result)
    }

    /// Generate a migration plan for the given `program`.
    async fn migrate_plan(
        &self,
        host_type: HostType,
        program: Program,
        style: PrettyPrintStyle,
    ) -> anyhow::Result<MigratePlanResult> {
        let old_module = self.module.borrow().info.clone();

        let module_def = extract_schema(program.bytes, host_type).await?;

        let res = match ponder_migrate(&old_module.module_def, &module_def) {
            Ok(plan) => MigratePlanResult::Success {
                old_module_hash: old_module.module_hash,
                new_module_hash: program.hash,
                breaks_client: plan.breaks_client(),
                plan: plan.pretty_print(style)?.into(),
            },
            Err(e) => MigratePlanResult::AutoMigrationError(e),
        };

        Ok(res)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.disk_metrics_recorder_task.abort();
        self.tx_metrics_recorder_task.abort();
    }
}

pub enum MigratePlanResult {
    Success {
        old_module_hash: Hash,
        new_module_hash: Hash,
        plan: Box<str>,
        breaks_client: bool,
    },
    AutoMigrationError(ErrorStream<AutoMigrateError>),
}

const STORAGE_METERING_INTERVAL: Duration = Duration::from_secs(15);

/// Periodically collect gauge stats and update prometheus metrics.
async fn metric_reporter(replica_ctx: Arc<ReplicaContext>) {
    // TODO: Consider adding a metric for heap usage.
    let message_log_size = DB_METRICS
        .message_log_size
        .with_label_values(&replica_ctx.database_identity);
    let module_log_file_size = DB_METRICS
        .module_log_file_size
        .with_label_values(&replica_ctx.database_identity);

    loop {
        let ctx = replica_ctx.clone();
        // We spawn a blocking task here because this grabs blocking locks.
        let disk_usage_future = tokio::task::spawn_blocking(move || {
            ctx.update_gauges();
            ctx.total_disk_usage()
        });
        if let Ok(disk_usage) = disk_usage_future.await {
            if let Some(num_bytes) = disk_usage.durability {
                message_log_size.set(num_bytes as i64);
            }
            if let Some(num_bytes) = disk_usage.logs {
                module_log_file_size.set(num_bytes as i64);
            }
        }
        tokio::time::sleep(STORAGE_METERING_INTERVAL).await;
    }
}

/// Extracts the schema from a given module.
///
/// Spins up a dummy host and returns the `ModuleDef` that it extracts.
pub async fn extract_schema(program_bytes: Box<[u8]>, host_type: HostType) -> anyhow::Result<ModuleDef> {
    let owner_identity = Identity::from_u256(0xdcba_u32.into());
    let database_identity = Identity::from_u256(0xabcd_u32.into());
    let program = Program::from_bytes(program_bytes);

    let database = Database {
        id: 0,
        database_identity,
        owner_identity,
        host_type,
        initial_program: program.hash,
    };

    let runtimes = HostRuntimes::new(None);
    let page_pool = PagePool::new(None);
    let core = SingleCoreExecutor::in_current_tokio_runtime();
    let module_info = Host::try_init_in_memory_to_check(&runtimes, page_pool, database, program, core).await?;
    // this should always succeed, but sometimes it doesn't
    let module_def = match Arc::try_unwrap(module_info) {
        Ok(info) => info.module_def,
        Err(info) => info.module_def.clone(),
    };

    Ok(module_def)
}

// Remove all gauges associated with a database.
// This is useful if a database is being deleted.
pub fn remove_database_gauges<'a, I>(db: &Identity, table_names: I)
where
    I: IntoIterator<Item = &'a str>,
{
    // Remove the per-table gauges.
    for table_name in table_names {
        let _ = DATA_SIZE_METRICS
            .data_size_table_num_rows
            .remove_label_values(db, table_name);
        let _ = DATA_SIZE_METRICS
            .data_size_table_bytes_used_by_rows
            .remove_label_values(db, table_name);
        let _ = DATA_SIZE_METRICS
            .data_size_table_num_rows_in_indexes
            .remove_label_values(db, table_name);
        let _ = DATA_SIZE_METRICS
            .data_size_table_bytes_used_by_index_keys
            .remove_label_values(db, table_name);
    }
    // Remove the per-db gauges.
    let _ = DATA_SIZE_METRICS.data_size_blob_store_num_blobs.remove_label_values(db);
    let _ = DATA_SIZE_METRICS
        .data_size_blob_store_bytes_used_by_blobs
        .remove_label_values(db);
    let _ = WORKER_METRICS.wasm_memory_bytes.remove_label_values(db);
}
