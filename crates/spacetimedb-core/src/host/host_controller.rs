use crate::hash::{hash_bytes, Hash};
use crate::host::host_wasmer;
use crate::host::module_host::ModuleHost;
use crate::protobuf::control_db::HostType;
use crate::worker_database_instance::WorkerDatabaseInstance;
use anyhow::{self, Context};
use futures::StreamExt;
use once_cell::sync::Lazy;
use serde::Serialize;
use spacetimedb_lib::EntityDef;
use std::fmt::{Display, Formatter};
use std::time::Duration;
use std::{collections::HashMap, sync::Mutex};
use tokio::sync::mpsc;
use tokio_util::time::DelayQueue;

use super::module_host::Catalog;
use super::timestamp::Timestamp;
use super::wasm_common::DEFAULT_EXECUTION_BUDGET;
use super::ReducerArgs;

pub static HOST: Lazy<HostController> = Lazy::new(HostController::new);

pub fn get_host() -> &'static HostController {
    &HOST
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

#[derive(Serialize, Clone, Debug)]
pub struct ReducerCallResult {
    pub committed: bool,
    pub budget_exceeded: bool,
    pub energy_quanta_used: i64,
    pub host_execution_duration: Duration,
}

impl HostController {
    fn new() -> Self {
        let modules = Mutex::new(HashMap::new());

        let (schedule_tx, schedule_rx) = mpsc::unbounded_channel();
        tokio::spawn(SchedulerActor::new(schedule_rx).run());
        let scheduler = Scheduler { tx: schedule_tx };
        Self { modules, scheduler }
    }

    pub async fn init_module(
        &self,
        worker_database_instance: WorkerDatabaseInstance,
        program_bytes: Vec<u8>,
    ) -> Result<ModuleHost, anyhow::Error> {
        let module_host = self.spawn_module(worker_database_instance, program_bytes).await?;
        // TODO(cloutiertyler): Hook this up again
        // let identity = &module_host.info().identity;
        // let max_spend = worker_budget::max_tx_spend(identity);
        let budget = ReducerBudget(DEFAULT_EXECUTION_BUDGET);

        let rcr = module_host.init_database(budget, ReducerArgs::Nullary).await?;
        // worker_budget::record_tx_spend(identity, rcr.energy_quanta_used);
        if let Some(rcr) = rcr {
            anyhow::ensure!(rcr.committed, "init reducer failed");
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
        worker_database_instance: WorkerDatabaseInstance,
        program_bytes: Vec<u8>,
    ) -> Result<ModuleHost, anyhow::Error> {
        let module_host = self.spawn_module(worker_database_instance, program_bytes).await?;
        module_host._migrate_database().await?;
        Ok(module_host)
    }

    pub async fn add_module(
        &self,
        worker_database_instance: WorkerDatabaseInstance,
        program_bytes: Vec<u8>,
    ) -> Result<ModuleHost, anyhow::Error> {
        let module_host = self.spawn_module(worker_database_instance, program_bytes).await?;
        // module_host.init_function(); ??
        Ok(module_host)
    }

    pub async fn spawn_module(
        &self,
        worker_database_instance: WorkerDatabaseInstance,
        program_bytes: Vec<u8>,
    ) -> Result<ModuleHost, anyhow::Error> {
        let module_hash = hash_bytes(&program_bytes);
        let key = worker_database_instance.database_instance_id;
        let module_host = self.take_module(key);
        if let Some(module_host) = module_host {
            module_host.exit().await?;
        }

        let scheduler = self.scheduler.clone();
        let module_host = tokio::task::spawn_blocking(move || {
            anyhow::Ok(match worker_database_instance.host_type {
                HostType::Wasmer => ModuleHost::spawn(host_wasmer::make_actor(
                    worker_database_instance,
                    module_hash,
                    program_bytes,
                    scheduler,
                )?),
            })
        })
        .await
        .unwrap()?;

        let mut modules = self.modules.lock().unwrap();
        modules.insert(key, module_host.clone());

        Ok(module_host)
    }

    pub async fn call_reducer(
        &self,
        instance_id: u64,
        caller_identity: Hash,
        reducer_name: &str,
        args: ReducerArgs,
    ) -> Result<Option<ReducerCallResult>, anyhow::Error> {
        let module_host = self.get_module(instance_id)?;
        Self::_call_reducer(module_host, caller_identity, reducer_name.into(), args).await
    }
    async fn _call_reducer(
        module_host: ModuleHost,
        caller_identity: Hash,
        reducer_name: String,
        args: ReducerArgs,
    ) -> Result<Option<ReducerCallResult>, anyhow::Error> {
        // TODO(cloutiertyler): Move this outside of the host controller
        // let max_spend = worker_budget::max_tx_spend(&module_host.identity);
        // let budget = ReducerBudget(max_spend);
        let budget = ReducerBudget(DEFAULT_EXECUTION_BUDGET);

        let res = module_host
            .call_reducer(caller_identity, reducer_name.into(), budget, args)
            .await;
        // TODO(cloutiertyler): Move this outside of the host controller
        // if let Ok(Some(rcr)) = &res {
        //     worker_budget::record_tx_spend(identity, rcr.energy_quanta_used);
        // }
        res
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

    pub fn get_module(&self, instance_id: u64) -> Result<ModuleHost, anyhow::Error> {
        let modules = self.modules.lock().unwrap();
        modules.get(&instance_id).cloned().context("No such module found.")
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

    pub fn schedule(&self, instance_id: u64, reducer: String, args: ReducerArgs, at: Timestamp) {
        let reducer = ScheduledReducer {
            instance_id,
            reducer,
            args,
        };
        self.tx
            .send(SchedulerMessage::Schedule(reducer, at))
            .unwrap_or_else(|_| panic!("scheduler actor panicked"))
    }
}

enum SchedulerMessage {
    Schedule(ScheduledReducer, Timestamp),
}

struct ScheduledReducer {
    instance_id: u64,
    reducer: String,
    args: ReducerArgs,
}

struct SchedulerActor {
    rx: mpsc::UnboundedReceiver<SchedulerMessage>,
    queue: DelayQueue<ScheduledReducer>,
}

impl SchedulerActor {
    fn new(rx: mpsc::UnboundedReceiver<SchedulerMessage>) -> Self {
        let queue = DelayQueue::new();
        Self { rx, queue }
    }
    async fn run(mut self) {
        let controller = get_host();
        loop {
            tokio::select! {
                Some(msg) = self.rx.recv() => self.handle_message(msg),
                Some(scheduled) = self.queue.next() => Self::handle_queued(controller, scheduled.into_inner()),
                else => break,
            }
        }
    }
    fn handle_message(&mut self, msg: SchedulerMessage) {
        match msg {
            SchedulerMessage::Schedule(reducer, time) => {
                self.queue.insert(reducer, time.to_duration_from_now());
            }
        }
    }
    fn handle_queued(controller: &HostController, scheduled: ScheduledReducer) {
        let Ok(module_host) = controller.get_module(scheduled.instance_id) else { return };
        tokio::spawn(async move {
            let identity = module_host.info().identity;
            // TODO: pass a logical "now" timestamp to this reducer call, but there's some
            //       intricacies to get right (how much drift to tolerate? what kind of tokio::time::MissedTickBehavior do we want?)
            let res = HostController::_call_reducer(module_host, identity, scheduled.reducer, scheduled.args).await;
            match res {
                Ok(Some(_)) => {}
                Ok(None) => log::error!("scheduled reducer doesn't exist?"),
                Err(e) => log::error!("invoking scheduled reducer failed: {e:#}"),
            }
        });
        // self.rep
    }
}
