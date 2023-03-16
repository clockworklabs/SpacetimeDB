use crate::hash::hash_bytes;
use crate::host::host_wasmer;
use crate::host::module_host::ModuleHost;
use crate::identity::Identity;
use crate::protobuf::control_db::HostType;
use crate::worker_database_instance::WorkerDatabaseInstance;
use futures::{stream, StreamExt, TryStreamExt};
use once_cell::sync::OnceCell;
use serde::Serialize;
use sled::transaction::ConflictableTransactionError::Abort as TxAbort;
use spacetimedb_lib::{bsatn, EntityDef};
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::time::DelayQueue;

use super::module_host::{Catalog, EventStatus, NoSuchModule, ReducerCallError};
use super::timestamp::Timestamp;
use super::wasm_common::DEFAULT_EXECUTION_BUDGET;
use super::ReducerArgs;

static HOST: OnceCell<HostController> = OnceCell::new();

pub async fn init(f: &impl DbGetter) -> anyhow::Result<()> {
    let (mut scheduler_actor, scheduler) = SchedulerActor::new()?;
    let modules = scheduler_actor.populate(&scheduler, f).await?.into();
    let host = HOST.try_insert(HostController { modules, scheduler }).ok().unwrap();
    tokio::spawn(scheduler_actor.run(host));
    Ok(())
}

pub fn init_basic() -> anyhow::Result<()> {
    let (scheduler_actor, scheduler) = SchedulerActor::new()?;
    let modules = Mutex::default();
    let host = HOST.try_insert(HostController { modules, scheduler }).ok().unwrap();
    tokio::spawn(scheduler_actor.run(host));
    Ok(())
}

#[inline]
pub fn get() -> &'static HostController {
    HOST.get().unwrap()
}

