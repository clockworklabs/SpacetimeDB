use super::worker_database_instance::WorkerDatabaseInstance;
use crate::hash::{hash_bytes, Hash};
use crate::nodes::worker_node::host_cpython::make_cpython_module_host_actor;
use crate::nodes::worker_node::host_wasm32::make_wasm32_module_host_actor;
use crate::nodes::worker_node::module_host::ModuleHost;
use crate::nodes::HostType;
use anyhow;
use lazy_static::lazy_static;
use serde::Serialize;
use spacetimedb_bindings::TupleDef;
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

#[derive(Serialize)]
pub struct ReducerDescription {
    reducer : String,
    arguments : TupleDef
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

    pub async fn call_reducer(
        &self,
        instance_id: u64,
        caller_identity: Hash,
        reducer_name: &str,
        arg_bytes: impl AsRef<[u8]>,
    ) -> Result<(), anyhow::Error> {
        let key = instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules
                .get(&key)
                .ok_or(anyhow::anyhow!("No such module found."))?
                .clone()
        };
        module_host
            .call_reducer(caller_identity, reducer_name.into(), arg_bytes.as_ref().to_vec())
            .await?;
        Ok(())
    }

    pub async fn describe_reducer(&self, instance_id: u64, reducer_name: &str) -> Result<ReducerDescription, anyhow::Error> {
        let key = instance_id;
        let module_host = {
            let modules = self.modules.lock().unwrap();
            modules
                .get(&key)
                .ok_or(anyhow::anyhow!("No such module found."))?
                .clone()
        };
        let arguments = module_host.describe_reducer(reducer_name.into()).await?;

        Ok(ReducerDescription{
            reducer: reducer_name.to_string(),
            arguments
        })
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
