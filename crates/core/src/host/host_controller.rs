use crate::hash::hash_bytes;
use crate::host;
use crate::messages::control_db::HostType;
use crate::module_host_context::ModuleHostContext;
use anyhow::Context;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::ops::Sub;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::module_host::{Catalog, EntityDef, EventStatus, ModuleHost, NoSuchModule, UpdateDatabaseResult};
use super::scheduler::SchedulerStarter;
use super::{EnergyMonitor, NullEnergyMonitor, ReducerArgs};

pub struct HostController {
    modules: Mutex<HashMap<u64, ModuleHost>>,
    threadpool: Arc<HostThreadpool>,
    pub energy_monitor: Arc<dyn EnergyMonitor>,
}

pub struct HostThreadpool {
    inner: rayon_core::ThreadPool,
}

impl HostThreadpool {
    fn new() -> Self {
        let rt = tokio::runtime::Handle::current();
        let inner = rayon_core::ThreadPoolBuilder::new()
            .num_threads(std::thread::available_parallelism().unwrap().get() * 2)
            .spawn_handler(move |thread| {
                rt.spawn_blocking(|| thread.run());
                Ok(())
            })
            .build()
            .unwrap();
        Self { inner }
    }

    pub fn spawn(&self, f: impl FnOnce() + Send + 'static) {
        self.inner.spawn(f)
    }
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

/// [EnergyQuanta] represents an amount of energy in a canonical unit.
/// It represents the smallest unit of energy that can be used to pay for
/// a reducer invocation. We will likely refer to this unit as an "eV".
///
/// NOTE: This is represented by a signed integer, because it is possible
/// for a user's balance to go negative. This is allowable
/// for reasons of eventual consistency motivated by performance.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnergyQuanta(i128);

impl EnergyQuanta {
    pub const ZERO: Self = EnergyQuanta(0);

    pub const DEFAULT_BUDGET: Self = EnergyQuanta(1_000_000_000_000_000_000);

    #[inline]
    pub fn new(v: i128) -> Self {
        Self(v)
    }

    #[inline]
    pub fn get(&self) -> i128 {
        self.0
    }
}

#[derive(Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnergyDiff(pub i128);

impl EnergyDiff {
    pub const ZERO: Self = EnergyDiff(0);

    pub fn as_quanta(self) -> EnergyQuanta {
        EnergyQuanta(self.0)
    }
}

impl Sub for EnergyQuanta {
    type Output = EnergyDiff;

    fn sub(self, rhs: Self) -> Self::Output {
        EnergyDiff(self.0 - rhs.0)
    }
}

impl fmt::Debug for EnergyDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)?;
        f.write_str("eV")
    }
}

#[derive(Clone, Debug)]
pub struct ReducerCallResult {
    pub outcome: ReducerOutcome,
    pub energy_used: EnergyDiff,
    pub execution_duration: Duration,
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

pub struct UpdateOutcome {
    pub module_host: ModuleHost,
    pub update_result: UpdateDatabaseResult,
}

impl HostController {
    pub fn new(energy_monitor: Arc<impl EnergyMonitor>) -> Self {
        Self {
            modules: Mutex::new(HashMap::new()),
            threadpool: Arc::new(HostThreadpool::new()),
            energy_monitor,
        }
    }

    pub async fn init_module_host(
        &self,
        fence: u128,
        module_host_context: ModuleHostContext,
    ) -> Result<ModuleHost, anyhow::Error> {
        let module_host = self.spawn_module_host(module_host_context).await?;
        // TODO(cloutiertyler): Hook this up again
        // let identity = &module_host.info().identity;
        // let max_spend = worker_budget::max_tx_spend(identity);

        let rcr = module_host.init_database(fence, ReducerArgs::Nullary).await?;
        // worker_budget::record_tx_spend(identity, rcr.energy_quanta_used);
        rcr.outcome.into_result().context("init reducer failed")?;
        Ok(module_host)
    }

    pub async fn delete_module_host(
        &self,
        _fence: u128,
        worker_database_instance_id: u64,
    ) -> Result<(), anyhow::Error> {
        // TODO(kim): If the delete semantics are to wipe all state from
        // persistent storage, `_fence` is not needed. Otherwise, we will need
        // to check it against the stored value to be able to order deletes wrt
        // other lifecycle operations.
        //
        // Note that currently we don't delete the persistent state, but also
        // imply that a deleted database cannot be resurrected.
        if let Some(host) = self.take_module_host(worker_database_instance_id) {
            host.exit().await;
        }
        Ok(())
    }

