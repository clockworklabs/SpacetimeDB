use crate::hash::{hash_bytes, Hash};
use crate::nodes::worker_node::host::host_cpython::make_cpython_module_host_actor;
use crate::nodes::worker_node::host::host_wasm32::make_wasm32_module_host_actor;
use crate::nodes::worker_node::host::module_host::ModuleHost;
use crate::nodes::worker_node::worker_budget;
use crate::nodes::worker_node::worker_database_instance::WorkerDatabaseInstance;
use crate::nodes::HostType;
use anyhow;
use lazy_static::lazy_static;
use serde::Serialize;
use spacetimedb_lib::TupleDef;
use std::fmt::{Display, Formatter};
use std::time::Duration;
use std::{collections::HashMap, sync::Mutex};

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

#[derive(Serialize, PartialEq, Eq, Hash, Clone, Debug)]
pub struct Entity {
    pub entity_name: String,
    pub entity_type: DescribedEntityType,
}

#[derive(Serialize, Clone, Debug)]
pub struct EntityDescription {
    pub entity: Entity,
    pub schema: TupleDef,
}

impl DescribedEntityType {
    pub fn as_str(&self) -> &str {
        match self {
            DescribedEntityType::Table => "table",
            DescribedEntityType::Reducer => "reducer",
            DescribedEntityType::RepeatingReducer => "repeater",
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
            modules.get(&key).map(|x| x.clone())
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
            modules.get(&key).map(|x| x.clone())
        };
        if let Some(module_host) = module_host {
            module_host.exit().await?;
        }

        let module_host = match worker_database_instance.host_type {
            HostType::WASM32 => make_wasm32_module_host_actor(worker_database_instance, module_hash, program_bytes)?,
            HostType::CPYTHON => make_cpython_module_host_actor(worker_database_instance, module_hash, program_bytes)?,
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
                .ok_or(anyhow::anyhow!("No such module found."))?
                .clone()
        };
        Ok(module_host)
    }

    pub async fn call_reducer(
        &self,
        instance_id: u64,
        caller_identity: Hash,
        reducer_name: &str,
        arg_bytes: impl AsRef<[u8]>,
    ) -> Result<ReducerCallResult, anyhow::Error> {
        let module_host = self.module_host(instance_id)?;
        let max_spend = worker_budget::max_tx_spend(&module_host.identity);
        let budget = ReducerBudget(max_spend);

        let result = module_host
            .call_reducer(
                caller_identity,
                reducer_name.into(),
                budget,
                arg_bytes.as_ref().to_vec(),
            )
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
    pub async fn describe(&self, instance_id: u64, entity: Entity) -> Result<Option<EntityDescription>, anyhow::Error> {
        let module_host = self.module_host(instance_id)?;
        let schema = module_host.describe(entity.clone()).await.unwrap();

        Ok(schema.map(|schema| EntityDescription { entity, schema }))
    }

    /// Request a list of all describable entities in a module.
    pub async fn catalog(&self, instance_id: u64) -> Result<Vec<EntityDescription>, anyhow::Error> {
        let module_host = self.module_host(instance_id)?;
        let catalog = module_host.catalog().await.unwrap();
        Ok(catalog)
    }

    pub fn get_module(&self, instance_id: u64) -> Result<ModuleHost, anyhow::Error> {
        let key = instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules
                .get(&key)
                .ok_or(anyhow::anyhow!("No such module found."))?
                .clone()
        };
        Ok(module_host)
    }
}
