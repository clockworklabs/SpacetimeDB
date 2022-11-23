use crate::hash::{hash_bytes, Hash};
use crate::host::host_wasmer;
use crate::host::module_host::ModuleHost;
use crate::protobuf::control_db::HostType;
use crate::worker_database_instance::WorkerDatabaseInstance;
use anyhow::{self, Context};
use lazy_static::lazy_static;
use serde::Serialize;
use spacetimedb_lib::EntityDef;
use std::fmt::{Display, Formatter};
use std::time::Duration;
use std::{collections::HashMap, sync::Mutex};

use super::module_host::{Catalog, SpawnResult};
use super::ReducerArgs;

lazy_static! {
    pub static ref HOST: HostController = HostController::new();
}

pub fn get_host() -> &'static HostController {
    &HOST
}

pub struct HostController {
    modules: Mutex<HashMap<u64, ModuleHost>>,
}

#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Debug)]
pub enum DescribedEntityType {
    Table,
    Reducer,
    RepeatingReducer,
}

impl DescribedEntityType {
    pub fn as_str(self) -> &'static str {
        match self {
            DescribedEntityType::Table => "table",
            DescribedEntityType::Reducer => "reducer",
            DescribedEntityType::RepeatingReducer => "repeater",
        }
    }
    pub fn from_entitydef(def: &EntityDef) -> Self {
        match def {
            EntityDef::Table(_) => Self::Table,
            EntityDef::Reducer(_) => Self::Reducer,
            EntityDef::Repeater(_) => Self::RepeatingReducer,
        }
    }
}
impl std::str::FromStr for DescribedEntityType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "table" => Ok(DescribedEntityType::Table),
            "reducer" => Ok(DescribedEntityType::Reducer),
            "repeater" => Ok(DescribedEntityType::RepeatingReducer),
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
        Self { modules }
    }

    pub async fn init_module(
        &self,
        worker_database_instance: WorkerDatabaseInstance,
        program_bytes: Vec<u8>,
    ) -> Result<ModuleHost, anyhow::Error> {
        let mut res = self.spawn_module(worker_database_instance, program_bytes).await?;
        res.module_host.init_database().await?;
        res.start_repeating_reducers();
        Ok(res.module_host)
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
        let mut res = self.spawn_module(worker_database_instance, program_bytes).await?;
        res.module_host._migrate_database().await?;
        res.start_repeating_reducers();
        Ok(res.module_host)
    }

    pub async fn add_module(
        &self,
        worker_database_instance: WorkerDatabaseInstance,
        program_bytes: Vec<u8>,
    ) -> Result<ModuleHost, anyhow::Error> {
        let mut res = self.spawn_module(worker_database_instance, program_bytes).await?;
        res.start_repeating_reducers();
        Ok(res.module_host)
    }

    pub async fn spawn_module(
        &self,
        worker_database_instance: WorkerDatabaseInstance,
        program_bytes: Vec<u8>,
    ) -> Result<SpawnResult, anyhow::Error> {
        let module_hash = hash_bytes(&program_bytes);
        let key = worker_database_instance.database_instance_id;
        let module_host = self.take_module(key);
        if let Some(module_host) = module_host {
            module_host.exit().await?;
        }

        let res = match worker_database_instance.host_type {
            HostType::Wasmer => ModuleHost::spawn(host_wasmer::make_actor(
                worker_database_instance,
                module_hash,
                program_bytes,
            )?),
        };

        let mut modules = self.modules.lock().unwrap();
        modules.insert(key, res.module_host.clone());

        Ok(res)
    }

    pub async fn call_reducer(
        &self,
        instance_id: u64,
        caller_identity: Hash,
        reducer_name: &str,
        args: ReducerArgs,
    ) -> Result<Option<ReducerCallResult>, anyhow::Error> {
        let module_host = self.get_module(instance_id)?;
        // TODO(cloutiertyler): Move this outside of the host controller
        // let max_spend = worker_budget::max_tx_spend(&module_host.identity);
        // let budget = ReducerBudget(max_spend);
        let budget = ReducerBudget(1_000_000_000_000);

        let rcr = module_host
            .call_reducer(caller_identity, reducer_name.into(), budget, args)
            .await?;
        // TODO(cloutiertyler): Move this outside of the host controller
        // if let Some(rcr) = &rcr {
        //     worker_budget::record_tx_spend(identity, rcr.energy_quanta_used);
        // }
        Ok(rcr)
    }

    /// Request a list of all describable entities in a module.
    pub fn catalog(&self, instance_id: u64) -> Result<Catalog, anyhow::Error> {
        let module_host = self.get_module(instance_id)?;
        Ok(module_host.catalog())
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
