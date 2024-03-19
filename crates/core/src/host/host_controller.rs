use crate::db::update::UpdateDatabaseError;
use crate::energy::{EnergyMonitor, EnergyQuanta, NullEnergyMonitor};
use crate::execution_context::ExecutionContext;
use crate::hash::hash_bytes;
use crate::host;
use crate::messages::control_db::HostType;
use crate::module_host_context::{ModuleCreationContext, ModuleHostContext};
use crate::util::spawn_rayon;
use anyhow::ensure;
use futures::TryFutureExt;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::module_host::{Catalog, EntityDef, EventStatus, ModuleHost, NoSuchModule, UpdateDatabaseResult};
use super::ReducerArgs;

pub struct HostController {
    modules: Mutex<HashMap<u64, ModuleHost>>,
    pub energy_monitor: Arc<dyn EnergyMonitor>,
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
    pub fn new(energy_monitor: Arc<impl EnergyMonitor>) -> Self {
        Self {
            modules: Mutex::new(HashMap::new()),
            energy_monitor,
        }
    }

    /// Initialize a module and underlying database.
    ///
    /// This will call the `init` reducer of the supplied program and set the
    /// program as the database's program if it succeeds (or no `init` reducer
    /// is defined).
    ///
    /// The method is executed in a transaction: if an error occurs (including
    /// the `init` reducer failing), the module will not be in an initialized
    /// state and the database will not have a program set..
    ///
    /// The result of calling the `init` reducer is returned as `Some` (or `None`
    /// if the module does not define an `init` reducer).
    ///
    /// Note that callers may want to scrutinize the [`ReducerOutcome`] contained
    /// in the [`ReducerCallResult`] in order to decide if the module was
    /// indeed initialized successfully.
    ///
    /// The reason this error case is nested is that some callers may want to
    /// report it instead of short-circuiting using `?`, consistent with
    /// [`Self::update_module_host`].
    pub async fn init_module_host(
        &self,
        fence: u128,
        module_host_context: ModuleHostContext,
    ) -> Result<Option<ReducerCallResult>, anyhow::Error> {
        self.setup_module_host(module_host_context, |module_host| async move {
            // TODO(cloutiertyler): Hook this up again
            // let identity = &module_host.info().identity;
            // let max_spend = worker_budget::max_tx_spend(identity);
            let maybe_init_call_result = module_host.init_database(fence, ReducerArgs::Nullary).await?;
            Ok(maybe_init_call_result)
        })
        .await
    }

    /// Exit and delete the module corresponding to the provided instance id.
    ///
    /// Note that this currently does not delete any physical data.
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

    /// Update an existing module and database with the supplied program.
    ///
    /// This will call the `update` reducer of the supplied program and set the
    /// program as the database's program if it succeeds (or no `update` reducer
    /// is defined).
    ///
    /// The method is executed in a transaction: if an error occurs (including
    /// the `update` reducer failing), the module and database will not be
    /// updated, and the previous instance will keep running.
    ///
    /// The result of calling the `update` reducer is returned in
    /// [`UpdateDatabaseResult`], which itself is a `Result`. Callers may choose
    /// to short-circuit it using `?`, or report the outcome in a different way.
    pub async fn update_module_host(
        &self,
        fence: u128,
        module_host_context: ModuleHostContext,
    ) -> Result<UpdateDatabaseResult, anyhow::Error> {
        self.setup_module_host(module_host_context, |module_host| async move {
            let update_result = module_host.update_database(fence).await?;
            // Turn UpdateDatabaseError into anyhow::Error, so the module gets
            // discarded.
            update_result.map_err(Into::into)
        })
        .await
        // Extract UpdateDatabaseError again, so we can return Ok(Err(..)), and
        // Err for any other error.
        .map(Ok)
        .or_else(|e| e.downcast::<UpdateDatabaseError>().map(Err))
    }

