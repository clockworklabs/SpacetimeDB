use super::module_host::{EventStatus, ModuleHost, ModuleInfo, NoSuchModule};
use super::scheduler::SchedulerStarter;
use super::wasmtime::WasmtimeRuntime;
use super::{Scheduler, UpdateDatabaseResult};
use crate::database_logger::DatabaseLogger;
use crate::db::datastore::traits::Program;
use crate::db::db_metrics::DB_METRICS;
use crate::db::relational_db::{self, DiskSizeFn, RelationalDB, Txdata};
use crate::db::{self, db_metrics};
use crate::energy::{EnergyMonitor, EnergyQuanta, NullEnergyMonitor};
use crate::messages::control_db::{Database, HostType};
use crate::module_host_context::ModuleCreationContext;
use crate::replica_context::ReplicaContext;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use crate::util::{asyncify, spawn_rayon};
use anyhow::{anyhow, ensure, Context};
use async_trait::async_trait;
use durability::{Durability, EmptyHistory};
use log::{info, trace, warn};
use parking_lot::Mutex;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_durability::{self as durability, TxOffset};
use spacetimedb_lib::{hash_bytes, Identity};
use spacetimedb_paths::server::{ReplicaDir, ServerDataDir};
use spacetimedb_paths::FromPathUnchecked;
use spacetimedb_sats::hash::Hash;
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

pub type StartSnapshotWatcher = Box<dyn FnOnce(watch::Receiver<TxOffset>)>;

#[async_trait]
pub trait DurabilityProvider: Send + Sync + 'static {
    async fn durability(&self, replica_id: u64) -> anyhow::Result<(ExternalDurability, Option<StartSnapshotWatcher>)>;
}

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
    /// Provides implementations of [`Durability`] for each replica.
    durability: Arc<dyn DurabilityProvider>,
    /// The page pool all databases will use by cloning the ref counted pool.
    pub page_pool: PagePool,
    /// The runtimes for running our modules.
    runtimes: Arc<HostRuntimes>,
}

struct HostRuntimes {
    wasmtime: WasmtimeRuntime,
}

