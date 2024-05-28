use super::module_host::{EntityDef, EventStatus, ModuleHost, NoSuchModule, UpdateDatabaseResult};
use super::scheduler::SchedulerStarter;
use super::Scheduler;
use crate::database_instance_context::DatabaseInstanceContext;
use crate::database_logger::DatabaseLogger;
use crate::db::datastore::traits::Metadata;
use crate::db::db_metrics::DB_METRICS;
use crate::db::relational_db::RelationalDB;
use crate::energy::{EnergyMonitor, EnergyQuanta};
use crate::messages::control_db::{Database, HostType};
use crate::module_host_context::ModuleCreationContext;
use crate::subscription::module_subscription_actor::ModuleSubscriptions;
use crate::util::spawn_rayon;
use crate::{db, host};
use anyhow::{anyhow, bail, ensure, Context as _};
use async_trait::async_trait;
use log::{debug, info, trace, warn};
use parking_lot::Mutex;
use serde::Serialize;
use spacetimedb_commitlog as commitlog;
use spacetimedb_data_structures::map::IntMap;
use spacetimedb_durability as durability;
use spacetimedb_lib::{hash_bytes, ProductValue};
use spacetimedb_sats::hash::Hash;
use std::fmt;
use std::future::Future;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{watch, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock as AsyncRwLock};
use tokio::task::AbortHandle;

// TODO:
//
// - [db::Config] should be per-[Database]
// - init / update to take program bytes, and store them in the db
// - get / spawn to load program from db
//
// - Do we need to distinguish between init and update?
//
//   `custom_bootstrap` suggests we don't.
//
// - Ordering:
//
//   The fencing token could be made obsolete if the expected previous
//   program hash is known. That, however, is not so easy in distributed
//   spacetimedb, because a disconnect will make a node miss history events.
//

/// A shared mutable cell containing a module host and associated database.
type HostCell = Arc<AsyncRwLock<Option<Host>>>;

/// The registry of all running hosts.
type Hosts = Arc<Mutex<IntMap<u64, HostCell>>>;

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
    /// The directory to create database instances in.
    ///
    /// For example:
    ///
    /// - `$STDB_PATH/worker_node/database_instances`
    /// - `$STDB_PATH/database_instances`
    root_dir: Arc<Path>,
    /// The default configuration to use for databases created by this
    /// controller.
    default_config: db::Config,
    /// The [`ProgramStorage`] to query when instantiating a module.
    program_storage: ProgramStorage,
    /// The [`EnergyMonitor`] used by this controller.
    energy_monitor: Arc<dyn EnergyMonitor>,
}

#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Debug)]
pub enum DescribedEntityType {
    Table,
    Reducer,
}