    /// Spawn the given module host.
    ///
    /// The supplied program must match the program stored in the database,
    /// otherwise an error is returned.
    ///
    /// NOTE: Currently repeating reducers are only restarted when the [ModuleHost] is spawned.
    /// That means that if SpacetimeDB is restarted, repeating reducers will not be restarted unless
    /// there is a trigger that causes the [ModuleHost] to be spawned (e.g. a reducer is run).
    pub async fn spawn_module_host(&self, mhc: ModuleHostContext) -> Result<ModuleHost, anyhow::Error> {
        // TODO(cloutiertyler): We need to determine what the correct behavior should be. In my mind,
        // the repeating reducers for all [ModuleHost]s should be rescheduled on startup, with the overarching
        // theory that SpacetimeDB should make a best effort to be as invisible as possible and not
        // impact the logic of applications. The idea being that if SpacetimeDB is a distributed operating
        // system, the applications will expect to be called when they are scheduled to be called regardless
        // of whether the OS has been restarted.
        self.setup_module_host(mhc, |mh| async { Ok(mh) }).await
    }

    /// Set up the [`ModuleHost`] described by the provided [`ModuleHostContext`],
    /// and run the closure `f` over it.
    ///
    /// If `F` returns an `Ok` result, the module host is registered with this
    /// controller (i.e. [`Self::has_module_host`] returns true), and its
    /// reducer scheduler is started.
    ///
    /// Otherwise, if `F` returns an `Err` result, the module is discarded.
    ///
    /// In the `Err` case, `F` **MUST** roll back any modifications it has made
    /// to the database passed in the [`ModuleHostContext`].
    async fn setup_module_host<F, Fut, T>(&self, mhc: ModuleHostContext, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(ModuleHost) -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        let key = mhc.dbic.database_instance_id;

        let program_hash = hash_bytes(&mhc.program_bytes);
        let mcc = ModuleCreationContext {
            dbic: mhc.dbic.clone(),
            scheduler: mhc.scheduler,
            program_hash,
            program_bytes: mhc.program_bytes,
            energy_monitor: self.energy_monitor.clone(),
        };
        let module_host = spawn_rayon(move || Self::make_module_host(mhc.host_type, mcc)).await?;
        module_host.start();

        let res = f(module_host.clone())
            .or_else(|e| async {
                module_host.exit().await;
                Err(e)
            })
            .await?;

        let guard_program_hash = || {
            let db = &mhc.dbic.relational_db;
            let db_program_hash = db.with_read_only(&ExecutionContext::default(), |tx| db.program_hash(tx))?;
            ensure!(
                Some(program_hash) == db_program_hash,
                "supplied program {} does not match database program {:?}",
                program_hash,
                db_program_hash,
            );

            Ok(())
        };
        let old_module = {
            let mut modules = self.modules.lock();
            // At this point, the supplied program must be the program stored in
            // the running database. Assert that this is the case.
            //
            // It is unfortunate that [`Self::spawn_module_host`] is both public
            // and takes the module's program bytes in its argument. Because it
            // can be called concurrently to [`Self::init_module_host`] or
            // [`Self::update_module_host`], we may end up with the wrong module
            // version here.
            //
            // We should instead store the current program bytes verbatim in the
            // database, such that `spawn_module_host` operates on the committed
            // state only.
            if let Err(e) = guard_program_hash() {
                drop(modules);
                module_host.exit().await;
                return Err(e);
            }

            modules.insert(key, module_host.clone())
        };
        if let Some(old_module) = old_module {
            old_module.exit().await;
        }
        mhc.scheduler_starter.start(&module_host)?;

        Ok(res)
    }

    /// Set up the actual module host.
    ///
    /// This is a fairly expensive operation and should not be run on the async
    /// task threadpool.
    ///
    /// Note that this function **MUST NOT** make any modifications to the
    /// database passed in as part of the [`ModuleCreationContext`].
    fn make_module_host(host_type: HostType, mcc: ModuleCreationContext) -> anyhow::Result<ModuleHost> {
        let module_host = match host_type {
            HostType::Wasm => {
                let start = Instant::now();
                let actor = host::wasmtime::make_actor(mcc)?;
                log::trace!("wasmtime::make_actor blocked for {:?}", start.elapsed());
                ModuleHost::new(actor)
            }
        };
        Ok(module_host)
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