impl HostRuntimes {
    fn new(data_dir: Option<&ServerDataDir>) -> Arc<Self> {
        let wasmtime = WasmtimeRuntime::new(data_dir);
        Arc::new(Self { wasmtime })
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

impl HostController {
    pub fn new(
        data_dir: Arc<ServerDataDir>,
        default_config: db::Config,
        program_storage: ProgramStorage,
        energy_monitor: Arc<impl EnergyMonitor>,
        durability: Arc<dyn DurabilityProvider>,
    ) -> Self {
        Self {
            hosts: <_>::default(),
            default_config,
            program_storage,
            energy_monitor,
            durability,
            runtimes: HostRuntimes::new(Some(&data_dir)),
            data_dir,
            page_pool: PagePool::new(default_config.page_pool_max_size),
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
        let host = self.try_init_host(database, replica_id).await?;

        let rx = host.module.subscribe();
        *guard = Some(host);

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
        Host::try_init_in_memory_to_check(&self.runtimes, self.page_pool.clone(), database, program).await
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
        let result = asyncify(move || f(&module.replica_ctx().relational_db)).await;
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
        let mut host = match guard.take() {
            None => {
                trace!("host not running, try_init");
                self.try_init_host(database, replica_id).await?
            }
            Some(host) => {
                trace!("host found, updating");
                host
            }
        };
        let update_result = host
            .update_module(
                self.runtimes.clone(),
                host_type,
                program,
                self.energy_monitor.clone(),
                self.unregister_fn(replica_id),
            )
            .await?;

        *guard = Some(host);
        Ok(update_result)
    }

    /// Start the host `replica_id` and conditionally update it.
    ///
    /// If the host was not initialized before, it is initialized with the
    /// program [`Database::initial_program`], which is loaded from the
    /// controller's [`ProgramStorage`].
    ///
    /// If it was already initialized and its stored program hash matches
    /// [`Database::initial_program`], no further action is taken.
    ///
    /// Otherwise, if `expected_hash` is `Some` and does **not** match the
    /// stored hash, an error is returned.
    ///
    /// Otherwise, the host is updated to [`Database::initial_program`], loading
    /// the program data from the controller's [`ProgramStorage`].
    ///
    /// > Note that this ascribes different semantics to [`Database::initial_program`]
    /// > than elsewhere, where the [`Database`] value is provided by the control
    /// > database. The method is mainly useful for bootstrapping the control
    /// > database itself.
    pub async fn init_maybe_update_module_host(
        &self,
        database: Database,
        replica_id: u64,
        expected_hash: Option<Hash>,
    ) -> anyhow::Result<watch::Receiver<ModuleHost>> {
        trace!("custom bootstrap {}/{}", database.database_identity, replica_id);

        let db_addr = database.database_identity;
        let host_type = database.host_type;
        let program_hash = database.initial_program;

        let mut guard = self.acquire_write_lock(replica_id).await;
        let mut host = match guard.take() {
            Some(host) => host,
            None => self.try_init_host(database, replica_id).await?,
        };
        let module = host.module.subscribe();

        // The program is now either:
        //
        // - the desired one from [Database], in which case we do nothing
        // - `Some` expected hash, in which case we update to the desired one
        // - `None` expected hash, in which case we also update
        let stored_hash = stored_program_hash(host.db())?
            .with_context(|| format!("[{}] database improperly initialized", db_addr))?;
        if stored_hash == program_hash {
            info!("[{}] database up-to-date with {}", db_addr, program_hash);
            *guard = Some(host);
        } else {
            if let Some(expected_hash) = expected_hash {
                ensure!(
                    expected_hash == stored_hash,
                    "[{}] expected program {} found {}",
                    db_addr,
                    expected_hash,
                    stored_hash
                );
            }
            info!(
                "[{}] updating database from `{}` to `{}`",
                db_addr, stored_hash, program_hash
            );
            let program = load_program(&self.program_storage, program_hash).await?;
            let update_result = host
                .update_module(
                    self.runtimes.clone(),
                    host_type,
                    program,
                    self.energy_monitor.clone(),
                    self.unregister_fn(replica_id),
                )
                .await?;
            match update_result {
                UpdateDatabaseResult::NoUpdateNeeded | UpdateDatabaseResult::UpdatePerformed => {
                    *guard = Some(host);
                }
                UpdateDatabaseResult::AutoMigrateError(e) => {
                    return Err(anyhow::anyhow!(e));
                }
                UpdateDatabaseResult::ErrorExecutingMigration(e) => {
                    return Err(e);
                }
            }
        }

        Ok(module)
    }

    /// Release all resources of the [`ModuleHost`] identified by `replica_id`,
    /// and deregister it from the controller.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn exit_module_host(&self, replica_id: u64) -> Result<(), anyhow::Error> {
        trace!("exit module host {}", replica_id);
        let lock = self.hosts.lock().remove(&replica_id);
        if let Some(lock) = lock {
            if let Some(host) = lock.write_owned().await.take() {
                let module = host.module.borrow().clone();
                module.exit().await;
                let table_names = module.info().module_def.tables().map(|t| t.name.deref());
                db_metrics::data_size::remove_database_gauges(&module.info().database_identity, table_names);
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
        trace!("get module host {}", replica_id);
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
        trace!("watch module host {}", replica_id);
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
        Host::try_init(self, database, replica_id).await
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
    let subscriptions = <_>::default();
    let downgraded = Arc::downgrade(&subscriptions);
    let subscriptions = ModuleSubscriptions::new(relational_db.clone(), subscriptions, database.owner_identity);

    // If an error occurs when evaluating a subscription,
    // we mark each client that was affected,
    // and we remove those clients from the manager async.
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let Some(subscriptions) = downgraded.upgrade() else {
                break;
            };
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
async fn make_module_host(
    runtimes: Arc<HostRuntimes>,
    host_type: HostType,
    replica_ctx: Arc<ReplicaContext>,
    scheduler: Scheduler,
    program: Program,
    energy_monitor: Arc<dyn EnergyMonitor>,
    unregister: impl Fn() + Send + Sync + 'static,
) -> anyhow::Result<(Program, ModuleHost)> {
    spawn_rayon(move || {
        let module_host = match host_type {
            HostType::Wasm => {
                let mcc = ModuleCreationContext {
                    replica_ctx,
                    scheduler,
                    program: &program,
                    energy_monitor,
                };
                let start = Instant::now();
                let actor = runtimes.wasmtime.make_actor(mcc)?;
                trace!("wasmtime::make_actor blocked for {:?}", start.elapsed());
                ModuleHost::new(actor, unregister)
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
        .with_context(|| format!("program {} not found", hash))?;
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
) -> anyhow::Result<UpdateDatabaseResult> {
    let addr = db.database_identity();
    match stored_program_hash(db)? {
        None => Err(anyhow!("database `{}` not yet initialized", addr)),
        Some(stored) => {
            let res = if stored == program.hash {
                info!("database `{}` up to date with program `{}`", addr, program.hash);
                UpdateDatabaseResult::NoUpdateNeeded
            } else {
                info!("updating `{}` from {} to {}", addr, stored, program.hash);
                module.update_database(program, old_module_info).await?
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
    metrics_task: AbortHandle,
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
            durability,
            page_pool,
            ..
        } = host_controller;
        let on_panic = host_controller.unregister_fn(replica_id);
        let replica_dir = data_dir.replica(replica_id);

        let (db, connected_clients) = match config.storage {
            db::Storage::Memory => RelationalDB::open(
                &replica_dir,
                database.database_identity,
                database.owner_identity,
                EmptyHistory::new(),
                None,
                None,
                page_pool.clone(),
            )?,
            db::Storage::Disk => {
                let snapshot_repo =
                    relational_db::open_snapshot_repo(replica_dir.snapshots(), database.database_identity, replica_id)?;
                let (history, _) = relational_db::local_durability(replica_dir.commit_log()).await?;
                let (durability, start_snapshot_watcher) = durability.durability(replica_id).await?;

                let (db, clients) = RelationalDB::open(
                    &replica_dir,
                    database.database_identity,
                    database.owner_identity,
                    history,
                    Some(durability),
                    Some(snapshot_repo),
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
                if let Some(start_snapshot_watcher) = start_snapshot_watcher {
                    let watcher = db.subscribe_to_snapshots().expect("we passed snapshot_repo");
                    start_snapshot_watcher(watcher)
                }
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

        scheduler_starter.start(&module_host)?;
        let metrics_task = tokio::spawn(metric_reporter(replica_ctx.clone())).abort_handle();

        Ok(Host {
            module: watch::Sender::new(module_host),
            replica_ctx,
            scheduler,
            metrics_task,
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
    async fn update_module(
        &mut self,
        runtimes: Arc<HostRuntimes>,
        host_type: HostType,
        program: Program,
        energy_monitor: Arc<dyn EnergyMonitor>,
        on_panic: impl Fn() + Send + Sync + 'static,
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
        )
        .await?;

        // Get the old module info to diff against when building a migration plan.
        let old_module_info = self.module.borrow().info.clone();

        let update_result = update_module(&replica_ctx.relational_db, &module, program, old_module_info).await?;
        trace!("update result: {update_result:?}");
        // Only replace the module + scheduler if the update succeeded.
        // Otherwise, we want the database to continue running with the old state.
        if update_result.was_successful() {
            self.scheduler = scheduler;
            scheduler_starter.start(&module)?;
            let old_module = self.module.send_replace(module);
            old_module.exit().await;
        }

        Ok(update_result)
    }

    fn db(&self) -> &RelationalDB {
        &self.replica_ctx.relational_db
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.metrics_task.abort();
    }
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
        let disk_usage = tokio::task::block_in_place(|| replica_ctx.total_disk_usage());
        replica_ctx.update_gauges();
        if let Some(num_bytes) = disk_usage.durability {
            message_log_size.set(num_bytes as i64);
        }
        if let Some(num_bytes) = disk_usage.logs {
            module_log_file_size.set(num_bytes as i64);
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
    let module_info = Host::try_init_in_memory_to_check(&runtimes, page_pool, database, program).await?;
    let module_info = Arc::into_inner(module_info).unwrap();

    Ok(module_info.module_def)
}