impl DescribedEntityType {
    pub fn as_str(self) -> &'static str {
        match self {
            DescribedEntityType::Table => "table",
            DescribedEntityType::Reducer => "reducer",
        }
    }
    pub fn from_entitydef(def: &EntityDef) -> Self {
        match def {
            EntityDef::Table(_) => Self::Table,
            EntityDef::Reducer(_) => Self::Reducer,
        }
    }
}
impl std::str::FromStr for DescribedEntityType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "table" => Ok(DescribedEntityType::Table),
            "reducer" => Ok(DescribedEntityType::Reducer),
            _ => Err(()),
        }
    }
}
impl fmt::Display for DescribedEntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
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
        root_dir: Arc<Path>,
        default_config: db::Config,
        program_storage: ProgramStorage,
        energy_monitor: Arc<impl EnergyMonitor>,
    ) -> Self {
        Self {
            hosts: <_>::default(),
            root_dir,
            default_config,
            program_storage,
            energy_monitor,
        }
    }

    /// Replace the [`ProgramStorage`] used by this controller.
    pub fn set_program_storage(&mut self, ps: ProgramStorage) {
        self.program_storage = ps;
    }

    /// Get a [`ModuleHost`] managed by this controller, or launch it from
    /// persistent state.
    ///
    /// An error is returned if the host's program does not match the hash given
    /// in [`Database`].
    ///
    /// See also: [`Self::get_module_host`]
    #[tracing::instrument(skip_all)]
    pub async fn get_or_launch_module_host(&self, database: Database, instance_id: u64) -> anyhow::Result<ModuleHost> {
        let mut rx = self.watch_maybe_launch_module_host(database, instance_id).await?;
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
    #[tracing::instrument(skip_all)]
    pub async fn watch_maybe_launch_module_host(
        &self,
        database: Database,
        instance_id: u64,
    ) -> anyhow::Result<watch::Receiver<ModuleHost>> {
        // Try a read lock first.
        {
            let guard = self.acquire_read_lock(instance_id).await;
            if let Some(host) = &*guard {
                trace!("cached host {}/{}", database.address, instance_id);
                return Ok(host.module.subscribe());
            }
        }

        // We didn't find a running module, so take a write lock.
        // Since [`tokio::sync::RwLock`] doesn't support upgrading of read locks,
        // we'll need to check again if a module was added meanwhile.
        let mut guard = self.acquire_write_lock(instance_id).await;
        if let Some(host) = &*guard {
            trace!("cached host {}/{} (lock upgrade)", database.address, instance_id);
            return Ok(host.module.subscribe());
        }

        trace!("launch host {}/{}", database.address, instance_id);
        let host = self.try_init_host(database, instance_id).await?;

        let rx = host.module.subscribe();
        *guard = Some(host);

        Ok(rx)
    }

    /// Run a computation on the [`RelationalDB`] of a [`ModuleHost`] managed by
    /// this controller, launching the host if necessary.
    ///
    /// An error is returned if the host's program does not match the hash given
    /// in [`Database`].
    ///
    /// If the computation `F` panics, the host is removed from this controller,
    /// releasing its resources.
    #[tracing::instrument(skip_all)]
    pub async fn using_database<F, T>(&self, database: Database, instance_id: u64, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&RelationalDB) -> T + Send + 'static,
        T: Send + 'static,
    {
        trace!("using database {}/{}", database.address, instance_id);
        let module = self.get_or_launch_module_host(database, instance_id).await?;
        let on_panic = self.unregister_fn(instance_id);
        let result = tokio::task::spawn_blocking(move || f(&module.dbic().relational_db))
            .await
            .unwrap_or_else(|e| {
                warn!("database operation panicked");
                on_panic();
                std::panic::resume_unwind(e.into_panic())
            });
        Ok(result)
    }

    /// Update the [`ModuleHost`] identified by `instance_id` to the given
    /// program.
    ///
    /// The host may not be running, in which case it is spawned.
    /// If the host was running, and the update fails, the previous version of
    /// the host keeps running.
    #[tracing::instrument(skip_all)]
    pub async fn update_module_host(
        &self,
        database: Database,
        instance_id: u64,
        program_bytes: Box<[u8]>,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        trace!("update module host {}/{}", database.address, instance_id);

        let program_hash = database.program_bytes_address;
        ensure!(
            hash_bytes(&program_bytes) == program_hash,
            "supplied program does not match hash"
        );

        let mut guard = self.acquire_write_lock(instance_id).await;
        let (update_result, maybe_updated_host) = match guard.take() {
            // If we don't have a running `Host`, spawn one.
            None => {
                let host = self.try_init_host(database, instance_id).await?;
                let module = host.module.borrow().clone();
                let update_result = update_module(host.db(), &module, (program_hash, program_bytes)).await?;
                // If the update was not successul, drop the host.
                // The `database` we gave it refers to a program hash which
                // doesn't exist (because we just rejected it).
                let maybe_updated_host = update_result.is_ok().then_some(host);

                (update_result, maybe_updated_host)
            }

            // Otherwise, update the host.
            // Note that we always put back the host -- if the update failed, it
            // will keep running the previous version of the module.
            Some(mut host) => {
                match host.dbic.relational_db.metadata()? {
                    None => bail!("Host improperly initialized: no metadata"),
                    Some(Metadata {
                        database_address,
                        owner_identity,
                        ..
                    }) => {
                        ensure!(
                            database_address == database.address,
                            "cannot change database address when updating module host"
                        );
                        ensure!(
                            owner_identity == database.identity,
                            "cannot (yet) change owner identity when updating module host"
                        );
                    }
                }
                let update_result = host
                    .update_module(database.host_type, program_hash, self.unregister_fn(instance_id))
                    .await?;

                (update_result, Some(host))
            }
        };

        *guard = maybe_updated_host;
        Ok(update_result)
    }

    // Accomodates control db bootstrap, which we hope to unify with regular
    // bootstrap in the future.
    #[doc(hidden)]
    pub async fn custom_bootstrap<F, G, T>(
        &self,
        expected_hash: Option<Hash>,
        database: Database,
        instance_id: u64,
        post_boot: F,
    ) -> anyhow::Result<T>
    where
        F: FnOnce(&RelationalDB) -> anyhow::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        trace!("custom bootstrap {}/{}", database.address, instance_id);

        let db_addr = database.address;
        let program_hash = database.program_bytes_address;

        let mut guard = self.acquire_write_lock(instance_id).await;
        let host = match guard.take() {
            Some(host) => host,
            None => self.try_init_host(database, instance_id).await?,
        };
        let module = host.module.clone();

        match (stored_program_hash(host.db())?, expected_hash) {
            (Some(stored_hash), _) if stored_hash == program_hash => {
                info!("[{}] database up to date with program `{}`", db_addr, program_hash);
                Ok(())
            }
            (Some(stored_hash), Some(expected_hash)) if stored_hash != expected_hash => Err(anyhow!(
                "[{}] expected program `{}` found `{}`",
                db_addr,
                expected_hash,
                stored_hash
            )),
            (Some(stored_hash), None) => Err(anyhow!(
                "[{}] expected uninitialized database found program `{}`",
                db_addr,
                stored_hash
            )),
            (None, Some(expected_hash)) => {
                Err(anyhow!("[{}] expected program `{}` found none", db_addr, expected_hash))
            }

            (None, None) => {
                info!("[{}] initializing database with program `{}`", db_addr, program_hash);
                // TODO: nonsensical
                Ok(())
            }

            (Some(stored_hash), Some(_)) => {
                info!(
                    "[{}] updating database from `{}` to `{}`",
                    db_addr, stored_hash, program_hash
                );
                let program_bytes = load_program(&self.program_storage, program_hash).await?;
                let update_result = host
                    .module
                    .borrow()
                    .update_database(program_hash, program_bytes.as_ref().into())
                    .await?;
                if update_result.is_ok() {
                    *guard = Some(host);
                }
                update_result.map(drop).map_err(Into::into)
            }
        }?;

        let on_panic = self.unregister_fn(instance_id);
        tokio::task::spawn_blocking(move || post_boot(&module.borrow().dbic().relational_db))
            .await
            .unwrap_or_else(|e| {
                warn!("post-boot database operation panicked");
                on_panic();
                std::panic::resume_unwind(e.into_panic())
            })
            .map_err(Into::into)
    }

    /// Release all resources of the [`ModuleHost`] identified by `instance_id`,
    /// and deregister it from the controller.
    #[tracing::instrument(skip_all)]
    pub async fn exit_module_host(&self, instance_id: u64) -> Result<(), anyhow::Error> {
        trace!("exit module host {}", instance_id);
        let lock = self.hosts.lock().remove(&instance_id);
        if let Some(lock) = lock {
            if let Some(host) = lock.write_owned().await.take() {
                let module = host.module.borrow().clone();
                module.exit().await;
                host.scheduler.clear();
            }
        }

        Ok(())
    }

    /// Get the [`ModuleHost`] identified by `instance_id` or return an error
    /// if it is not registered with the controller.
    ///
    /// See [`Self::get_or_launch_module_host`] for a variant which launches
    /// the host if it is not running.
    #[tracing::instrument(skip_all)]
    pub async fn get_module_host(&self, instance_id: u64) -> Result<ModuleHost, NoSuchModule> {
        trace!("get module host {}", instance_id);
        let guard = self.acquire_read_lock(instance_id).await;
        guard
            .as_ref()
            .map(|Host { module, .. }| module.borrow().clone())
            .ok_or(NoSuchModule)
    }

    /// Subscribe to updates of the [`ModuleHost`] identified by `instance_id`,
    /// or return an error if it is not registered with the controller.
    ///
    /// See [`Self::watch_maybe_launch_module_host`] for a variant which
    /// launches the host if it is not running.
    #[tracing::instrument(skip_all)]
    pub async fn watch_module_host(&self, instance_id: u64) -> Result<watch::Receiver<ModuleHost>, NoSuchModule> {
        trace!("watch module host {}", instance_id);
        let guard = self.acquire_read_lock(instance_id).await;
        guard
            .as_ref()
            .map(|Host { module, .. }| module.subscribe())
            .ok_or(NoSuchModule)
    }

    /// `true` if the module host `instance_id` is currently registered with
    /// the controller.
    pub async fn has_module_host(&self, instance_id: u64) -> bool {
        self.acquire_read_lock(instance_id).await.is_some()
    }

    /// On-panic callback passed to [`ModuleHost`]s created by this controller.
    ///
    /// Removes the module with the given `instance_id` from this controller.
    fn unregister_fn(&self, instance_id: u64) -> impl Fn() + Send + Sync + 'static {
        let hosts = Arc::downgrade(&self.hosts);
        move || {
            if let Some(hosts) = hosts.upgrade() {
                hosts.lock().remove(&instance_id);
            }
        }
    }

    async fn acquire_write_lock(&self, instance_id: u64) -> OwnedRwLockWriteGuard<Option<Host>> {
        let lock = self.hosts.lock().entry(instance_id).or_default().clone();
        lock.write_owned().await
    }

    async fn acquire_read_lock(&self, instance_id: u64) -> OwnedRwLockReadGuard<Option<Host>> {
        let lock = self.hosts.lock().entry(instance_id).or_default().clone();
        lock.read_owned().await
    }

    async fn try_init_host(&self, database: Database, instance_id: u64) -> anyhow::Result<Host> {
        Host::try_init(
            &self.root_dir,
            self.default_config,
            database,
            instance_id,
            self.program_storage.clone(),
            self.energy_monitor.clone(),
            self.unregister_fn(instance_id),
        )
        .await
    }
}