    pub async fn update_module_host(
        &self,
        fence: u128,
        module_host_context: ModuleHostContext,
    ) -> Result<UpdateOutcome, anyhow::Error> {
        let module_host = self.spawn_module_host(module_host_context).await?;
        // TODO: see init_module_host
        let update_result = module_host.update_database(fence).await?;

        Ok(UpdateOutcome {
            module_host,
            update_result,
        })
    }

    pub async fn add_module_host(&self, module_host_context: ModuleHostContext) -> Result<ModuleHost, anyhow::Error> {
        let module_host = self.spawn_module_host(module_host_context).await?;
        // module_host.init_function(); ??
        Ok(module_host)
    }

    /// NOTE: Currently repeating reducers are only restarted when the [ModuleHost] is spawned.
    /// That means that if SpacetimeDB is restarted, repeating reducers will not be restarted unless
    /// there is a trigger that causes the [ModuleHost] to be spawned (e.g. a reducer is run).
    ///
    /// TODO(cloutiertyler): We need to determine what the correct behavior should be. In my mind,
    /// the repeating reducers for all [ModuleHost]s should be rescheduled on startup, with the overarching
    /// theory that SpacetimeDB should make a best effort to be as invisible as possible and not
    /// impact the logic of applications. The idea being that if SpacetimeDB is a distributed operating
    /// system, the applications will expect to be called when they are scheduled to be called regardless
    /// of whether the OS has been restarted.
    pub async fn spawn_module_host(&self, module_host_context: ModuleHostContext) -> Result<ModuleHost, anyhow::Error> {
        let key = module_host_context.dbic.database_instance_id;

        let (module_host, start_scheduler) = self.make_module_host(module_host_context)?;

        let old_module = self.modules.lock().insert(key, module_host.clone());
        if let Some(old_module) = old_module {
            old_module.exit().await
        }
        module_host.start();
        start_scheduler.start(&module_host)?;

        Ok(module_host)
    }

    fn make_module_host(&self, mhc: ModuleHostContext) -> anyhow::Result<(ModuleHost, SchedulerStarter)> {
        let module_hash = hash_bytes(&mhc.program_bytes);
        let (threadpool, energy_monitor) = (self.threadpool.clone(), self.energy_monitor.clone());
        let module_host = match mhc.host_type {
            HostType::Wasm => {
                // make_actor with block_in_place since it's going to take some time to compute.
                let start = Instant::now();
                let actor = tokio::task::block_in_place(|| {
                    host::wasmtime::make_actor(mhc.dbic, module_hash, &mhc.program_bytes, mhc.scheduler, energy_monitor)
                })?;
                log::trace!("wasmtime::make_actor blocked for {:?}", start.elapsed());
                ModuleHost::new(threadpool, actor)
            }
        };
        Ok((module_host, mhc.scheduler_starter))
    }

    /// Determine if the module host described by [`ModuleHostContext`] is
    /// managed by this host controller.
    ///
    /// Note that this method may report false negatives if the module host is
    /// currently being spawned via [`Self::spawn_module_host`].
    pub fn has_module_host(&self, module_host_context: &ModuleHostContext) -> bool {
        let key = &module_host_context.dbic.database_instance_id;
        self.modules.lock().contains_key(key)
    }

    /// Request a list of all describable entities in a module.
    pub fn catalog(&self, instance_id: u64) -> Result<Catalog, anyhow::Error> {
        let module_host = self.get_module_host(instance_id)?;
        Ok(module_host.catalog())
    }

    pub fn subscribe_to_logs(
        &self,
        instance_id: u64,
    ) -> anyhow::Result<tokio::sync::broadcast::Receiver<bytes::Bytes>> {
        Ok(self.get_module_host(instance_id)?.info().log_tx.subscribe())
    }
    pub fn get_module_host(&self, instance_id: u64) -> Result<ModuleHost, NoSuchModule> {
        let modules = self.modules.lock();
        modules.get(&instance_id).cloned().ok_or(NoSuchModule)
    }

    fn take_module_host(&self, instance_id: u64) -> Option<ModuleHost> {
        self.modules.lock().remove(&instance_id)
    }
}

impl Default for HostController {
    fn default() -> Self {
        Self::new(Arc::new(NullEnergyMonitor))
    }
}