pub struct HostController {
    modules: Mutex<HashMap<u64, ModuleHost>>,
    scheduler: Scheduler,
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
impl Display for DescribedEntityType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ReducerBudget(pub i64 /* maximum spend for this call */);

impl Default for ReducerBudget {
    fn default() -> Self {
        Self(DEFAULT_EXECUTION_BUDGET)
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ReducerCallResult {
    pub outcome: ReducerOutcome,
    pub energy_quanta_used: i64,
    pub host_execution_duration: Duration,
}

#[derive(Serialize, Clone, Debug)]
pub enum ReducerOutcome {
    Committed,
    Failed(String),
    BudgetExceeded,
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
    pub async fn init_module(
        &self,
        worker_database_instance: Arc<WorkerDatabaseInstance>,
        program_bytes: impl AsRef<[u8]> + Send + 'static,
    ) -> Result<ModuleHost, anyhow::Error> {
        let module_host = self.spawn_module(worker_database_instance, program_bytes).await?;
        // TODO(cloutiertyler): Hook this up again
        // let identity = &module_host.info().identity;
        // let max_spend = worker_budget::max_tx_spend(identity);
        let budget = ReducerBudget(DEFAULT_EXECUTION_BUDGET);

        let rcr = module_host.init_database(budget, ReducerArgs::Nullary).await?;
        // worker_budget::record_tx_spend(identity, rcr.energy_quanta_used);
        if let Some(rcr) = rcr {
            match rcr.outcome {
                ReducerOutcome::Committed => {}
                ReducerOutcome::Failed(err) => anyhow::bail!("init reducer failed: {err}"),
                ReducerOutcome::BudgetExceeded => anyhow::bail!("init reducer ran out of energy"),
            }
        }
        Ok(module_host)
    }

    pub async fn delete_module(&self, worker_database_instance_id: u64) -> Result<(), anyhow::Error> {
        let module_host = self.take_module(worker_database_instance_id);
        if let Some(module_host) = module_host {
            module_host.delete_database().await?;
        }
        Ok(())
    }

    pub async fn _update_module(
        &self,
        worker_database_instance: Arc<WorkerDatabaseInstance>,
        program_bytes: impl AsRef<[u8]> + Send + 'static,
    ) -> Result<ModuleHost, anyhow::Error> {
        let module_host = self.spawn_module(worker_database_instance, program_bytes).await?;
        module_host._migrate_database().await?;
        Ok(module_host)
    }

    pub async fn add_module(
        &self,
        worker_database_instance: Arc<WorkerDatabaseInstance>,
        program_bytes: impl AsRef<[u8]> + Send + 'static,
    ) -> Result<ModuleHost, anyhow::Error> {
        let module_host = self.spawn_module(worker_database_instance, program_bytes).await?;
        // module_host.init_function(); ??
        Ok(module_host)
    }

    pub async fn spawn_module(
        &self,
        worker_database_instance: Arc<WorkerDatabaseInstance>,
        program_bytes: impl AsRef<[u8]> + Send + 'static,
    ) -> Result<ModuleHost, anyhow::Error> {
        let key = worker_database_instance.database_instance_id;
        let module_host = self.take_module(key);
        if let Some(module_host) = module_host {
            module_host.exit().await;
        }

        let module_host = Self::make_module(self.scheduler.clone(), worker_database_instance, program_bytes).await?;

        let mut modules = self.modules.lock().unwrap();
        modules.insert(key, module_host.clone());

        Ok(module_host)
    }

    async fn make_module(
        scheduler: Scheduler,
        worker_database_instance: Arc<WorkerDatabaseInstance>,
        program_bytes: impl AsRef<[u8]> + Send + 'static,
    ) -> anyhow::Result<ModuleHost> {
        let module_hash = hash_bytes(&program_bytes);
        tokio::task::spawn_blocking(move || {
            anyhow::Ok(match worker_database_instance.host_type {
                HostType::Wasmer => ModuleHost::spawn(host_wasmer::make_actor(
                    worker_database_instance,
                    module_hash,
                    program_bytes.as_ref(),
                    scheduler,
                )?),
            })
        })
        .await
        .unwrap()
    }

    pub async fn call_reducer(
        &self,
        instance_id: u64,
        caller_identity: Identity,
        reducer_name: &str,
        args: ReducerArgs,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        let module_host = self.get_module(instance_id)?;
        Self::call_reducer_inner(module_host, caller_identity, reducer_name.into(), args).await
    }

    async fn call_reducer_inner(
        module_host: ModuleHost,
        caller_identity: Identity,
        reducer_name: String,
        args: ReducerArgs,
    ) -> Result<ReducerCallResult, ReducerCallError> {
        // TODO(cloutiertyler): Move this outside of the host controller
        // let max_spend = worker_budget::max_tx_spend(&module_host.identity);
        // let budget = ReducerBudget(max_spend);
        let budget = ReducerBudget(DEFAULT_EXECUTION_BUDGET);

        module_host
            .call_reducer(caller_identity, reducer_name, budget, args)
            .await
        // TODO(cloutiertyler): Move this outside of the host controller
        // if let Ok(Some(rcr)) = &res {
        //     worker_budget::record_tx_spend(identity, rcr.energy_quanta_used);
        // }
    }

    /// Request a list of all describable entities in a module.
    pub fn catalog(&self, instance_id: u64) -> Result<Catalog, anyhow::Error> {
        let module_host = self.get_module(instance_id)?;
        Ok(module_host.catalog())
    }

    pub fn subscribe_to_logs(
        &self,
        instance_id: u64,
    ) -> anyhow::Result<tokio::sync::broadcast::Receiver<bytes::Bytes>> {
        Ok(self.get_module(instance_id)?.info().log_tx.subscribe())
    }

    /// If a module's DB activity is being traced (for diagnostics etc.), retrieves the current contents of its trace stream.
    #[cfg(feature = "tracelogging")]
    pub async fn get_trace(&self, instance_id: u64) -> Result<Option<bytes::Bytes>, anyhow::Error> {
        let module_host = self.get_module(instance_id)?;
        let trace = module_host.get_trace().await.unwrap();
        Ok(trace)
    }

    /// If a module's DB activity is being traced (for diagnostics etc.), stop tracing it.
    #[cfg(feature = "tracelogging")]
    pub async fn stop_trace(&self, instance_id: u64) -> Result<(), anyhow::Error> {
        let module_host = self.get_module(instance_id)?;
        module_host.stop_trace().await.unwrap();
        Ok(())
    }

    pub fn get_module(&self, instance_id: u64) -> Result<ModuleHost, NoSuchModule> {
        let modules = self.modules.lock().unwrap();
        modules.get(&instance_id).cloned().ok_or(NoSuchModule)
    }

    fn take_module(&self, instance_id: u64) -> Option<ModuleHost> {
        self.modules.lock().unwrap().remove(&instance_id)
    }
}

#[derive(Clone)]
pub struct Scheduler {
    tx: mpsc::UnboundedSender<SchedulerMessage>,
}

impl Scheduler {
    pub fn dummy() -> Self {
        let (tx, _) = mpsc::unbounded_channel();
        Self { tx }
    }

    pub fn schedule(&self, db_id: u64, instance_id: u64, reducer: String, bsatn_args: Vec<u8>, at: Timestamp) {
        let reducer = ScheduledReducer {
            at,
            database_id: db_id,
            instance_id,
            reducer,
            bsatn_args,
        };
        self.tx
            .send(SchedulerMessage::Schedule(reducer))
            .unwrap_or_else(|_| panic!("scheduler actor panicked"))
    }
}

enum SchedulerMessage {
    Schedule(ScheduledReducer),
}

#[derive(spacetimedb_sats::ser::Serialize, spacetimedb_sats::de::Deserialize)]
struct ScheduledReducer {
    at: Timestamp,
    database_id: u64,
    instance_id: u64,
    reducer: String,
    bsatn_args: Vec<u8>,
}

struct SchedulerActor {
    rx: mpsc::UnboundedReceiver<SchedulerMessage>,
    queue: DelayQueue<u64>,
    db: sled::Db,
}

#[async_trait::async_trait]
pub trait DbGetter {
    type ProgramBytes: AsRef<[u8]> + Send + 'static;
    async fn load_db_instance(
        &self,
        db_id: u64,
        instance_id: u64,
    ) -> anyhow::Result<(Arc<WorkerDatabaseInstance>, Self::ProgramBytes)>;
}

impl SchedulerActor {
    fn new() -> anyhow::Result<(Self, Scheduler)> {
        let (tx, rx) = mpsc::unbounded_channel();
        let scheduler = Scheduler { tx };

        let queue = DelayQueue::new();
        let db = sled::Config::default()
            .path(crate::stdb_path("schedule"))
            .flush_every_ms(Some(50))
            .mode(sled::Mode::HighThroughput)
            .open()?;

        Ok((Self { rx, queue, db }, scheduler))
    }
    async fn populate(
        &mut self,
        sched_ref: &Scheduler,
        get_db: &impl DbGetter,
    ) -> anyhow::Result<HashMap<u64, ModuleHost>> {
        let mut insts_to_spawn = HashSet::new();
        for entry in self.db.iter() {
            let (k, v) = entry?;
            let get_u64 = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap());
            let id = get_u64(&k);
            let at = Timestamp(get_u64(&v[..8]));
            let db_id = get_u64(&v[8..16]);
            let inst_id = get_u64(&v[16..24]);
            insts_to_spawn.insert((db_id, inst_id));
            self.queue.insert(id, at.to_duration_from_now());
        }
        let modules = insts_to_spawn.into_iter().map(|(db_id, inst_id)| async move {
            let (wdi, program_bytes) = get_db.load_db_instance(db_id, inst_id).await?;
            HostController::make_module(sched_ref.clone(), wdi, program_bytes)
                .await
                .map(|m| (inst_id, m))
        });
        let modules = stream::FuturesUnordered::from_iter(modules).try_collect().await?;
        Ok(modules)
    }
    async fn run(mut self, host: &'static HostController) {
        loop {
            tokio::select! {
                Some(msg) = self.rx.recv() => self.handle_message(msg),
                Some(scheduled) = self.queue.next() => self.handle_queued(host, scheduled.into_inner()),
                else => break,
            }
        }
    }
    fn handle_message(&mut self, msg: SchedulerMessage) {
        match msg {
            SchedulerMessage::Schedule(reducer) => {
                let at = reducer.at;
                let id = self
                    .db
                    .transaction(|tx| {
                        let id = tx.generate_id()?;
                        let reducer = bsatn::to_vec(&reducer).map_err(TxAbort)?;
                        tx.insert(&id.to_le_bytes(), reducer)?;
                        Ok(id)
                    })
                    .unwrap();
                self.queue.insert(id, at.to_duration_from_now());
            }
        }
    }
    fn handle_queued(&self, host: &HostController, scheduled: u64) {
        let Some(scheduled) = self.db.remove(scheduled.to_le_bytes()).unwrap() else { return };
        let scheduled: ScheduledReducer = bsatn::from_reader(&mut &scheduled[..]).unwrap();
        let Ok(module_host) = host.get_module(scheduled.instance_id) else { return };
        tokio::spawn(async move {
            let identity = module_host.info().identity;
            // TODO: pass a logical "now" timestamp to this reducer call, but there's some
            //       intricacies to get right (how much drift to tolerate? what kind of tokio::time::MissedTickBehavior do we want?)
            let res = HostController::call_reducer_inner(
                module_host,
                identity,
                scheduled.reducer,
                ReducerArgs::Bsatn(scheduled.bsatn_args.into()),
            )
            .await;
            match res {
                Ok(_) => {}
                Err(e) => log::error!("invoking scheduled reducer failed: {e:#}"),
            }
        });
        // self.rep
    }
}