fn stored_program_hash(db: &RelationalDB) -> anyhow::Result<Option<Hash>> {
    let meta = db.metadata()?;
    Ok(meta.map(|meta| meta.program_hash))
}

async fn make_dbic(
    database: Database,
    instance_id: u64,
    relational_db: Arc<RelationalDB>,
) -> anyhow::Result<DatabaseInstanceContext> {
    let log_path = DatabaseLogger::filepath(&database.address, instance_id);
    let logger = tokio::task::block_in_place(|| Arc::new(DatabaseLogger::open(log_path)));
    let subscriptions = ModuleSubscriptions::new(relational_db.clone(), database.identity);

    Ok(DatabaseInstanceContext {
        database,
        database_instance_id: instance_id,
        logger,
        relational_db,
        subscriptions,
    })
}

async fn make_module_host(
    host_type: HostType,
    mcc: ModuleCreationContext,
    unregister: impl Fn() + Send + Sync + 'static,
) -> anyhow::Result<ModuleHost> {
    spawn_rayon(move || {
        let module_host = match host_type {
            HostType::Wasm => {
                let start = Instant::now();
                let actor = host::wasmtime::make_actor(mcc)?;
                trace!("wasmtime::make_actor blocked for {:?}", start.elapsed());
                ModuleHost::new(actor, unregister)
            }
        };
        Ok(module_host)
    })
    .await
}

