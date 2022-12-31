use crate::hash::{hash_bytes, Hash};
use crate::nodes::worker_node::host::host_wasmer::make_wasmer_module_host_actor;
use crate::nodes::worker_node::host::module_host::ModuleHost;
use crate::nodes::worker_node::worker_budget;
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;
use crate::protobuf::control_db::HostType;
use anyhow::{self, Context};
use bytes::Bytes;
use lazy_static::lazy_static;
use serde::Serialize;
use spacetimedb_lib::{EntityDef, ReducerDef, TupleValue};
use std::fmt::{Display, Formatter};
use std::time::Duration;
use std::{collections::HashMap, sync::Mutex};
use thiserror::Error;

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

/// Returned from call_reducer if the reducer does not exist.
#[derive(Error, Debug, Clone)]
pub enum ReducerError {
    #[error("Reducer not found: {0}")]
    NotFound(String),
    #[error("Invalid arguments for reducer")]
    InvalidArgs,
}

#[derive(Debug)]
pub enum ReducerArgs {
    Json(Bytes),
}

impl ReducerArgs {
    pub(super) fn into_tuple(self, schema: &ReducerDef) -> anyhow::Result<TupleValue> {
        match self {
            ReducerArgs::Json(json) => {
                use serde::de::DeserializeSeed;
                let mut de = serde_json::Deserializer::from_slice(&json);
                let args = schema.deserialize(&mut de).context(ReducerError::InvalidArgs)?;
                de.end()?;
                Ok(args)
            }
        }
    }
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
    ) -> Result<Hash, anyhow::Error> {
        let key = worker_database_instance.database_instance_id;
        let module_hash = self.spawn_module(worker_database_instance, program_bytes).await?;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).unwrap().clone()
        };
        module_host.init_database().await?;
        module_host.start_repeating_reducers().await?;
        Ok(module_hash)
    }

    pub async fn delete_module(&self, worker_database_instance_id: u64) -> Result<(), anyhow::Error> {
        let key = worker_database_instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).cloned()
        };
        if let Some(module_host) = module_host {
            module_host.delete_database().await?;
        }
        let mut modules = self.modules.lock().unwrap();
        modules.remove(&key);
        Ok(())
    }

    pub async fn _update_module(
        &self,
        worker_database_instance: WorkerDatabaseInstance,
        program_bytes: Vec<u8>,
    ) -> Result<Hash, anyhow::Error> {
        let key = worker_database_instance.database_instance_id;
        let module_hash = self.spawn_module(worker_database_instance, program_bytes).await?;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).unwrap().clone()
        };
        module_host._migrate_database().await?;
        module_host.start_repeating_reducers().await?;
        Ok(module_hash)
    }

    pub async fn add_module(
        &self,
        worker_database_instance: WorkerDatabaseInstance,
        program_bytes: Vec<u8>,
    ) -> Result<Hash, anyhow::Error> {
        let key = worker_database_instance.database_instance_id;
        let module_hash = self.spawn_module(worker_database_instance, program_bytes).await?;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).unwrap().clone()
        };
        module_host.start_repeating_reducers().await?;
        Ok(module_hash)
    }

    pub async fn spawn_module(
        &self,
        worker_database_instance: WorkerDatabaseInstance,
        program_bytes: Vec<u8>,
    ) -> Result<Hash, anyhow::Error> {
        let module_hash = hash_bytes(&program_bytes);
        let key = worker_database_instance.database_instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules.get(&key).cloned()
        };
        if let Some(module_host) = module_host {
            module_host.exit().await?;
        }

        let trace_log = worker_database_instance.trace_log;
        let module_host = match worker_database_instance.host_type {
            HostType::Wasmer => {
                make_wasmer_module_host_actor(worker_database_instance, module_hash, program_bytes, trace_log)?
            }
        };

        let mut modules = self.modules.lock().unwrap();
        modules.insert(key, module_host);

        Ok(module_hash)
    }

    fn module_host(&self, instance_id: u64) -> Result<ModuleHost, anyhow::Error> {
        let key = instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules
                .get(&key)
                .ok_or_else(|| anyhow::anyhow!("No such module found."))?
                .clone()
        };
        Ok(module_host)
    }

    pub async fn call_reducer(
        &self,
        instance_id: u64,
        caller_identity: Hash,
        reducer_name: &str,
        args: ReducerArgs,
    ) -> Result<ReducerCallResult, anyhow::Error> {
        let module_host = self.module_host(instance_id)?;
        let max_spend = worker_budget::max_tx_spend(&module_host.identity);
        let budget = ReducerBudget(max_spend);

        let result = module_host
            .call_reducer(caller_identity, reducer_name.into(), budget, args)
            .await;
        match result {
            Ok(rcr) => {
                worker_budget::record_tx_spend(&module_host.identity, rcr.energy_quanta_used);
                Ok(rcr)
            }
            Err(e) => Err(e),
        }
    }

    /// Describe a specific entity in a module.
    /// None if not present.
    pub async fn describe(&self, instance_id: u64, entity_name: String) -> Result<Option<EntityDef>, anyhow::Error> {
        let module_host = self.module_host(instance_id)?;
        let schema = module_host.describe(entity_name).await.unwrap();

        Ok(schema)
    }

    /// Request a list of all describable entities in a module.
    pub async fn catalog(&self, instance_id: u64) -> Result<Vec<(String, EntityDef)>, anyhow::Error> {
        let module_host = self.module_host(instance_id)?;
        let catalog = module_host.catalog().await.unwrap();
        Ok(catalog)
    }

    /// If a module's DB activity is being traced (for diagnostics etc.), retrieves the current contents of its trace stream.
    #[cfg(feature = "tracelogging")]
    pub async fn get_trace(&self, instance_id: u64) -> Result<Option<bytes::Bytes>, anyhow::Error> {
        let module_host = self.module_host(instance_id)?;
        let trace = module_host.get_trace().await.unwrap();
        Ok(trace)
    }

    /// If a module's DB activity is being traced (for diagnostics etc.), stop tracing it.
    #[cfg(feature = "tracelogging")]
    pub async fn stop_trace(&self, instance_id: u64) -> Result<(), anyhow::Error> {
        let module_host = self.module_host(instance_id)?;
        module_host.stop_trace().await.unwrap();
        Ok(())
    }

    pub fn get_module(&self, instance_id: u64) -> Result<ModuleHost, anyhow::Error> {
        let key = instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules
                .get(&key)
                .ok_or_else(|| anyhow::anyhow!("No such module found."))?
                .clone()
        };
        Ok(module_host)
    }
}