async fn make_local_durability(root_dir: &Path) -> io::Result<durability::Local<ProductValue>> {
    let commitlog_dir = root_dir.join("clog");
    tokio::fs::create_dir_all(&commitlog_dir).await?;
    let rt = tokio::runtime::Handle::current();
    // TODO: Should this better be spawn_blocking?
    spawn_rayon(move || {
        durability::Local::open(
            commitlog_dir,
            rt,
            durability::local::Options {
                commitlog: commitlog::Options {
                    max_records_in_commit: 1.try_into().unwrap(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
    })
    .await
}

async fn load_program(storage: &ProgramStorage, hash: Hash) -> anyhow::Result<Box<[u8]>> {
    debug!("lookup program {}", hash);
    storage
        .lookup(hash)
        .await?
        .with_context(|| format!("program {} not found", hash))
}

async fn launch_module(
    root_dir: &Path,
    database: Database,
    instance_id: u64,
    program_bytes: Box<[u8]>,
    on_panic: impl Fn() + Send + Sync + 'static,
    relational_db: Arc<RelationalDB>,
    energy_monitor: Arc<dyn EnergyMonitor>,
) -> anyhow::Result<(Arc<DatabaseInstanceContext>, ModuleHost, Scheduler, SchedulerStarter)> {
    let program_hash = database.program_bytes_address;
    let host_type = database.host_type;

    let dbic = make_dbic(database, instance_id, relational_db).await.map(Arc::new)?;
    let (scheduler, scheduler_starter) = Scheduler::open(dbic.scheduler_db_path(root_dir.to_path_buf()))?;
    let module_host = make_module_host(
        host_type,
        ModuleCreationContext {
            dbic: dbic.clone(),
            scheduler: scheduler.clone(),
            program_hash,
            program_bytes: program_bytes.into(),
            energy_monitor: energy_monitor.clone(),
        },
        on_panic,
    )
    .await?;

    debug!("launch done");

    Ok((dbic, module_host, scheduler, scheduler_starter))
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
    (program_hash, program_bytes): (Hash, Box<[u8]>),
) -> anyhow::Result<UpdateDatabaseResult> {
    let addr = db.address();
    match stored_program_hash(db)? {
        None => Err(anyhow!("database `{}` not yet initialized", addr)),
        Some(stored) if stored == program_hash => {
            info!("database `{}` up to date with program `{}`", addr, program_hash);
            anyhow::Ok(Ok(<_>::default()))
        }
        Some(stored) => {
            info!("updating `{}` from {} to {}", addr, stored, program_hash);
            let update_result = module.update_database(program_hash, program_bytes).await?;
            Ok(update_result)
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
    /// Pointer to the `module`'s [`DatabaseInstanceContext`].
    ///
    /// The database stays the same if and when the module is updated via
    /// [`Host::update_module`].
    dbic: Arc<DatabaseInstanceContext>,
    /// Scheduler for repeating reducers, operating on the current `module`.
    scheduler: Scheduler,
    /// Handle to the metrics collection task started via [`disk_monitor`].
    ///
    /// The task collects metrics from the `dbic`, and so stays alive as long
    /// as the `dbic` is live. The task is aborted when [`Host`] is dropped.
    metrics_task: AbortHandle,

    /// [`ProgramStorage`] to use for [`Host::update_module`].
    program_storage: ProgramStorage,
    /// [`EnergyMonitor`] to use for [`Host::update_module`].
    energy_monitor: Arc<dyn EnergyMonitor>,
}

impl Host {
    /// Attempt to instantiate a [`Host`] from persistent storage.
    ///
    /// Note that this does **not** run module initialization routines, but may
    /// create on-disk artifacts if the host / database did not exist.
    async fn try_init(
        root_dir: &Path,
        config: db::Config,
        database: Database,
        instance_id: u64,
        program_storage: ProgramStorage,
        energy_monitor: Arc<dyn EnergyMonitor>,
        on_panic: impl Fn() + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        let mut db_path = root_dir.to_path_buf();
        db_path.extend([&*database.address.to_hex(), &*instance_id.to_string()]);
        db_path.push("database");

        let durability = make_local_durability(&db_path).await.map(Arc::new)?;
        let (db, connected_clients) = RelationalDB::open(
            &db_path,
            database.address,
            database.identity,
            durability.clone(),
            match config.storage {
                db::Storage::Memory => None,
                db::Storage::Disk => {
                    let disk_size_fn = Arc::new({
                        let durability = durability.clone();
                        move || durability.size_on_disk()
                    });
                    Some((durability, disk_size_fn))
                }
            },
        )?;
        let (dbic, module_host, scheduler, scheduler_starter) = match db.program_bytes()? {
            // Launch module with program from existing database.
            Some(program_bytes) if !program_bytes.is_empty() => {
                launch_module(
                    root_dir,
                    database,
                    instance_id,
                    program_bytes,
                    on_panic,
                    Arc::new(db),
                    energy_monitor.clone(),
                )
                .await?
            }

            // Database is empty, load program from external storage and run
            // initialization.
            None | Some(_) => {
                let program_hash = database.program_bytes_address;
                let program_bytes = load_program(&program_storage, program_hash).await?;
                let res = launch_module(
                    root_dir,
                    database,
                    instance_id,
                    program_bytes.clone(),
                    on_panic,
                    Arc::new(db),
                    energy_monitor.clone(),
                )
                .await?;

                let module_host = &res.1;
                let call_result = module_host.init_database(program_hash, program_bytes).await?;
                if let Some(call_result) = call_result {
                    Result::from(call_result)?;
                }

                res
            }
        };

        for (identity, address) in connected_clients {
            module_host
                .call_identity_connected_disconnected(identity, address, false)
                .await
                .with_context(|| {
                    format!(
                        "Error calling disconnect for {} {} on {}",
                        identity, address, dbic.address
                    )
                })?;
        }

        scheduler_starter.start(&module_host)?;
        let metrics_task = tokio::spawn(disk_monitor(dbic.clone(), energy_monitor.clone())).abort_handle();

        Ok(Host {
            module: watch::Sender::new(module_host),
            dbic,
            scheduler,
            metrics_task,

            program_storage,
            energy_monitor,
        })
    }

    /// Attempt to replace this [`Host`]'s [`ModuleHost`] with a new one running
    /// the program `program_hash`.
    ///
    /// The associated [`DatabaseInstanceContext`] stays the same.
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
        host_type: HostType,
        program_hash: Hash,
        on_panic: impl Fn() + Send + Sync + 'static,
    ) -> anyhow::Result<UpdateDatabaseResult> {
        let dbic = &self.dbic;
        let (scheduler, scheduler_starter) = self.scheduler.new_with_same_db();
        let program_bytes = load_program(&self.program_storage, program_hash).await?;
        let module = make_module_host(
            host_type,
            ModuleCreationContext {
                dbic: dbic.clone(),
                scheduler: scheduler.clone(),
                program_bytes: program_bytes.clone().into(),
                program_hash,
                energy_monitor: self.energy_monitor.clone(),
            },
            on_panic,
        )
        .await?;

        let update_result = update_module(&dbic.relational_db, &module, (program_hash, program_bytes)).await?;
        debug!("update result: {update_result:?}");
        if update_result.is_ok() {
            scheduler_starter.start(&module)?;
            let old_module = self.module.send_replace(module);
            old_module.exit().await;
        }

        Ok(update_result)
    }

    fn db(&self) -> &RelationalDB {
        &self.dbic.relational_db
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.metrics_task.abort();
    }
}

const DISK_METERING_INTERVAL: Duration = Duration::from_secs(5);

/// Periodically collect the disk usage of `dbic` and update metrics as well as
/// the `energy_monitor` accordingly.
async fn disk_monitor(dbic: Arc<DatabaseInstanceContext>, energy_monitor: Arc<dyn EnergyMonitor>) {
    let mut interval = tokio::time::interval(DISK_METERING_INTERVAL);
    // We don't care about happening precisely every 5 seconds - it just matters
    // that the time between ticks is accurate.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut prev_disk_usage = dbic.total_disk_usage();
    let mut prev_tick = interval.tick().await;
    loop {
        let tick = interval.tick().await;
        let dt = tick - prev_tick;
        let disk_usage = tokio::task::block_in_place(|| dbic.total_disk_usage());
        if let Some(num_bytes) = disk_usage.durability {
            DB_METRICS
                .message_log_size
                .with_label_values(&dbic.address)
                .set(num_bytes as i64);
        }
        if let Some(num_bytes) = disk_usage.logs {
            DB_METRICS
                .module_log_file_size
                .with_label_values(&dbic.address)
                .set(num_bytes as i64);
        }
        let disk_usage = disk_usage.or(prev_disk_usage);
        energy_monitor.record_disk_usage(&dbic.database, dbic.database_instance_id, disk_usage.sum(), dt);
        prev_disk_usage = disk_usage;
        prev_tick = tick;
    }
}
